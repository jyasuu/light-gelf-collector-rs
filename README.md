# Light GELF Collector

A lightweight GELF (Graylog Extended Log Format) log collector written in Rust.

## Features

✅ **UDP Listener** - Receives GELF log messages on configurable UDP port (default: 12201)
✅ **In-Memory Storage** - Stores log messages in memory with configurable size limits and automatic cleanup
✅ **HTTP Service** - Provides REST API to fetch stored log messages
✅ **GZIP Support** - Automatically decompresses GZIP-compressed GELF messages
✅ **Configurable** - Command-line options for ports, storage limits, and bind addresses

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

The server accepts standard GELF messages in JSON format. Example:

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

- **Async UDP Server**: Uses Tokio for high-performance async UDP message handling
- **Thread-Safe Storage**: Uses Arc<RwLock<VecDeque>> for concurrent access to the message store
- **HTTP API**: Built with Warp web framework for the REST API
- **GZIP Support**: Automatic decompression of compressed GELF messages
- **Structured Logging**: Uses tracing for application logging

## Dependencies

- `tokio` - Async runtime
- `warp` - Web framework for HTTP API
- `serde/serde_json` - JSON serialization/deserialization
- `clap` - Command-line argument parsing
- `tracing` - Structured logging
- `flate2` - GZIP compression support