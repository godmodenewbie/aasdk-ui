use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{sink::SinkExt, stream::StreamExt};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{broadcast, mpsc};
use tower_http::services::ServeDir;
use tracing::{error, info};

// Assuming the `android-auto` crate provides channels logic similar to this:
// use android_auto::usb::UsbConnection;
// use android_auto::channels::{VideoChannel, InputChannel};

/// Android Auto protocol defines actions for touch events.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TouchEvent {
    pub action: u8, // Typically 0 = down, 1 = up, 2 = move
    pub x: u32,
    pub y: u32,
}

struct AppState {
    /// Broadcast channel for H.264 NAL units coming from Android Auto UI Video Channel
    video_tx: broadcast::Sender<bytes::Bytes>,
    /// MPSC channel to send touch events back to Android Auto Input Channel
    touch_tx: mpsc::Sender<TouchEvent>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting Android Auto Web UI Bridge");

    // Channels bridging AA and WS clients
    let (video_tx, _) = broadcast::channel(1024); // Large buffer for zero-copy H.264 frame drops management
    let (touch_tx, mut touch_rx) = mpsc::channel(128);

    let state = Arc::new(AppState {
        video_tx: video_tx.clone(),
        touch_tx,
    });

    // 1. Spawn Android Auto Background Task
    let video_tx_aa = video_tx.clone();
    tokio::spawn(async move {
        info!("Android Auto USB listener starting...");
        
        // --- BOILERPLATE FOR ANDROID-AUTO CRATE ---
        // let mut aa = android_auto::usb::UsbConnection::connect().await.expect("Failed to bind USB");
        // let mut video_channel = aa.open_video_channel();
        // let mut input_channel = aa.open_input_channel();
        
        loop {
            tokio::select! {
                // Read video frames from AA and broadcast
                // Some(frame) = video_channel.next() => {
                //       Zero-copy conversion into bytes, ready for broadcast to all WS clients
                //      let _ = video_tx_aa.send(bytes::Bytes::from(frame.payload));
                // }
                
                // Read touch events from WS clients and send to AA
                Some(touch_event) = touch_rx.recv() => {
                     info!("Injecting touch event into AA: {:?}", touch_event);
                     // Example of using the `android-auto` InputChannel to send touch:
                     // input_channel.send_touch(touch_event.action, touch_event.x, touch_event.y).await.unwrap();
                }
            }
        }
    });

    // 2. Setup HTTP/WebSocket Server
    let app = Router::new()
        // Serve the frontend HTML/JS files statically from "public"
        .nest_service("/", ServeDir::new("public"))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Listening on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}

/// Upgrades the HTTP request to a WebSocket connection
async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handles the actual WebSocket duplex stream for a single Web UI client
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut video_rx = state.video_tx.subscribe();

    // Stream H.264 Frames to Client
    let mut send_task = tokio::spawn(async move {
        while let Ok(frame) = video_rx.recv().await {
            // Forward raw H.264 binary frame via zero-copy Bytes payload
            if sender.send(WsMessage::Binary(frame.to_vec())).await.is_err() {
                break;
            }
        }
    });

    // Receive JSON Touch Events from Client
    let touch_tx = state.touch_tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(WsMessage::Text(msg))) = receiver.next().await {
            match serde_json::from_str::<TouchEvent>(&msg) {
                Ok(touch_event) => {
                    if touch_tx.send(touch_event).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to parse touch event JSON: {}", e);
                }
            }
        }
    });

    // Automatically disconnect and cleanup if either channel dies
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}
