use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::debug;

/// GELF message structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GelfMessage {
    pub version: Option<String>,
    pub host: Option<String>,
    pub short_message: Option<String>,
    pub full_message: Option<String>,
    pub timestamp: Option<f64>,
    pub level: Option<u8>,
    pub facility: Option<String>,
    pub line: Option<u32>,
    pub file: Option<String>,
    #[serde(flatten)]
    pub additional_fields: serde_json::Map<String, serde_json::Value>,
}

/// Stored message with metadata
#[derive(Debug, Clone)]
pub struct StoredMessage {
    pub gelf_message: GelfMessage,
    pub received_at: f64,
    pub raw_message: String,
}

/// Message response for API
#[derive(Debug, Clone, Serialize)]
pub struct MessageResponse {
    #[serde(flatten)]
    pub gelf_message: GelfMessage,
    pub received_at: f64,
}

/// Trait for parsing GELF messages
pub trait GelfParser {
    fn parse(&self, message_str: &str) -> Result<GelfMessage, serde_json::Error>;
}

/// Default JSON-based GELF parser
pub struct JsonGelfParser;

impl GelfParser for JsonGelfParser {
    fn parse(&self, message_str: &str) -> Result<GelfMessage, serde_json::Error> {
        debug!("Parsing GELF JSON message of {} characters", message_str.len());

        match serde_json::from_str::<GelfMessage>(message_str) {
            Ok(gelf_msg) => {
                debug!("GELF JSON parsing successful");
                debug!(
                    "Parsed message fields - version: {:?}, host: {:?}, short_message length: {}",
                    gelf_msg.version,
                    gelf_msg.host,
                    gelf_msg.short_message.as_ref().map(|s| s.len()).unwrap_or(0)
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
}

impl StoredMessage {
    pub fn new(gelf_message: GelfMessage, raw_message: String) -> Self {
        let received_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();

        Self {
            gelf_message,
            received_at,
            raw_message,
        }
    }

    pub fn to_response(&self) -> MessageResponse {
        MessageResponse {
            gelf_message: self.gelf_message.clone(),
            received_at: self.received_at,
        }
    }
}