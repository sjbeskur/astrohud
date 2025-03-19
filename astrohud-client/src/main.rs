use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use futures_util::{StreamExt, SinkExt};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use bytes::Bytes;  // Add this import
mod cli;
use cli::Cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get image path from command line arguments
    let args = Cli::parse_args();
    
    if !Path::new(&args.image_path).exists() {
        eprintln!("Image file does not exist: {}", args.image_path);
        std::process::exit(1);
    }

    // Connect to WebSocket server
    let url = format!("ws://{}/ws/", args.endpoint);
    let (mut ws_stream, _) = match connect_async(&url).await {
        Ok((stream, response)) => {
            println!("Connected to WebSocket server: {:?}", response);
            (stream, response)
        }
        Err(e) => {
            eprintln!("Failed to connect to websocket endpoint at: {} \n Error: {}", &url, e);
            std::process::exit(1);
        }
    };

    // Read and send the image
    match send_image(&mut ws_stream, &args.image_path).await {
        Ok(()) => println!("Image sent successfully"),
        Err(e) => eprintln!("Failed to send image: {}", e),
    }

    // Wait for response
    if let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(Message::Text(text)) => println!("Server response: {}", text),
            Ok(_) => println!("Received unexpected message type"),
            Err(e) => eprintln!("Error receiving message: {}", e),
        }
    }

    // Clean up
    if let Err(e) = ws_stream.close(None).await {
        eprintln!("Error closing connection: {}", e);
    }
    
    Ok(())
}

async fn send_image(
    ws_stream: &mut tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    image_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::open(image_path)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    let bytes = Bytes::from(buffer);
    ws_stream.send(Message::Binary(bytes)).await?;
    Ok(())
}