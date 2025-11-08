use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, error, info, warn};
use warp::Filter;
use futures_util::StreamExt;
use tokio_stream::wrappers::BroadcastStream;

#[derive(Parser)]
#[command(name = "light-gelf-collector")]
#[command(about = "A lightweight GELF log collector")]
struct Args {
    /// UDP port to listen for GELF messages
    #[arg(short, long, default_value = "12201")]
    udp_port: u16,

    /// HTTP port for the web service
    #[arg(short = 'H', long, default_value = "8080")]
    http_port: u16,

    /// Maximum number of log messages to keep in memory
    #[arg(short, long, default_value = "10000")]
    max_messages: usize,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0")]
    bind_address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GelfMessage {
    version: Option<String>,
    host: Option<String>,
    short_message: Option<String>,
    full_message: Option<String>,
    timestamp: Option<f64>,
    level: Option<u8>,
    facility: Option<String>,
    line: Option<u32>,
    file: Option<String>,
    #[serde(flatten)]
    additional_fields: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
struct StoredMessage {
    gelf_message: GelfMessage,
    received_at: f64,
    raw_message: String,
}

#[derive(Debug, Clone, Serialize)]
struct MessageResponse {
    #[serde(flatten)]
    gelf_message: GelfMessage,
    received_at: f64,
}

#[derive(Debug, Clone)]
struct LogStore {
    messages: Arc<RwLock<VecDeque<StoredMessage>>>,
    max_size: usize,
    broadcast_tx: broadcast::Sender<MessageResponse>,
}

impl LogStore {
    fn new(max_size: usize) -> Self {
        let (broadcast_tx, _) = broadcast::channel(100); // Buffer for 100 messages
        Self {
            messages: Arc::new(RwLock::new(VecDeque::new())),
            max_size,
            broadcast_tx,
        }
    }

    async fn add_message(&self, gelf_message: GelfMessage, raw_message: String) {
        let mut messages = self.messages.write().await;

        let stored_message = StoredMessage {
            gelf_message: gelf_message.clone(),
            received_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64(),
            raw_message,
        };

        let response = MessageResponse {
            gelf_message,
            received_at: stored_message.received_at,
        };

        messages.push_back(stored_message);

        // Clean up if we exceed max size
        while messages.len() > self.max_size {
            messages.pop_front();
        }

        // Broadcast the new message to subscribers (ignore if no subscribers)
        let _ = self.broadcast_tx.send(response);
    }

    fn subscribe(&self) -> broadcast::Receiver<MessageResponse> {
        self.broadcast_tx.subscribe()
    }

    async fn get_messages(&self, limit: Option<usize>) -> Vec<MessageResponse> {
        let messages = self.messages.read().await;
        let limit = limit.unwrap_or(messages.len());
        messages
            .iter()
            .rev()
            .take(limit)
            .map(|stored| MessageResponse {
                gelf_message: stored.gelf_message.clone(),
                received_at: stored.received_at,
            })
            .collect()
    }

    async fn get_stats(&self) -> serde_json::Value {
        let messages = self.messages.read().await;
        serde_json::json!({
            "total_messages": messages.len(),
            "max_capacity": self.max_size,
            "capacity_used_percent": (messages.len() as f64 / self.max_size as f64) * 100.0
        })
    }
}

async fn handle_udp_message(socket: Arc<UdpSocket>, store: LogStore) {
    let mut buf = vec![0; 8192];
    debug!(
        "Starting UDP message handler with buffer size: {}",
        buf.len()
    );

    loop {
        debug!("Waiting for UDP message...");
        match socket.recv_from(&mut buf).await {
            Ok((len, addr)) => {
                debug!("Received {} bytes from {}", len, addr);
                let raw_data = &buf[..len];

                // Log raw data information
                debug!(
                    "Raw data first 10 bytes: {:?}",
                    &raw_data[..std::cmp::min(10, raw_data.len())]
                );

                // Try to decompress if it's compressed
                let is_gzipped = raw_data.len() > 2 && raw_data[0] == 0x1f && raw_data[1] == 0x8b;
                let is_zlib = raw_data.len() > 2 && raw_data[0] == 0x78 && (raw_data[1] == 0x9c || raw_data[1] == 0xda || raw_data[1] == 0x01);
                
                let compression_type = if is_gzipped {
                    "GZIP"
                } else if is_zlib {
                    "ZLIB"
                } else {
                    "none"
                };
                
                debug!("Message compression detected: {}", compression_type);

                let message_str = if is_gzipped {
                    debug!("Attempting to decompress GZIP data...");
                    match decompress_gzip(raw_data) {
                        Ok(decompressed) => {
                            debug!(
                                "Successfully decompressed {} bytes to {} bytes",
                                raw_data.len(),
                                decompressed.len()
                            );
                            String::from_utf8_lossy(&decompressed).to_string()
                        }
                        Err(e) => {
                            warn!("Failed to decompress GZIP message from {}: {}", addr, e);
                            debug!("GZIP decompression error details: {:?}", e);
                            continue;
                        }
                    }
                } else if is_zlib {
                    debug!("Attempting to decompress ZLIB data...");
                    match decompress_zlib(raw_data) {
                        Ok(decompressed) => {
                            debug!(
                                "Successfully decompressed {} bytes to {} bytes",
                                raw_data.len(),
                                decompressed.len()
                            );
                            String::from_utf8_lossy(&decompressed).to_string()
                        }
                        Err(e) => {
                            warn!("Failed to decompress ZLIB message from {}: {}", addr, e);
                            debug!("ZLIB decompression error details: {:?}", e);
                            continue;
                        }
                    }
                } else {
                    debug!("Processing uncompressed message data");
                    String::from_utf8_lossy(raw_data).to_string()
                };

                debug!("Message string length: {} characters", message_str.len());
                
                // Safe string truncation that respects UTF-8 character boundaries
                let preview = if message_str.len() <= 200 {
                    message_str.as_str()
                } else {
                    // Find a safe character boundary at or before 200 bytes
                    let mut end = 200;
                    while end > 0 && !message_str.is_char_boundary(end) {
                        end -= 1;
                    }
                    &message_str[..end]
                };
                
                debug!("Message preview (first ~200 chars): {}", preview);

                // Parse GELF message
                debug!("Attempting to parse GELF message...");
                match parse_gelf_message(&message_str) {
                    Ok(gelf_msg) => {
                        debug!("Successfully parsed GELF message structure");
                        debug!("GELF version: {:?}", gelf_msg.version);
                        debug!("GELF host: {:?}", gelf_msg.host);
                        debug!("GELF timestamp: {:?}", gelf_msg.timestamp);
                        debug!("GELF level: {:?}", gelf_msg.level);
                        debug!("GELF facility: {:?}", gelf_msg.facility);

                        info!(
                            "Received GELF message from {}: {}",
                            addr,
                            gelf_msg.short_message.as_deref().unwrap_or("(no message)")
                        );

                        debug!("Adding message to store...");
                        store.add_message(gelf_msg, message_str).await;
                        debug!("Message successfully added to store");
                    }
                    Err(e) => {
                        warn!("Failed to parse GELF message from {}: {}", addr, e);
                        debug!("JSON parsing error details: {:?}", e);
                        debug!("Failed message content: {}", message_str);
                    }
                }
            }
            Err(e) => {
                error!("UDP receive error: {}", e);
                debug!("UDP receive error details: {:?}", e);
            }
        }
    }
}

fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    use flate2::read::GzDecoder;
    use std::io::Read;

    debug!("Starting GZIP decompression for {} bytes", data.len());
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();

    match decoder.read_to_end(&mut decompressed) {
        Ok(bytes_read) => {
            debug!("GZIP decompression successful: {} bytes read", bytes_read);
            Ok(decompressed)
        }
        Err(e) => {
            debug!("GZIP decompression failed: {:?}", e);
            Err(e)
        }
    }
}

fn decompress_zlib(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    debug!("Starting ZLIB decompression for {} bytes", data.len());
    let mut decoder = ZlibDecoder::new(data);
    let mut decompressed = Vec::new();

    match decoder.read_to_end(&mut decompressed) {
        Ok(bytes_read) => {
            debug!("ZLIB decompression successful: {} bytes read", bytes_read);
            Ok(decompressed)
        }
        Err(e) => {
            debug!("ZLIB decompression failed: {:?}", e);
            Err(e)
        }
    }
}

fn parse_gelf_message(message_str: &str) -> Result<GelfMessage, serde_json::Error> {
    debug!(
        "Parsing GELF JSON message of {} characters",
        message_str.len()
    );

    match serde_json::from_str::<GelfMessage>(message_str) {
        Ok(gelf_msg) => {
            debug!("GELF JSON parsing successful");
            debug!(
                "Parsed message fields - version: {:?}, host: {:?}, short_message length: {}",
                gelf_msg.version,
                gelf_msg.host,
                gelf_msg
                    .short_message
                    .as_ref()
                    .map(|s| s.len())
                    .unwrap_or(0)
            );
            Ok(gelf_msg)
        }
        Err(e) => {
            debug!("GELF JSON parsing failed: {:?}", e);
            
            // Safe string truncation that respects UTF-8 character boundaries
            let preview = if message_str.len() <= 100 {
                message_str
            } else {
                // Find a safe character boundary at or before 100 bytes
                let mut end = 100;
                while end > 0 && !message_str.is_char_boundary(end) {
                    end -= 1;
                }
                &message_str[..end]
            };
            
            debug!("Failed to parse as GELF, message preview: {}", preview);
            Err(e)
        }
    }
}

fn get_web_interface() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>GELF Log Viewer</title>
    <style>
        * {
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background-color: #1a1a1a;
            color: #e0e0e0;
            line-height: 1.6;
        }
        
        .header {
            background: linear-gradient(135deg, #2d3748, #4a5568);
            padding: 1rem 2rem;
            border-bottom: 3px solid #4299e1;
            box-shadow: 0 2px 10px rgba(0,0,0,0.3);
        }
        
        .header h1 {
            color: #63b3ed;
            font-size: 1.8rem;
            font-weight: 600;
        }
        
        .stats {
            color: #a0aec0;
            font-size: 0.9rem;
            margin-top: 0.5rem;
        }
        
        .controls {
            padding: 1rem 2rem;
            background: #2d3748;
            border-bottom: 1px solid #4a5568;
            display: flex;
            gap: 1rem;
            align-items: center;
            flex-wrap: wrap;
        }
        
        .btn {
            background: #4299e1;
            color: white;
            border: none;
            padding: 0.5rem 1rem;
            border-radius: 6px;
            cursor: pointer;
            font-size: 0.9rem;
            font-weight: 500;
            transition: all 0.2s;
        }
        
        .btn:hover {
            background: #3182ce;
            transform: translateY(-1px);
        }
        
        .btn:active {
            transform: translateY(0);
        }
        
        .btn.danger {
            background: #e53e3e;
        }
        
        .btn.danger:hover {
            background: #c53030;
        }
        
        .status {
            padding: 0.5rem 1rem;
            border-radius: 6px;
            font-size: 0.85rem;
            font-weight: 500;
        }
        
        .status.connected {
            background: #38a169;
            color: white;
        }
        
        .status.disconnected {
            background: #e53e3e;
            color: white;
        }
        
        .main-content {
            height: calc(100vh - 140px);
            overflow: hidden;
        }
        
        .log-container {
            height: 100%;
            overflow-y: auto;
            padding: 1rem;
            background: #1a1a1a;
        }
        
        .log-entry {
            background: #2d3748;
            border: 1px solid #4a5568;
            border-radius: 8px;
            margin-bottom: 0.75rem;
            padding: 1rem;
            transition: all 0.2s;
            animation: slideIn 0.3s ease-out;
        }
        
        @keyframes slideIn {
            from {
                opacity: 0;
                transform: translateX(-20px);
            }
            to {
                opacity: 1;
                transform: translateX(0);
            }
        }
        
        .log-entry:hover {
            background: #374151;
            border-color: #63b3ed;
            transform: translateY(-1px);
            box-shadow: 0 4px 12px rgba(0,0,0,0.3);
        }
        
        .log-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 0.5rem;
            flex-wrap: wrap;
            gap: 0.5rem;
        }
        
        .log-level {
            padding: 0.2rem 0.6rem;
            border-radius: 4px;
            font-size: 0.75rem;
            font-weight: 600;
            text-transform: uppercase;
        }
        
        .level-0, .level-1, .level-2, .level-3 { background: #e53e3e; color: white; }
        .level-4 { background: #ed8936; color: white; }
        .level-5 { background: #ecc94b; color: #1a1a1a; }
        .level-6 { background: #48bb78; color: white; }
        .level-7 { background: #4299e1; color: white; }
        
        .timestamp {
            color: #a0aec0;
            font-size: 0.8rem;
            font-family: 'Courier New', monospace;
        }
        
        .host {
            color: #63b3ed;
            font-weight: 500;
            font-size: 0.9rem;
        }
        
        .message {
            margin-top: 0.5rem;
        }
        
        .short-message {
            color: #f7fafc;
            font-weight: 500;
            margin-bottom: 0.3rem;
        }
        
        .full-message {
            color: #cbd5e0;
            font-size: 0.9rem;
            background: #1a1a1a;
            padding: 0.5rem;
            border-radius: 4px;
            border-left: 3px solid #4299e1;
            white-space: pre-wrap;
            word-break: break-word;
            margin-top: 0.3rem;
        }
        
        .additional-fields {
            margin-top: 0.5rem;
            padding-top: 0.5rem;
            border-top: 1px solid #4a5568;
        }
        
        .field {
            display: inline-block;
            background: #4a5568;
            color: #e2e8f0;
            padding: 0.2rem 0.5rem;
            margin: 0.1rem 0.3rem 0.1rem 0;
            border-radius: 4px;
            font-size: 0.8rem;
            font-family: 'Courier New', monospace;
        }
        
        .empty-state {
            text-align: center;
            padding: 4rem 2rem;
            color: #a0aec0;
        }
        
        .empty-state h3 {
            font-size: 1.2rem;
            margin-bottom: 0.5rem;
            color: #cbd5e0;
        }
        
        ::-webkit-scrollbar {
            width: 8px;
        }
        
        ::-webkit-scrollbar-track {
            background: #2d3748;
        }
        
        ::-webkit-scrollbar-thumb {
            background: #4a5568;
            border-radius: 4px;
        }
        
        ::-webkit-scrollbar-thumb:hover {
            background: #63b3ed;
        }
        
        @media (max-width: 768px) {
            .controls {
                padding: 1rem;
            }
            
            .log-entry {
                padding: 0.75rem;
            }
            
            .log-header {
                flex-direction: column;
                align-items: flex-start;
            }
        }
    </style>
</head>
<body>
    <div class="header">
        <h1>🔍 GELF Log Viewer</h1>
        <div class="stats">
            <span id="messageCount">0</span> messages • 
            <span id="capacity">0</span>% capacity • 
            Real-time streaming
        </div>
    </div>
    
    <div class="controls">
        <button class="btn" onclick="toggleStream()">
            <span id="streamBtn">Pause Stream</span>
        </button>
        <button class="btn danger" onclick="clearLogs()">Clear Display</button>
        <button class="btn" onclick="loadHistoryLogs()">Load History</button>
        <div class="status" id="status">
            <span id="statusText">Connecting...</span>
        </div>
    </div>
    
    <div class="main-content">
        <div class="log-container" id="logContainer">
            <div class="empty-state">
                <h3>Waiting for log messages...</h3>
                <p>GELF messages will appear here in real-time</p>
            </div>
        </div>
    </div>

    <script>
        let eventSource = null;
        let isStreaming = false;
        let logs = [];
        
        function formatTimestamp(timestamp) {
            return new Date(timestamp * 1000).toLocaleString();
        }
        
        function getLevelClass(level) {
            return level !== undefined ? `level-${level}` : 'level-6';
        }
        
        function getLevelText(level) {
            const levels = {
                0: 'EMERG', 1: 'ALERT', 2: 'CRIT', 3: 'ERR',
                4: 'WARN', 5: 'NOTICE', 6: 'INFO', 7: 'DEBUG'
            };
            return levels[level] || 'INFO';
        }
        
        function createLogEntry(log) {
            const entry = document.createElement('div');
            entry.className = 'log-entry';
            
            const additionalFields = Object.entries(log)
                .filter(([key, value]) => key.startsWith('_') && value !== null && value !== undefined)
                .map(([key, value]) => `<span class="field">${key}: ${value}</span>`)
                .join('');
            
            entry.innerHTML = `
                <div class="log-header">
                    <div>
                        <span class="log-level ${getLevelClass(log.level)}">${getLevelText(log.level)}</span>
                        <span class="host">${log.host || 'unknown'}</span>
                    </div>
                    <span class="timestamp">${formatTimestamp(log.received_at)}</span>
                </div>
                <div class="message">
                    <div class="short-message">${log.short_message || 'No message'}</div>
                    ${log.full_message ? `<div class="full-message">${log.full_message}</div>` : ''}
                </div>
                ${additionalFields ? `<div class="additional-fields">${additionalFields}</div>` : ''}
            `;
            
            return entry;
        }
        
        function addLogEntry(log) {
            const container = document.getElementById('logContainer');
            const emptyState = container.querySelector('.empty-state');
            
            if (emptyState) {
                emptyState.remove();
            }
            
            const entry = createLogEntry(log);
            container.insertBefore(entry, container.firstChild);
            
            // Keep only last 1000 entries for performance
            while (container.children.length > 1000) {
                container.removeChild(container.lastChild);
            }
        }
        
        function updateStats() {
            fetch('/stats')
                .then(response => response.json())
                .then(data => {
                    document.getElementById('messageCount').textContent = data.total_messages;
                    document.getElementById('capacity').textContent = data.capacity_used_percent.toFixed(1);
                })
                .catch(console.error);
        }
        
        function startStream() {
            if (eventSource) {
                eventSource.close();
            }
            
            eventSource = new EventSource('/stream');
            
            eventSource.onopen = function() {
                console.log('SSE connection opened');
                document.getElementById('status').className = 'status connected';
                document.getElementById('statusText').textContent = 'Connected';
                isStreaming = true;
                document.getElementById('streamBtn').textContent = 'Pause Stream';
            };
            
            eventSource.onmessage = function(event) {
                const log = JSON.parse(event.data);
                addLogEntry(log);
            };
            
            eventSource.onerror = function() {
                console.log('SSE connection error');
                document.getElementById('status').className = 'status disconnected';
                document.getElementById('statusText').textContent = 'Disconnected';
                
                // Attempt to reconnect after 5 seconds
                setTimeout(() => {
                    if (isStreaming) {
                        console.log('Attempting to reconnect...');
                        startStream();
                    }
                }, 5000);
            };
        }
        
        function stopStream() {
            if (eventSource) {
                eventSource.close();
                eventSource = null;
            }
            isStreaming = false;
            document.getElementById('status').className = 'status disconnected';
            document.getElementById('statusText').textContent = 'Paused';
            document.getElementById('streamBtn').textContent = 'Resume Stream';
        }
        
        function toggleStream() {
            if (isStreaming) {
                stopStream();
            } else {
                startStream();
            }
        }
        
        function clearLogs() {
            const container = document.getElementById('logContainer');
            container.innerHTML = '<div class="empty-state"><h3>Display cleared</h3><p>New messages will appear here</p></div>';
        }
        
        function loadHistoryLogs() {
            fetch('/logs?limit=50')
                .then(response => response.json())
                .then(data => {
                    clearLogs();
                    data.reverse().forEach(log => addLogEntry(log));
                })
                .catch(console.error);
        }
        
        // Initialize
        document.addEventListener('DOMContentLoaded', function() {
            startStream();
            updateStats();
            setInterval(updateStats, 10000); // Update stats every 10 seconds
            
            // Load initial history
            loadHistoryLogs();
        });
        
        // Clean up on page unload
        window.addEventListener('beforeunload', function() {
            if (eventSource) {
                eventSource.close();
            }
        });
    </script>
</body>
</html>"#.to_string()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with debug level support
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".into()),
        )
        .init();

    debug!("Tracing initialized with debug level");

    let args = Args::parse();
    debug!(
        "Parsed command line arguments: UDP port: {}, HTTP port: {}, bind address: {}, max messages: {}",
        args.udp_port, args.http_port, args.bind_address, args.max_messages
    );

    let store = LogStore::new(args.max_messages);
    debug!("Created log store with max capacity: {}", args.max_messages);

    info!("Starting GELF collector...");
    info!("UDP port: {}", args.udp_port);
    info!("HTTP port: {}", args.http_port);
    info!("Max messages: {}", args.max_messages);

    // Setup UDP listener
    let udp_addr: SocketAddr = format!("{}:{}", args.bind_address, args.udp_port).parse()?;
    debug!("Attempting to bind UDP socket to address: {}", udp_addr);

    let socket = Arc::new(UdpSocket::bind(udp_addr).await?);
    info!("UDP listener started on {}", udp_addr);
    debug!("UDP socket successfully bound and ready to receive messages");

    // Start UDP message handler
    let store_clone = store.clone();
    debug!("Spawning UDP message handler task");
    let udp_task = tokio::spawn(async move {
        debug!("UDP message handler task started");
        handle_udp_message(socket, store_clone).await;
    });

    // Setup HTTP routes
    debug!("Setting up HTTP routes");
    let store_filter = warp::any().map(move || store.clone());

    // GET /logs - retrieve log messages
    let logs_route = warp::path("logs")
        .and(warp::get())
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .and(store_filter.clone())
        .and_then(
            |params: std::collections::HashMap<String, String>, store: LogStore| async move {
                debug!(
                    "Received request for /logs endpoint with params: {:?}",
                    params
                );
                let limit = params.get("limit").and_then(|s| s.parse::<usize>().ok());
                debug!("Parsed limit parameter: {:?}", limit);

                let messages = store.get_messages(limit).await;
                debug!("Retrieved {} messages from store", messages.len());
                Ok::<_, warp::Rejection>(warp::reply::json(&messages))
            },
        );

    // GET /stats - get storage statistics
    let stats_route = warp::path("stats")
        .and(warp::get())
        .and(store_filter.clone())
        .and_then(|store: LogStore| async move {
            debug!("Received request for /stats endpoint");
            let stats = store.get_stats().await;
            debug!("Retrieved stats: {:?}", stats);
            Ok::<_, warp::Rejection>(warp::reply::json(&stats))
        });

    // GET /health - health check
    let health_route = warp::path("health").and(warp::get()).map(|| {
        debug!("Received request for /health endpoint");
        warp::reply::json(&serde_json::json!({"status": "ok"}))
    });

    // GET / - serve web interface
    let web_route = warp::path::end().and(warp::get()).map(|| {
        debug!("Received request for web interface");
        warp::reply::html(get_web_interface())
    });

    // GET /stream - Server-Sent Events for real-time log streaming
    let stream_route = warp::path("stream")
        .and(warp::get())
        .and(store_filter.clone())
        .map(|store: LogStore| {
            debug!("New SSE client connected");
            let rx = store.subscribe();
            let stream = BroadcastStream::new(rx)
                .filter_map(|result| async move {
                    match result {
                        Ok(message) => {
                            let json_str = serde_json::to_string(&message).ok()?;
                            Some(Ok::<_, warp::Error>(warp::sse::Event::default()
                                .event("message")
                                .data(json_str)))
                        }
                        Err(_) => None, // Client lagged behind, skip
                    }
                });

            warp::sse::reply(warp::sse::keep_alive().stream(stream))
        });

    // Combine routes
    debug!("Combining HTTP routes with CORS configuration");
    let routes = web_route
        .or(logs_route)
        .or(stats_route)
        .or(health_route)
        .or(stream_route)
        .with(
            warp::cors()
                .allow_any_origin()
                .allow_headers(vec!["content-type"])
                .allow_methods(vec!["GET"]),
        );

    // Start HTTP server
    let http_addr: SocketAddr = format!("{}:{}", args.bind_address, args.http_port).parse()?;
    debug!("Attempting to start HTTP server on address: {}", http_addr);
    info!("HTTP server starting on {}", http_addr);

    let http_task = tokio::spawn(async move {
        debug!("HTTP server task started, beginning to serve requests");
        warp::serve(routes).run(http_addr).await;
    });

    info!("GELF collector is running!");
    info!("Send GELF messages to UDP port {}", args.udp_port);
    info!(
        "🌐 Web Interface: http://{}:{}/ (Real-time log viewer)",
        args.bind_address, args.http_port
    );
    info!(
        "📊 API Endpoints: http://{}:{}/logs | /stats | /stream",
        args.bind_address, args.http_port
    );

    // Wait for both tasks
    tokio::select! {
        _ = udp_task => {
            error!("UDP task terminated unexpectedly");
        }
        _ = http_task => {
            error!("HTTP task terminated unexpectedly");
        }
    }

    Ok(())
}
