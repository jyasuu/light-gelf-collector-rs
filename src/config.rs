use clap::Parser;
use std::net::SocketAddr;

/// Application configuration
#[derive(Parser, Clone, Debug)]
#[command(name = "light-gelf-collector")]
#[command(about = "A lightweight GELF log collector")]
pub struct Config {
    /// UDP port to listen for GELF messages
    #[arg(short, long, default_value = "12201")]
    pub udp_port: u16,

    /// HTTP port for the web service
    #[arg(short = 'H', long, default_value = "8080")]
    pub http_port: u16,

    /// Maximum number of log messages to keep in memory
    #[arg(short, long, default_value = "10000")]
    pub max_messages: usize,

    /// Bind address
    #[arg(short, long, default_value = "0.0.0.0")]
    pub bind_address: String,
}

impl Config {
    pub fn udp_addr(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        format!("{}:{}", self.bind_address, self.udp_port).parse()
    }

    pub fn http_addr(&self) -> Result<SocketAddr, std::net::AddrParseError> {
        format!("{}:{}", self.bind_address, self.http_port).parse()
    }
}