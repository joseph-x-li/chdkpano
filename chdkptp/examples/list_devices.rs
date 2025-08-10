use rusb::{Context, UsbContext};

fn main() -> Result<(), rusb::Error> {
    let context = Context::new()?;
    let devices = context.devices()?;
    
    println!("Available USB devices:");
    println!("=====================");
    
    for device in devices.iter() {
        if let Ok(desc) = device.device_descriptor() {
            let vendor_id = desc.vendor_id();
            let product_id = desc.product_id();
            
            // Try to get manufacturer and product strings
            let handle = device.open()?;
            let manufacturer = handle.read_manufacturer_string_ascii(&desc).unwrap_or_else(|_| "Unknown".to_string());
            let product = handle.read_product_string_ascii(&desc).unwrap_or_else(|_| "Unknown".to_string());
            
            println!("Vendor ID: 0x{:04X}, Product ID: 0x{:04X}", vendor_id, product_id);
            println!("  Manufacturer: {}", manufacturer);
            println!("  Product: {}", product);
            println!();
        }
    }
    
    Ok(())
} 