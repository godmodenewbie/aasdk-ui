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
MIIDmTCCAoGgAwIBAgIUAdS8BSFFo9bt6vh1hhWQ+MXXADcwDQYJKoZIhvcNAQEL\n\
BQAwXDELMAkGA1UEBhMCVVMxCzAJBgNVBAgMAkNBMQswCQYDVQQHDAJTRjEOMAwG\n\
A1UECgwFQUFTREsxETAPBgNVBAsMCEhlYWR1bml0MRAwDgYDVQQDDAdBQVNES1VJ\n\
MB4XDTI2MDMyOTExMDMyOVoXDTM2MDMyNjExMDMyOVowXDELMAkGA1UEBhMCVVMx\n\
CzAJBgNVBAgMAkNBMQswCQYDVQQHDAJTRjEOMAwGA1UECgwFQUFTREsxETAPBgNV\n\
BAsMCEhlYWR1bml0MRAwDgYDVQQDDAdBQVNES1VJMIIBIjANBgkqhkiG9w0BAQEF\n\
AAOCAQ8AMIIBCgKCAQEAxm79xdHTvotuScdTYvTpwQB48EJMzOkybMP41NE3cQ2l\n\
8Uj1i0zWIg+9/8E9NaEgVmgYKWTiRtD33rFgLpzRYfNHvteHKNbaYQMu3oHAGDXW\n\
tDa7Uz9bBLdy5UmBU+asUJadNQwGF9Jcrf1cI1H+8VFZNxno7B8Cfv/8tEPJHcHk\n\
DMrYLmSn4EXHxN7iVIVEpFoC28FNK5N4YTnEdU1aD/QikQNVtH+wnjMh/snLMP/3\n\
c5F/NVEbY+efzc89e2QFrumGXl+8mz4cU/I1dq/nk5q1iKZKu8YsoaJbpUDi3g/V\n\
2Md8Avv7b435ZckM8/WJakJ4MPqREfaHrwdz3xa/DQIDAQABo1MwUTAdBgNVHQ4E\n\
FgQUsPYIIMh8XlwDP6pxzOaJUyqA6O8wHwYDVR0jBBgwFoAUsPYIIMh8XlwDP6px\n\
zOaJUyqA6O8wDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOCAQEAgR29\n\
5I8lJwAzgIt8s+r4IMs8NPzMSveawIGVfxdbS80itg9JAUx0EPNChi6CqlH9dgwe\n\
9G8J0SAZ3lNW4T9DwCSh1X28KPIIJDpY1+pvWeRv5IZS3rCkaiGU9GKbXJAAMDGU\n\
ICCryNGmxKMlH8W1V7v3CuZNAEWJZBK8grVAXN+O4KxCbs44CRqT0QMTgaoF0iCv\n\
H8UIWKO5kbZHamcfUwR290c5zLXCb8z23mdmNo65CkHpP8mSR6+4i7SYMk8RJs1Q\n\
KnkpXf74ajrf4+FJwDlOvrXO0KmKTT/Z+5kPmTxAWnj/4X+rk87/WgoaoUPFR4BY\n\
4lO850Hv5LVFFsYXJg==\n\
-----END CERTIFICATE-----\n";

const KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQDGbv3F0dO+i25J\n\
x1Ni9OnBAHjwQkzM6TJsw/jU0TdxDaXxSPWLTNYiD73/wT01oSBWaBgpZOJG0Pfe\n\
sWAunNFh80e+14co1tphAy7egcAYNda0NrtTP1sEt3LlSYFT5qxQlp01DAYX0lyt\n\
/VwjUf7xUVk3GejsHwJ+//y0Q8kdweQMytguZKfgRcfE3uJUhUSkWgLbwU0rk3hh\n\
OcR1TVoP9CKRA1W0f7CeMyH+ycsw//dzkX81URtj55/Nzz17ZAWu6YZeX7ybPhxT\n\
8jV2r+eTmrWIpkq7xiyholulQOLeD9XYx3wC+/tvjfllyQzz9YlqQngw+pER9oev\n\
B3PfFr8NAgMBAAECggEAMTJU5A3q3a+zcwLCY4Mdji54DXcek+IQEKPApkDNqk+M\n\
MAda60OsRksZW9aUvp5ZRlrt/JtIt255OcLHuh7CkbKPe9rzJVapU0qG/P71uXrl\n\
pY35QQEw53k0+PBRqlPDLoK87KkzvIW42SE6zf33A2zb/duEYkAo7gQ46pdwvhnD\n\
kS2D662V141IVc1+lfkQNu+JPLE+Chgv/lfre37XoxZglFsOSO8sSNmA0wYJ2Fdk\n\
ugFf/6xCM2XgB+zzWlz+/0BEZhflctIW5dlNoTKd8H9XHZbci1BRiINexb0jINcM\n\
YRt+sei36t3u00uLzKS015OZTX2k2fg8ec9JpIbMQQKBgQDltWO4nex4eLbWKEEr\n\
csaWfy3XcSmn25p0NVxyv24rujLidCeRG9kn+/aMNEuMesGfIMwUgK7rFivh3HHa\n\
iBSiRSEADkEwmccjGhGg4BbjEbbEF3BxabhEr3ZlYwywZGHER3/hJhg1EZLWHwES\n\
37yVm8CHqcHmAo451Rshcbe7TQKBgQDdJTlcGhoXvtOMlPszn3Z+R6ZLxEv/7tcU\n\
1xjdtiqBk5le2yLSv7WMm5NOCS/FyjzMxi3NGSRY4zByxbfhyRVtBzBW6iQPiJRp\n\
y+V/GgTJjGAQQtyhjA15pOJqHYK6lHYrHttoJ7fW3qSkCfyKMSRE/pUlwMHr0U88\n\
5smN3zOywQKBgF7XpOPN+JvJI5yKpFW/HvV2b0P7yjovNrdybMhH98IAMBBF6yxD\n\
tkaHBsXeta675IPCM+DnPNF9pwKrVSrocrSJHFX8jLf3VjxNAChPPcPlRXPzRY7e\n\
GqHpXFYCLnQKDj/PUaJxax9GMT1NMdFMJX4T/8tDsPY56eVA8uG9JSIlAoGBAIiI\n\
iOdyPhXW/SlYedcfZrsEZYl1wi5bOXNmcbXA2HFzvUcxKEjRj7cl/kY5qcMF34/V\n\
80UjdqtiaPETXToLOi08OP4QRP9KJcdD2YclezssbcrcXPdoTpGB2UAGxEWJj4OD\n\
45Zknz4L675TZBW1zVzDiTXr0k5TxgYlvt7WpUaBAoGBAKKe++rU0lv05AJAGAwv\n\
wti/VseHj2Zc7XIZAEz0/Sy2MvUQeYhb8CkGzxB1YWwWq4VbNrbzx878DGlD419/\n\
42tMWuWynTaDHxGPbgv5Bf+5N07VfCa8sxyU3ARkHoNW3NMzhCz7X+UG9a7kTWuf\n\
DuBhrgYjfQxDvlaIyELn55Ou\n\
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
                name: "Debian".to_string(),
                car_model: "VM Headunit".to_string(),
                car_year: "2024".to_string(),
                car_serial: "AASDK-1".to_string(),
                left_hand: true,
                head_manufacturer: "Linux".to_string(),
                head_model: "AASDK-UI".to_string(),
                sw_build: "1.0".to_string(),
                sw_version: "1.0".to_string(),
                native_media: false,
                hide_clock: None,
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
