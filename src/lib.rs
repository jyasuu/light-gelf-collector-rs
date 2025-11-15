// Core library modules
pub mod config;
pub mod compression;
pub mod gelf;
pub mod storage;
pub mod web;
pub mod udp_handler;

// Re-export commonly used types
pub use config::Config;
pub use gelf::{GelfMessage, MessageResponse, StoredMessage};
pub use storage::{MessageStore, InMemoryMessageStore};