use crate::compression::CompressionManager;
use crate::gelf::{GelfParser, JsonGelfParser};
use crate::storage::MessageStore;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tracing::{debug, error, info, warn};

/// UDP message handler configuration
pub struct UdpHandlerConfig {
    pub buffer_size: usize,
}

impl Default for UdpHandlerConfig {
    fn default() -> Self {
        Self {
            buffer_size: 8192,
        }
    }
}

/// UDP message handler that processes incoming GELF messages
pub struct UdpMessageHandler<S: MessageStore, P: GelfParser> {
    socket: Arc<UdpSocket>,
    store: S,
    compression_manager: CompressionManager,
    parser: P,
    config: UdpHandlerConfig,
}

impl<S: MessageStore> UdpMessageHandler<S, JsonGelfParser> {
    pub fn new(socket: Arc<UdpSocket>, store: S) -> Self {
        Self::with_config(socket, store, UdpHandlerConfig::default())
    }

    pub fn with_config(socket: Arc<UdpSocket>, store: S, config: UdpHandlerConfig) -> Self {
        Self {
            socket,
            store,
            compression_manager: CompressionManager::new(),
            parser: JsonGelfParser,
            config,
        }
    }
}

impl<S: MessageStore, P: GelfParser> UdpMessageHandler<S, P> {
    pub fn with_parser(socket: Arc<UdpSocket>, store: S, parser: P) -> Self {
        Self {
            socket,
            store,
            compression_manager: CompressionManager::new(),
            parser,
            config: UdpHandlerConfig::default(),
        }
    }

    pub async fn run(&self) {
        let mut buf = vec![0; self.config.buffer_size];
        debug!("Starting UDP message handler with buffer size: {}", buf.len());

        loop {
            debug!("Waiting for UDP message...");
            match self.socket.recv_from(&mut buf).await {
                Ok((len, addr)) => {
                    debug!("Received {} bytes from {}", len, addr);
                    let raw_data = &buf[..len];

                    // Log raw data information
                    debug!(
                        "Raw data first 10 bytes: {:?}",
                        &raw_data[..std::cmp::min(10, raw_data.len())]
                    );

                    // Try to decompress the data
                    let message_str = match self.compression_manager.decompress(raw_data) {
                        Ok(decompressed) => {
                            if decompressed.len() != raw_data.len() {
                                debug!(
                                    "Successfully decompressed {} bytes to {} bytes",
                                    raw_data.len(),
                                    decompressed.len()
                                );
                            } else {
                                debug!("Processing uncompressed message data");
                            }
                            String::from_utf8_lossy(&decompressed).to_string()
                        }
                        Err(e) => {
                            warn!("Failed to decompress message from {}: {}", addr, e);
                            debug!("Decompression error details: {:?}", e);
                            continue;
                        }
                    };

                    debug!("Message string length: {} characters", message_str.len());
                    
                    // Safe string truncation for logging
                    let preview = self.get_safe_preview(&message_str, 200);
                    debug!("Message preview (first ~200 chars): {}", preview);

                    // Parse GELF message
                    debug!("Attempting to parse GELF message...");
                    match self.parser.parse(&message_str) {
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
                            self.store.add_message(gelf_msg, message_str).await;
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

    fn get_safe_preview<'a>(&self, text: &'a str, max_len: usize) -> &'a str {
        if text.len() <= max_len {
            text
        } else {
            // Find a safe character boundary at or before max_len bytes
            let mut end = max_len;
            while end > 0 && !text.is_char_boundary(end) {
                end -= 1;
            }
            &text[..end]
        }
    }
}

/// Convenience function to handle UDP messages
pub async fn handle_udp_messages<S: MessageStore>(socket: Arc<UdpSocket>, store: S) {
    let handler = UdpMessageHandler::new(socket, store);
    handler.run().await;
}