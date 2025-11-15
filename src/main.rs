use clap::Parser;
use light_gelf_collector_rs::{Config, InMemoryMessageStore};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing::{debug, error, info};

use light_gelf_collector_rs::udp_handler::handle_udp_messages;
use light_gelf_collector_rs::web::create_routes;





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

    let config = Config::parse();
    debug!(
        "Parsed command line arguments: UDP port: {}, HTTP port: {}, bind address: {}, max messages: {}",
        config.udp_port, config.http_port, config.bind_address, config.max_messages
    );

    let store = InMemoryMessageStore::new(config.max_messages);
    debug!("Created log store with max capacity: {}", config.max_messages);

    info!("Starting GELF collector...");
    info!("UDP port: {}", config.udp_port);
    info!("HTTP port: {}", config.http_port);
    info!("Max messages: {}", config.max_messages);

    // Setup UDP listener
    let udp_addr = config.udp_addr()?;
    debug!("Attempting to bind UDP socket to address: {}", udp_addr);

    let socket = Arc::new(UdpSocket::bind(udp_addr).await?);
    info!("UDP listener started on {}", udp_addr);
    debug!("UDP socket successfully bound and ready to receive messages");

    // Start UDP message handler
    let store_clone = store.clone();
    debug!("Spawning UDP message handler task");
    let udp_task = tokio::spawn(async move {
        debug!("UDP message handler task started");
        handle_udp_messages(socket, store_clone).await;
    });

    // Setup HTTP routes
    debug!("Setting up HTTP routes");
    let routes = create_routes(store);

    // Start HTTP server
    let http_addr = config.http_addr()?;
    debug!("Attempting to start HTTP server on address: {}", http_addr);
    info!("HTTP server starting on {}", http_addr);

    let http_task = tokio::spawn(async move {
        debug!("HTTP server task started, beginning to serve requests");
        warp::serve(routes).run(http_addr).await;
    });

    info!("GELF collector is running!");
    info!("Send GELF messages to UDP port {}", config.udp_port);
    info!(
        "🌐 Web Interface: http://{}:{}/ (Real-time log viewer)",
        config.bind_address, config.http_port
    );
    info!(
        "📊 API Endpoints: http://{}:{}/logs | /stats | /stream",
        config.bind_address, config.http_port
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
