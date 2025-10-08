use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use warp::Filter;
use serde::{Deserialize, Serialize};
use clap::Parser;
use tracing::{info, warn, error};

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
        messages.iter()
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
    
    loop {
        match socket.recv_from(&mut buf).await {
            Ok((len, addr)) => {
                let raw_data = &buf[..len];
                
                // Try to decompress if it's gzipped
                let message_str = if raw_data.len() > 2 && raw_data[0] == 0x1f && raw_data[1] == 0x8b {
                    match decompress_gzip(raw_data) {
                        Ok(decompressed) => String::from_utf8_lossy(&decompressed).to_string(),
                        Err(e) => {
                            warn!("Failed to decompress GZIP message from {}: {}", addr, e);
                            continue;
                        }
                    }
                } else {
                    String::from_utf8_lossy(raw_data).to_string()
                };
                
                // Parse GELF message
                match parse_gelf_message(&message_str) {
                    Ok(gelf_msg) => {
                        info!("Received GELF message from {}: {}", addr, 
                              gelf_msg.short_message.as_deref().unwrap_or("(no message)"));
                        store.add_message(gelf_msg, message_str).await;
                    }
                    Err(e) => {
                        warn!("Failed to parse GELF message from {}: {}", addr, e);
                    }
                }
            }
            Err(e) => {
                error!("UDP receive error: {}", e);
            }
        }
    }
}

fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    
    let mut decoder = GzDecoder::new(data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    Ok(decompressed)
}

fn parse_gelf_message(message_str: &str) -> Result<GelfMessage, serde_json::Error> {
    let gelf_msg: GelfMessage = serde_json::from_str(message_str)?;
    Ok(gelf_msg)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();
    
    let args = Args::parse();
    let store = LogStore::new(args.max_messages);
    
    info!("Starting GELF collector...");
    info!("UDP port: {}", args.udp_port);
    info!("HTTP port: {}", args.http_port);
    info!("Max messages: {}", args.max_messages);
    
    // Setup UDP listener
    let udp_addr: SocketAddr = format!("{}:{}", args.bind_address, args.udp_port).parse()?;
    let socket = Arc::new(UdpSocket::bind(udp_addr).await?);
    info!("UDP listener started on {}", udp_addr);
    
    // Start UDP message handler
    let store_clone = store.clone();
    let udp_task = tokio::spawn(async move {
        handle_udp_message(socket, store_clone).await;
    });
    
    // Setup HTTP routes
    let store_filter = warp::any().map(move || store.clone());
    
    // GET /logs - retrieve log messages
    let logs_route = warp::path("logs")
        .and(warp::get())
        .and(warp::query::<std::collections::HashMap<String, String>>())
        .and(store_filter.clone())
        .and_then(|params: std::collections::HashMap<String, String>, store: LogStore| async move {
            let limit = params.get("limit")
                .and_then(|s| s.parse::<usize>().ok());
            
            let messages = store.get_messages(limit).await;
            Ok::<_, warp::Rejection>(warp::reply::json(&messages))
        });
    
    // GET /stats - get storage statistics
    let stats_route = warp::path("stats")
        .and(warp::get())
        .and(store_filter.clone())
        .and_then(|store: LogStore| async move {
            let stats = store.get_stats().await;
            Ok::<_, warp::Rejection>(warp::reply::json(&stats))
        });
    
    // GET /health - health check
    let health_route = warp::path("health")
        .and(warp::get())
        .map(|| {
            warp::reply::json(&serde_json::json!({"status": "ok"}))
        });
    
    // Combine routes
    let routes = logs_route
        .or(stats_route)
        .or(health_route)
        .with(warp::cors().allow_any_origin().allow_headers(vec!["content-type"]).allow_methods(vec!["GET"]));
    
    // Start HTTP server
    let http_addr: SocketAddr = format!("{}:{}", args.bind_address, args.http_port).parse()?;
    info!("HTTP server starting on {}", http_addr);
    
    let http_task = tokio::spawn(async move {
        warp::serve(routes).run(http_addr).await;
    });
    
    info!("GELF collector is running!");
    info!("Send GELF messages to UDP port {}", args.udp_port);
    info!("Access logs via HTTP: http://{}:{}/logs", args.bind_address, args.http_port);
    info!("View stats at: http://{}:{}/stats", args.bind_address, args.http_port);
    
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
