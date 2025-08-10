# Pano Server

A Rust-based web server that integrates with the chdkptp library for camera control and panoramic photography.

## Overview

This project is designed to provide a web API for controlling cameras through the chdkptp library, enabling remote camera operations for panoramic photography applications.

## Features (Planned)

- Web server with REST API endpoints
- Camera discovery and connection management
- Photo capture with configurable parameters
- Integration with the local chdkptp library

## Project Structure

```
pano_server/
├── Cargo.toml          # Project dependencies and configuration
├── src/
│   └── main.rs        # Main application code
└── README.md          # This file
```

## Dependencies

- **actix-web**: Web framework for building HTTP servers
- **tokio**: Async runtime for Rust
- **serde**: Serialization/deserialization framework
- **chdkptp**: Local library for camera communication (path: `../chdkptp`)

## API Endpoints

- `GET /` - Hello world endpoint
- `GET /cameras` - List available cameras
- `POST /capture` - Capture a photo with specified parameters

## Development Status

This is currently a prototype with pseudo code. The actual integration with the chdkptp library will be implemented as the library becomes available.

## Building and Running

```bash
# Build the project
cargo build

# Run the server
cargo run
```

The server will start on `http://127.0.0.1:8080`

## Next Steps

1. Complete the chdkptp library implementation
2. Replace pseudo code with actual camera control logic
3. Add error handling and logging
4. Implement camera connection management
5. Add photo capture and storage functionality
