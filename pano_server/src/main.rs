use actix_web::{web, App, HttpServer, HttpResponse, Result};
use serde::{Deserialize, Serialize};
// TODO: Import from our chdkptp library when it's ready
// use chdkptp::{Camera, PTPConnection};

mod servo;
use servo::ServoArray;

#[derive(Serialize, Deserialize)]
struct CameraInfo {
    name: String,
    connected: bool,
    model: String,
}

#[derive(Serialize, Deserialize)]
struct CaptureRequest {
    exposure_time: f32,
    iso: u32,
}

#[derive(Serialize, Deserialize)]
struct ServoRequest {
    position: String, // "reset", "power", "battery", or "custom"
    angles: Option<[u16; 4]>, // Required for "custom" position
}

async fn hello() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json("Hello from Pano Server!"))
}

async fn list_cameras() -> Result<HttpResponse> {
    // TODO: Replace with actual chdkptp library calls
    // let cameras = chdkptp::list_devices().await?;
    
    // Pseudo code for now
    let cameras = vec![
        CameraInfo {
            name: "Canon EOS R5".to_string(),
            connected: true,
            model: "EOS R5".to_string(),
        },
        CameraInfo {
            name: "Canon EOS R6".to_string(),
            connected: false,
            model: "EOS R6".to_string(),
        }
    ];
    
    Ok(HttpResponse::Ok().json(cameras))
}

async fn capture_photo(capture_req: web::Json<CaptureRequest>) -> Result<HttpResponse> {
    // TODO: Replace with actual chdkptp library calls
    // let camera = chdkptp::connect_to_camera("EOS R5").await?;
    // camera.set_exposure_time(capture_req.exposure_time).await?;
    // camera.set_iso(capture_req.iso).await?;
    // let photo_data = camera.capture_photo().await?;
    
    // Pseudo code response for now
    let response = format!(
        "Photo captured with exposure: {}s, ISO: {}",
        capture_req.exposure_time, capture_req.iso
    );
    
    Ok(HttpResponse::Ok().json(response))
}

async fn control_servos(servo_req: web::Json<ServoRequest>) -> Result<HttpResponse> {
    // Create servo array (this would typically be stored globally in a real app)
    let mut servo_array = match ServoArray::new() {
        Ok(servo) => servo,
        Err(e) => return Ok(HttpResponse::InternalServerError().json(format!("Failed to initialize servos: {}", e))),
    };
    
    let result = match servo_req.position.as_str() {
        "reset" => servo_array.reset(),
        "power" => servo_array.power_position(),
        "battery" => servo_array.battery_position(),
        "custom" => {
            if let Some(angles) = servo_req.angles {
                servo_array.set_position(angles)
            } else {
                return Ok(HttpResponse::BadRequest().json("Custom position requires angles array"));
            }
        }
        _ => return Ok(HttpResponse::BadRequest().json("Invalid position. Use: reset, power, battery, or custom")),
    };
    
    match result {
        Ok(_) => Ok(HttpResponse::Ok().json(format!("Servos moved to {} position", servo_req.position))),
        Err(e) => Ok(HttpResponse::InternalServerError().json(format!("Failed to control servos: {}", e))),
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    println!("Starting Pano Server...");
    println!("This server will integrate with the chdkptp library for camera control");
    
    // TODO: Initialize chdkptp library
    // chdkptp::initialize().await?;
    
    println!("Starting web server on http://127.0.0.1:8080");
    
    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(hello))
            .route("/cameras", web::get().to(list_cameras))
            .route("/capture", web::post().to(capture_photo))
            .route("/servos", web::post().to(control_servos))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}
