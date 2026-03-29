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
MIIDtTCCAp2gAwIBAgIUA0WUNdRxHra38QPH0C9Pi/ZAp6wwDQYJKoZIhvcNAQEL\n\
BQAwWzELMAkGA1UEBhMCVVMxEzARBgNVBAgMCkNhbGlmb3JuaWExFjAUBgNVBAcM\n\
DU1vdW50YWluIFZpZXcxHzAdBgNVBAoMFkdvb2dsZSBBdXRvbW90aXZlIExpbmsw\n\
HhcNMjYwMzI5MTExNTA3WhcNMzYwMzI2MTExNTA3WjBTMQswCQYDVQQGEwJKUDEO\n\
MAwGA1UECAwFVG9reW8xETAPBgNVBAcMCEhhY2hpb2ppMRQwEgYDVQQKDAtKVkMg\n\
S2Vud29vZDELMAkGA1UECwwCMDEwggEiMA0GCSqGSIb3DQEBAQUAA4IBDwAwggEK\n\
AoIBAQCpwzTLcgpSUtU39MLC2x7XWT7dJyfLg2Lgy22GwjNPXHql/Q2WA4qOt0sf\n\
QA0IhiBu8KSQBC7sDTxfr2sZRmA+XaaBUmBj85Q2EO367395KIZ0kRXBJzqOxcKB\n\
AzL0TVix7lrhekLNK73zNp/21bpp/fCNIWlk1K5rbFjLLWgVl9v180oPMSRF+TtB\n\
KARNqniqOvapizC5ROfF+6nCiS0W7o7kz3bU/uoKxgHMvYKh7uUowcCTsML3cEYt\n\
1HGaEZOQEHzQvislGCBHX2Lwtd5/n4sBuNYlmxDA+qB2O3Uh/zi6Sa+57Wo5ThAY\n\
svybLvdSUaH4qIlmLgP/hYwEPvezAgMBAAGjeTB3MAkGA1UdEwQCMAAwCwYDVR0P\n\
BAQDAgTwMB0GA1UdJQQWMBQGCCsGAQUFBwMCBggrBgEFBQcDATAdBgNVHQ4EFgQU\n\
awE+4vUzcl4LqOnXfnD+ShtOhE0wHwYDVR0jBBgwFoAUjr/BdTbLVgLa8sGzFNDb\n\
cmV8NQUwDQYJKoZIhvcNAQELBQADggEBAEl5axocVAAP5lDpdA9HVjML+fE/Fcdz\n\
8v/Doc5DtV/dQLK+kQg7rLKBreSr5+/EMeqHTUkPTnnsjvIFUlvd/yF0p9+xQRYk\n\
WyWidwuM3lyI9/y91QA9muFI7v83CEv8k/V5OSysmSuJapsUj738wzP+jV0YR6Em\n\
NCn5ZANDpNXenG/R0qFe69EpfIVH3b+0jWQoUyhYfKRNdb37HdCxiwk14f0me8rP\n\
hO49WtcnaDQ+OxhcuSgttZvlkWrQ5upLg0Bg34BNcd6TstDHywpLeYrI3irAsTVH\n\
mxKGPoMpI25VykZamJxhPSGy0BgUYWZArMrM9nMSuGT0OIDGmdjUeEI=\n\
-----END CERTIFICATE-----\n";

const KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQCpwzTLcgpSUtU3\n\
9MLC2x7XWT7dJyfLg2Lgy22GwjNPXHql/Q2WA4qOt0sfQA0IhiBu8KSQBC7sDTxf\n\
r2sZRmA+XaaBUmBj85Q2EO367395KIZ0kRXBJzqOxcKBAzL0TVix7lrhekLNK73z\n\
Np/21bpp/fCNIWlk1K5rbFjLLWgVl9v180oPMSRF+TtBKARNqniqOvapizC5ROfF\n\
+6nCiS0W7o7kz3bU/uoKxgHMvYKh7uUowcCTsML3cEYt1HGaEZOQEHzQvislGCBH\n\
X2Lwtd5/n4sBuNYlmxDA+qB2O3Uh/zi6Sa+57Wo5ThAYsvybLvdSUaH4qIlmLgP/\n\
hYwEPvezAgMBAAECggEAE3K9laEW9Z9vtd1ggpo/ykP7I7LcqEABD+e+QHX3Etxx\n\
YJrA97KoKPlurcHUvGlBRfRjpewUxA4wIHYkOt0JIZvw+1fImyrIi/kcimbtn5+4\n\
55nHeD1aRAj743POXpaN1rSLzNEI3iBovng/kzOhC4uAB2sQe/CxmrTq5zvodLCk\n\
7IKKcI23vWioGOeTul+CBaMOXz+rAsqjfNfytxQl3q/QewHh9+iP1cYpP+Syjc8n\n\
8Ye4DcJNAW8obsXSD5n/VRl+slt0IzDmNx4zLo8a5PMM6ZQjIqmqxBYdhTWUW+iY\n\
OMo/BErpiyaFEa/O9T6NuqOzIWjrzBE+c6+C0LSEoQKBgQDr89lyeAM6uohCLknz\n\
dVZsSe5BwVu5LgZ7zqBDkXT7uYZ57DK8JHkQoqa5ymQKwf47oOX7l0KBwl3IFY/Q\n\
nQu3dyz7t0HcYg9jVrZxKvCOAuRTnpIzI7w8miRAFvAUlwJgAQdBUEVYVSYQmFH9\n\
lFI3piYUwATzAg2SzXadV05jjwKBgQC4L6w43a++VusgBwNHMoG5jLmuJTe1pyWo\n\
Z1nHD24KQuPQaIGOB5aiOfR9IWEorzD6P7iHA/z1YnvbUXlh7b7WiT57YFV9CyXg\n\
iWZx84TjyMT5CV82nrBoe14p+f0sZFt+y/8y3G3n8ygggBOBg5sLdqhJQyOdUC24\n\
52Mift0HnQKBgBhCP+8G674UA4JaY/wF6lbD2x0jlhyZ4MzF17Bauh5PWsYaRLUX\n\
QuM09dNQPazleRAEYODXEl1o8F9r6BdYriW0uQlANCNGabKa7bMA6S6QmY0HVpyv\n\
ZeENMADu2swjInlgYbCTYi3Mw1cdcgCSSUmzaWLkwx2A7ohTW4idu099AoGBAI7M\n\
Fx/3b5uIU75+8WGvnLe4jPSg0jI5po6LoiUcp1m5Rlp7y4XMCFM5z3179ZHPUY+S\n\
+4Nh6ips8k21OwBbjItT2Gda5qyNig4tOIm8HRlkvKG/TFxSZ755dyXgNRLHs8/4\n\
ZKCQGX2tHT0lTvooiHo4wnwaW3BJi0lBy7Ag30hZAoGBAOVsU4VQALgsDyojXBQi\n\
vDaejzKhBELHSo8a1LbfiyZWAL/gtDgsX/3+o3dqUIEL5ko+Bd28Bxixditbyu27\n\
M7EsRkIwk6eyD4VnubhAWbJZg2ZvBeLWfdvdBPcQQXnvCXMwYw4gjxKbZYx7gD0P\n\
x7065yanuTIWabH7lKDVcGZN\n\
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
