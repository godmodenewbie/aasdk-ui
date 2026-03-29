use android_auto::{
    AndroidAutoAudioInputTrait, AndroidAutoAudioOutputTrait, AndroidAutoInputChannelTrait,
    AndroidAutoMainTrait, AndroidAutoSensorTrait, AndroidAutoVideoChannelTrait,
    AndroidAutoWiredTrait, VideoConfiguration,
};
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
use std::{collections::HashSet, net::SocketAddr, sync::Arc};
use tokio::sync::{broadcast, mpsc, Mutex};
use tower_http::services::ServeDir;
use tracing::{error, info};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TouchEvent {
    pub action: u8,
    pub x: u32,
    pub y: u32,
}

struct AppState {
    video_tx: broadcast::Sender<bytes::Bytes>,
    touch_tx: mpsc::Sender<TouchEvent>,
}

#[derive(Clone)]
struct AppHeadunit {
    video_tx: broadcast::Sender<bytes::Bytes>,
    android_send: mpsc::Sender<android_auto::SendableAndroidAutoMessage>,
    android_recv: Arc<Mutex<Option<mpsc::Receiver<android_auto::SendableAndroidAutoMessage>>>>,
    config: VideoConfiguration,
    sensors: android_auto::SensorInformation,
    input_config: android_auto::InputConfiguration,
}

#[async_trait::async_trait]
impl AndroidAutoVideoChannelTrait for AppHeadunit {
    async fn receive_video(&self, data: Vec<u8>, _timestamp: Option<u64>) {
        // Zero-copy broadcast to all Web UI Clients
        let _ = self.video_tx.send(bytes::Bytes::from(data));
    }
    async fn setup_video(&self) -> Result<(), ()> { Ok(()) }
    async fn teardown_video(&self) {}
    async fn wait_for_focus(&self) {}
    async fn set_focus(&self, _focus: bool) {}
    fn retrieve_video_configuration(&self) -> &VideoConfiguration {
        &self.config
    }
}

#[async_trait::async_trait]
impl AndroidAutoAudioOutputTrait for AppHeadunit {
    async fn open_output_channel(&self, _t: android_auto::AudioChannelType) -> Result<(), ()> { Ok(()) }
    async fn close_output_channel(&self, _t: android_auto::AudioChannelType) -> Result<(), ()> { Ok(()) }
    async fn receive_output_audio(&self, _t: android_auto::AudioChannelType, _data: Vec<u8>) {}
    async fn start_output_audio(&self, _t: android_auto::AudioChannelType) {}
    async fn stop_output_audio(&self, _t: android_auto::AudioChannelType) {}
}

#[async_trait::async_trait]
impl AndroidAutoAudioInputTrait for AppHeadunit {
    async fn open_input_channel(&self) -> Result<(), ()> { Ok(()) }
    async fn close_input_channel(&self) -> Result<(), ()> { Ok(()) }
    async fn start_input_audio(&self) {}
    async fn audio_input_ack(&self, _chan: u8, _ack: android_auto::Wifi::AVMediaAckIndication) {}
    async fn stop_input_audio(&self) {}
}

#[async_trait::async_trait]
impl AndroidAutoSensorTrait for AppHeadunit {
    fn get_supported_sensors(&self) -> &android_auto::SensorInformation {
        &self.sensors
    }
    async fn start_sensor(&self, stype: android_auto::Wifi::sensor_type::Enum) -> Result<(), ()> {
        if self.sensors.sensors.contains(&stype) {
            let mut m3 = android_auto::Wifi::SensorEventIndication::new();
            match stype {
                android_auto::Wifi::sensor_type::Enum::DRIVING_STATUS => {
                    let mut ds = android_auto::Wifi::DrivingStatus::new();
                    ds.set_status(android_auto::Wifi::DrivingStatusEnum::UNRESTRICTED as i32);
                    m3.driving_status.push(ds);
                }
                android_auto::Wifi::sensor_type::Enum::NIGHT_DATA => {
                    let mut ds = android_auto::Wifi::NightMode::new();
                    ds.set_is_night(false);
                    m3.night_mode.push(ds);
                }
                _ => {}
            }
            let m = android_auto::AndroidAutoMessage::Sensor(m3);
            let _ = self.android_send.send(m.sendable()).await;
            Ok(())
        } else {
            Err(())
        }
    }
}

#[async_trait::async_trait]
impl AndroidAutoInputChannelTrait for AppHeadunit {
    async fn binding_request(&self, _code: u32) -> Result<(), ()> { Ok(()) }
    fn retrieve_input_configuration(&self) -> &android_auto::InputConfiguration {
        &self.input_config
    }
}

#[cfg(feature = "usb")]
#[async_trait::async_trait]
impl AndroidAutoWiredTrait for AppHeadunit {}

#[async_trait::async_trait]
impl AndroidAutoMainTrait for AppHeadunit {
    async fn connect(&self) {
        info!("--- ANDROID AUTO USB CONNECTED ---");
    }

    async fn disconnect(&self) {
        info!("--- ANDROID AUTO DISCONNECTED ---");
    }

    async fn get_receiver(
        &self,
    ) -> Option<tokio::sync::mpsc::Receiver<android_auto::SendableAndroidAutoMessage>> {
        let mut inner = self.android_recv.lock().await;
        inner.take()
    }

    #[cfg(feature = "usb")]
    fn supports_wired(&self) -> Option<Arc<dyn AndroidAutoWiredTrait>> {
        Some(Arc::new(self.clone()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting Android Auto Web UI Host on 0.0.0.0:8080");

    let (video_tx, _) = broadcast::channel(1024);
    let (touch_tx, mut touch_rx) = mpsc::channel(128);

    let (android_send, android_recv) = mpsc::channel(50);
    
    // 1. Thread for handling WS -> Android Auto Touch Translation
    let android_send2 = android_send.clone();
    tokio::spawn(async move {
        while let Some(touch) = touch_rx.recv().await {
            let mut i_event = android_auto::Wifi::InputEventIndication::new();
            let timestamp: u64 = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64;
            i_event.set_timestamp(timestamp);
            let mut te = android_auto::Wifi::TouchEvent::new();
            let mut tl = android_auto::Wifi::TouchLocation::new();
            tl.set_x(touch.x);
            tl.set_y(touch.y);
            tl.set_pointer_id(0);
            te.touch_location = vec![tl];

            if touch.action == 0 {
                te.set_touch_action(android_auto::Wifi::touch_action::Enum::POINTER_DOWN);
            } else if touch.action == 1 {
                te.set_touch_action(android_auto::Wifi::touch_action::Enum::POINTER_UP);
            } else {
                te.set_touch_action(android_auto::Wifi::touch_action::Enum::DRAG);
            }

            i_event.touch_event = android_auto::protobuf::MessageField::some(te);
            let e = android_auto::AndroidAutoMessage::Input(i_event);
            if let Err(e) = android_send2.send(e.sendable()).await {
                 error!("Failed to inject touch event into AA: {:?}", e);
            }
        }
    });

    let mut sensors = HashSet::new();
    sensors.insert(android_auto::Wifi::sensor_type::Enum::DRIVING_STATUS);
    sensors.insert(android_auto::Wifi::sensor_type::Enum::NIGHT_DATA);

    let headunit = AppHeadunit {
        video_tx: video_tx.clone(),
        android_send,
        android_recv: Arc::new(Mutex::new(Some(android_recv))),
        config: VideoConfiguration {
            resolution: android_auto::Wifi::video_resolution::Enum::_1080p,
            fps: android_auto::Wifi::video_fps::Enum::_30,
            dpi: 160,
        },
        sensors: android_auto::SensorInformation { sensors },
        input_config: android_auto::InputConfiguration {
            keycodes: vec![1, 2, 3, 4, 5],
            touchscreen: Some((1920, 1080)),
        },
    };

    // 2. Start Android Auto Logic via MainTrait Loop
    tokio::task::spawn(async move {
        let config = android_auto::AndroidAutoConfiguration {
            make: "Debian".to_string(),
            model: "VM Headunit".to_string(),
            year: "2024".to_string(),
        };
        let setup = android_auto::AndroidAutoSetup {
            id: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
            display: "Headunit Display".to_string(),
            setup: Default::default(),
        };

        let mut joinset = tokio::task::JoinSet::new();
        info!("Wait for USB Phone connection...");
        let b = Box::new(headunit);
        let _ = b.run(config, &mut joinset, &setup).await;
        joinset.join_all().await;
        info!("Android auto session task ended.");
    });

    // 3. Web UI Server
    let state = Arc::new(AppState { video_tx, touch_tx });
    let app = Router::new()
        .nest_service("/", ServeDir::new("public"))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut video_rx = state.video_tx.subscribe();

    let mut send_task = tokio::spawn(async move {
        while let Ok(frame) = video_rx.recv().await {
            if sender.send(WsMessage::Binary(frame.to_vec())).await.is_err() {
                break;
            }
        }
    });

    let touch_tx = state.touch_tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(WsMessage::Text(msg))) = receiver.next().await {
            if let Ok(touch_event) = serde_json::from_str::<TouchEvent>(&msg) {
                if touch_tx.send(touch_event).await.is_err() {
                    break;
                }
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}
