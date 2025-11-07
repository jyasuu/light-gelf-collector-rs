# Light GELF Collector

A lightweight GELF (Graylog Extended Log Format) log collector written in Rust.

## Features

### Core Functionality
✅ **UDP GELF Listener** - High-performance async UDP server that receives GELF log messages on configurable port (default: 12201)  
✅ **In-Memory Storage** - Thread-safe circular buffer storage with configurable size limits and automatic cleanup  
✅ **REST API** - Full-featured HTTP service providing multiple endpoints for log retrieval and monitoring  
✅ **Real-time Processing** - Concurrent message handling with detailed logging and error handling  

### Compression Support
✅ **Multi-Format Compression** - Automatic detection and decompression of compressed GELF messages:
  - **GZIP** compression (RFC 1952) - `0x1f 0x8b` magic bytes
  - **ZLIB** compression (RFC 1950) - `0x78 0x9c/0xda/0x01` magic bytes  
  - **Uncompressed** messages - Raw JSON format
✅ **Safe UTF-8 Handling** - Robust string processing with proper character boundary handling

### Configuration & Deployment
✅ **Flexible Configuration** - Command-line options for all major settings:
  - UDP/HTTP ports, bind addresses, memory limits
  - Environment-based log level configuration
✅ **Docker Support** - Ready-to-use containerized deployment with docker-compose
✅ **Production Ready** - Structured logging, health checks, and comprehensive error handling

### Monitoring & Observability
✅ **Built-in Statistics** - Memory usage, message counts, and capacity monitoring
✅ **Health Checks** - Dedicated endpoint for service health monitoring
✅ **Structured Logging** - Detailed debug logging with configurable levels using `tracing`
✅ **CORS Support** - Cross-origin resource sharing for web-based dashboards

## Installation & Usage

### Build and Run

```bash
# Build the project
cargo build --release

# Run with default settings
cargo run

# Run with custom configuration
cargo run -- --udp-port 12201 --http-port 8080 --max-messages 5000
```

### Command Line Options

```
-u, --udp-port <UDP_PORT>           UDP port to listen for GELF messages [default: 12201]
-H, --http-port <HTTP_PORT>         HTTP port for the web service [default: 8080]
-m, --max-messages <MAX_MESSAGES>   Maximum number of log messages to keep in memory [default: 10000]
-b, --bind-address <BIND_ADDRESS>   Bind address [default: 0.0.0.0]
```

## API Endpoints

### GET /logs
Retrieve stored log messages (most recent first).

**Query Parameters:**
- `limit` (optional): Maximum number of messages to return

**Example:**
```bash
curl "http://localhost:8080/logs?limit=10"
```

**Response Format:**
Each message includes the original GELF fields plus a `received_at` timestamp:
```json
[
  {
    "version": "1.1",
    "host": "web-server-01",
    "short_message": "User login successful",
    "timestamp": 1672531200.123,
    "level": 6,
    "facility": "auth",
    "_user_id": "12345",
    "received_at": 1672531205.456
  }
]
```

### GET /stats
Get storage statistics.

**Example:**
```bash
curl "http://localhost:8080/stats"
```

**Response:**
```json
{
  "total_messages": 150,
  "max_capacity": 10000,
  "capacity_used_percent": 1.5
}
```

### GET /health
Health check endpoint.

**Example:**
```bash
curl "http://localhost:8080/health"
```

## GELF Message Format

The server accepts standard GELF messages in JSON format, both compressed and uncompressed:

### Supported Formats
- **Uncompressed JSON** - Raw GELF messages in UTF-8 encoded JSON
- **GZIP Compressed** - JSON compressed with GZIP (RFC 1952)
- **ZLIB Compressed** - JSON compressed with ZLIB/Deflate (RFC 1950)

The compression format is automatically detected based on magic bytes and decompressed transparently.

### Example GELF Message

```json
{
  "version": "1.1",
  "host": "web-server-01",
  "short_message": "User login successful",
  "full_message": "User john.doe@example.com logged in successfully from IP 192.168.1.100",
  "timestamp": 1672531200.123,
  "level": 6,
  "facility": "auth",
  "_user_id": "12345",
  "_ip_address": "192.168.1.100"
}
```

## Testing

### Send a Test GELF Message

You can test the collector by sending a GELF message via UDP:

```bash
# Using netcat (if available)
echo '{"version":"1.1","host":"test-host","short_message":"Test message","timestamp":1672531200,"level":6}' | nc -u localhost 12201

# Using PowerShell
$message = '{"version":"1.1","host":"test-host","short_message":"Test message","timestamp":1672531200,"level":6}'
$bytes = [System.Text.Encoding]::UTF8.GetBytes($message)
$udp = New-Object System.Net.Sockets.UdpClient
$udp.Send($bytes, $bytes.Length, "localhost", 12201)
$udp.Close()
```

### Retrieve Messages

```bash
curl "http://localhost:8080/logs"
```

## Memory Management

The collector automatically manages memory by:
- Storing messages in a circular buffer (VecDeque)
- Removing oldest messages when the limit is reached
- Adding a `received_at` timestamp to each message for tracking

## Architecture

- **Async UDP Server**: Uses Tokio for high-performance async UDP message handling with 8KB buffer
- **Thread-Safe Storage**: Uses `Arc<RwLock<VecDeque>>` for concurrent access to the circular message buffer
- **HTTP API**: Built with Warp web framework providing RESTful endpoints with CORS support
- **Multi-Format Compression**: Automatic detection and decompression using `flate2` (GZIP & ZLIB)
- **Safe String Processing**: UTF-8 character boundary-aware truncation and preview generation
- **Structured Logging**: Uses `tracing` with configurable log levels and detailed debug information
- **Memory Management**: Automatic cleanup with configurable limits and real-time statistics tracking

## Dependencies

- `tokio` - Async runtime
- `warp` - Web framework for HTTP API
- `serde/serde_json` - JSON serialization/deserialization
- `clap` - Command-line argument parsing
- `tracing` - Structured logging
- `flate2` - GZIP compression support