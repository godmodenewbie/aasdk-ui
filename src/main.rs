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

const CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\n\
MIIDKTCCAhGgAwIBAgIUcCv02YtqjLw0zJQiObZFcxPCCQEwDQYJKoZIhvcNAQEL\n\
BQAwJDEQMA4GA1UECgwHRXhhbXBsZTEQMA4GA1UEAwwHRXhhbXBsZTAeFw0yNjAz\n\
MjkxMTMyNDRaFw0zNjAzMjYxMTMyNDRaMCQxEDAOBgNVBAoMB0V4YW1wbGUxEDAO\n\
BgNVBAMMB0V4YW1wbGUwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEKAoIBAQCs\n\
d0MFVD9Rw6J1kVWpJuNUn4LPfNDrWm03tMgmOWDjFywqmBsRaZnwoIovI0UhTXac\n\
Nz02PO14OGv+0IdkWOvT0S0G7h9NkKsQ2NFQmvvfKRjiVq52y+x27YVkEWsH4bZ1\n\
y2ucIoBuA24b6k9nZsqGChOhrHE6waIWNwrh8d+2Oae8F5iWZiQfjZj4m534PQ1r\n\
TrnGHEdexKJmNoSlAChdtDy2v+7az9uhqdcvtFlSC8Aa/+FyBb2s4rj5yaRHL5++\n\
HhRsS0cchkijEJL0V9S3GQvnxJ0lirLmWoojtHCbWvtc6YdiNiLl14jURg+U6MMs\n\
JTXyKeVbhWwvc8Vss/z9AgMBAAGjUzBRMB0GA1UdDgQWBBR2fy86e4/rGI7FBM53\n\
4GkMxd4zjTAfBgNVHSMEGDAWgBR2fy86e4/rGI7FBM534GkMxd4zjTAPBgNVHRMB\n\
Af8EBTADAQH/MA0GCSqGSIb3DQEBCwUAA4IBAQCbX9SeIMaRb9RSg4oxxukXwPzj\n\
JonOdiontBZAeYsIY+OMyUeApsD8aVk7BnhzHxcayP/d//sgsGUZV2f34/V5Whg5\n\
gewY8MXptaiAng3RXadbPHVU75P5BPrBZZ+FUSuTo2i3s0a1R3WSd+UEjhY45TCa\n\
cxmnolFClzN3ACZpqbYaBcWRw4iiZuesCJh5b2+qXA52mHk7aa/Zu4xiDYodJ/2y\n\
/3gfCp+Smpu5TLab6xU7COXAG2rvIknMNVCZxUVMYgW/dYKT3723pzWnXYUwP20R\n\
VpzCr4vR3DQW6Ru7onFJhgxd+fHCR0v98vMPM4u8Hs6TmGpsobiUp/PZ0WM9\n\
-----END CERTIFICATE-----\n";

const KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCsd0MFVD9Rw6J1\n\
kVWpJuNUn4LPfNDrWm03tMgmOWDjFywqmBsRaZnwoIovI0UhTXacNz02PO14OGv+\n\
0IdkWOvT0S0G7h9NkKsQ2NFQmvvfKRjiVq52y+x27YVkEWsH4bZ1y2ucIoBuA24b\n\
6k9nZsqGChOhrHE6waIWNwrh8d+2Oae8F5iWZiQfjZj4m534PQ1rTrnGHEdexKJm\n\
NoSlAChdtDy2v+7az9uhqdcvtFlSC8Aa/+FyBb2s4rj5yaRHL5++HhRsS0cchkij\n\
EJL0V9S3GQvnxJ0lirLmWoojtHCbWvtc6YdiNiLl14jURg+U6MMsJTXyKeVbhWwv\n\
c8Vss/z9AgMBAAECggEAAqx2pYaA1MuroRb3tP+dVpqCdKUCuCNWvh5XXABXuC2L\n\
yb1B7iss78YNXl21nKaOyC0zDbw0EkENq42gC7Y1Mbt0bz8RzSoI/OHfnNhKP1Nr\n\
x1aArebLa6yS/NIoTp75LSpSKMGALDRxaI1hXcECMsHFPCRoPPjzglSoHoiZZ0HH\n\
C70dCHov8jkgwpg8ARAm4lYMSiffpRpgO04sURc6607xAyPFqbnY+6Gj0UoMTYQs\n\
WkwciIJ7fX+pZTvpW4rgeWm8CeHyfi65r8as/hqhypf5XFfLGyfNZYIxIhzu1aW9\n\
k7LTBDogAOorN+MDyFzbJgoGFwmG0VEAnRoCyWL/AQKBgQDWZEi4zReunMjRRzAi\n\
DGZqdi1KD6NPgfDNTHqTAET8DDV4bGHlZbHHI+YHKX9h3r310WL5klkbb3eFSZeH\n\
quzjauAoCsgXVxZaCxNCWFXLGvKSto8t3foRLm7Jik5cn2XGYC80W6wQIPIHB3St\n\
q7qNOVQRH4D4MjCqn3nz5iJXKQKBgQDN7/T6mR/9eGtV34gOvnHcyY6iDh3OSy82\n\
cNAyVTknpbASYVnNnY5HhSrBLQs+HIG6jAQA6gBJjUEmXDZ/hQVASJtKNaxqMfsN\n\
NSBllAoge3QXWTue6huUSSiRJv0FOUsSW13sdiAwPUtEz0djE1msJexS6vIIqmJh\n\
SMNr4dYVtQKBgQDOFtLNSuHkCXUFsC/12wOsfXOlyQiNCnUHdOgzXUPzIm1YGJ+2\n\
m55ctwaNhfechjkHD0PcczFTLUCwkQCn+sgDCR73fv2/agjjf9gAo9e9CWd7XyCd\n\
z89uKrt244vWf6efHaDi7OinDHR8C0+/DuCilyRX3XflnqGnsuvRaD1EmQKBgBaP\n\
AYvt+CYg6ckXWmUbEYf5AEnaOAOgEsTo6LWKxl8EdFwfE+JFLw/Ak6VjlMayArf3\n\
nHypJWzpL0jPcxzW6nNXQMOJS6C6ZuDUf/8Aj3dtbpMcMD7BMFI3DV2RIshOtV2G\n\
aqx7aB1AqZ0ZA53jwb/sy41ttSOj3nD/soB/1Z69AoGALOn3YmInC8NM5fffIxHG\n\
DUg/0uzEvLQB83wZJaieUY8R00236POC7lIQLqc95DZ54HeOYhQ5uRwJkY80RfB2\n\
Ufdydug9NAhtGo1pPH25Rzz7yI6n6HUuDw1Dp+cVutR7RVgIox1iKNpPkfPWbdjC\n\
5Fx6mE/RgmfBz1j32OXgBtg=\n\
-----END PRIVATE KEY-----\n";

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

    fn supports_wired(&self) -> Option<Arc<dyn AndroidAutoWiredTrait>> {
        Some(Arc::new(self.clone()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    info!("Starting Android Auto Web UI Host on 0.0.0.0:8080");

    let (video_tx, _) = broadcast::channel(1024);
    let (touch_tx, mut touch_rx) = mpsc::channel::<TouchEvent>(128);

    let (android_send, android_recv) = mpsc::channel::<android_auto::SendableAndroidAutoMessage>(50);
    
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
            unit: android_auto::HeadUnitInfo {
                name: "Example".to_string(),
                car_model: "Example".to_string(),
                car_year: "1943".to_string(),
                car_serial: "42".to_string(),
                left_hand: false,
                head_manufacturer: "Example".to_string(),
                head_model: "Example".to_string(),
                sw_build: "37".to_string(),
                sw_version: "1.2.3".to_string(),
                native_media: true,
                hide_clock: Some(true),
            },
            custom_certificate: Some((
                CERT_PEM.as_bytes().to_vec(),
                KEY_PEM.as_bytes().to_vec()
            )),
        };
        let setup = android_auto::setup();

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
