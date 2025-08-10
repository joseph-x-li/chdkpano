# CHDK PTP Rust Library

This is a Rust implementation of the CHDK PTP (Picture Transfer Protocol) extensions for communicating with Canon cameras running CHDK firmware.

## Features

- Full Rust implementation using `rusb` for USB communication
- Implements the CHDK PTP protocol as specified in `ptp.h`
- Currently supports the basic `PTP_CHDK_Version` command
- Extensible architecture for adding more CHDK PTP commands

## Prerequisites

- Rust and Cargo installed
- A Canon camera with CHDK firmware loaded
- USB connection to the camera
- On Linux/macOS, you may need to run with sudo/administrator privileges

## Installation

```bash
cd chdkptp
cargo build
```

## Usage

### Finding Your Camera's USB IDs

First, run the device listing utility to find your camera's vendor and product IDs:

```bash
cargo run --example list_devices
```

Look for your camera in the output. Canon cameras typically have vendor ID `0x04A9`.

### Basic Version Check

Run the basic example to test the connection:

```bash
cargo run --example basic_version
```

You'll need to modify the vendor and product IDs in the example to match your camera.

### Using the Library

```rust
use chdkptp::{ChdkPtpClient, ChdkPtpError};

fn main() -> Result<(), ChdkPtpError> {
    // Replace with your camera's IDs
    let vendor_id = 0x04A9;  // Canon
    let product_id = 0x32F9; // Your camera's product ID
    
    let mut client = ChdkPtpClient::new(vendor_id, product_id)?;
    let (major, minor) = client.get_version()?;
    println!("CHDK PTP Version: {}.{}", major, minor);
    
    Ok(())
}
```

## Protocol Implementation

The library implements the CHDK PTP protocol as defined in `ptp.h`:

- **PTP Container Structure**: 12-byte header + payload
- **CHDK Commands**: All commands from the protocol specification
- **Error Handling**: Comprehensive error types for USB and PTP errors
- **Transaction Management**: Automatic transaction ID management

## Current Status

This is a first pass implementation with:

- ✅ Basic PTP container structure
- ✅ USB device discovery and connection
- ✅ `PTP_CHDK_Version` command implementation
- ✅ Error handling and type safety
- ✅ Extensible architecture for additional commands

## Next Steps

The library is designed to be easily extended with additional CHDK PTP commands:

1. **Memory Operations**: `GetMemory`, `SetMemory`
2. **Script Execution**: `ExecuteScript`, `ScriptStatus`, `ReadScriptMsg`
3. **File Operations**: `UploadFile`, `DownloadFile`
4. **Remote Capture**: `RemoteCaptureIsReady`, `RemoteCaptureGetData`

## Troubleshooting

1. **Device not found**: Check that your camera is connected and in PTP mode
2. **Permission denied**: Run with sudo/administrator privileges
3. **Interface not found**: Ensure CHDK is loaded on your camera
4. **Wrong product ID**: Use the `list_devices` example to find the correct ID

## License

This project is licensed under the MIT License. 