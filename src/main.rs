use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};
use warp::Filter;

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
}

impl LogStore {
    fn new(max_size: usize) -> Self {
        Self {
            messages: Arc::new(RwLock::new(VecDeque::new())),
            max_size,
        }
    }

    async fn add_message(&self, gelf_message: GelfMessage, raw_message: String) {
        let mut messages = self.messages.write().await;

        let stored_message = StoredMessage {
            gelf_message,
            received_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs_f64(),
            raw_message,
        };

        messages.push_back(stored_message);

        // Clean up if we exceed max size
        while messages.len() > self.max_size {
            messages.pop_front();
        }
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

                // Try to decompress if it's gzipped
                let is_gzipped = raw_data.len() > 2 && raw_data[0] == 0x1f && raw_data[1] == 0x8b;
                debug!(
                    "Message compression detected: {}",
                    if is_gzipped { "GZIP" } else { "none" }
                );

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
                } else {
                    debug!("Processing uncompressed message data");
                    String::from_utf8_lossy(raw_data).to_string()
                };

                debug!("Message string length: {} characters", message_str.len());
                debug!(
                    "Message preview (first 200 chars): {}",
                    &message_str[..std::cmp::min(200, message_str.len())]
                );

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
            debug!(
                "Failed to parse as GELF, message preview: {}",
                &message_str[..std::cmp::min(100, message_str.len())]
            );
            Err(e)
        }
    }
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

    // Combine routes
    debug!("Combining HTTP routes with CORS configuration");
    let routes = logs_route.or(stats_route).or(health_route).with(
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
        "Access logs via HTTP: http://{}:{}/logs",
        args.bind_address, args.http_port
    );
    info!(
        "View stats at: http://{}:{}/stats",
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
