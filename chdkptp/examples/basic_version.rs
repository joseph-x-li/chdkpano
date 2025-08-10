use chdkptp::{ChdkPtpClient, ChdkPtpError};

fn main() -> Result<(), ChdkPtpError> {
    // Common Canon camera vendor ID
    let vendor_id = 0x04A9; // Canon
    let product_id = 0x32F9; // Example product ID - you'll need to find the correct one for your camera
    
    println!("Attempting to connect to CHDK camera...");
    println!("Vendor ID: 0x{:04X}, Product ID: 0x{:04X}", vendor_id, product_id);
    println!("Note: You may need to adjust the product ID for your specific camera model");
    
    match ChdkPtpClient::new(vendor_id, product_id) {
        Ok(mut client) => {
            println!("Successfully connected to camera!");
            
            match client.get_version() {
                Ok((major, minor)) => {
                    println!("CHDK PTP Version: {}.{}", major, minor);
                }
                Err(e) => {
                    println!("Failed to get version: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Failed to connect to camera: {}", e);
            println!("\nTroubleshooting tips:");
            println!("1. Make sure your camera is connected via USB");
            println!("2. Ensure CHDK is loaded on your camera");
            println!("3. Check that the camera is in PTP mode");
            println!("4. You may need to run this with sudo/administrator privileges");
            println!("5. Try different product IDs for your camera model");
        }
    }
    
    Ok(())
} 