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
use tracing::{debug, error, info, warn};

// =====================================================================
//  DEVICE SPOOFING: Pioneer AVH-W4500NEX OEM Head Unit Identifiers
//  These values mimic a real certified Pioneer head unit to pass
//  Android Auto's "Communication Error 7" (device not certified) check.
//  The phone verifies these fields during the AA handshake (Service List
//  negotiation), not during TLS. Using real OEM values significantly
//  increases compatibility.
// =====================================================================
const HU_NAME:         &str = "AVH-W4500NEX";
const HU_CAR_MODEL:    &str = "AVH-W4500NEX";
const HU_CAR_YEAR:     &str = "2019";
const HU_CAR_SERIAL:   &str = "PIONEER0001";
const HU_MANUFACTURER: &str = "Pioneer";
const HU_MODEL:        &str = "AVH-W4500NEX";
const HU_SW_BUILD:     &str = "1";
const HU_SW_VERSION:   &str = "1.0";

// =====================================================================
//  SSL CERTIFICATE NOTE:
//  We set custom_certificate: None here, which means the android-auto
//  crate will use its own built-in JVC Kenwood certificate (the real OEM
//  cert signed by Google Automotive Link CA).
//
//  The UnsupportedCertVersion panic was caused because rustls-webpki
//  does NOT support the X.509 v1 format used by that 2014 cert.
//  This is FIXED by the [patch.crates-io] in Cargo.toml which replaces
//  rustls-webpki with uglyoldbob's fork that supports v1 certs.
//
//  DO NOT use a custom_certificate unless you have a cert signed by the
//  real Google Automotive Link CA root (which is not publicly available).
// =====================================================================

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
    // Shared sender updated each session — touch task writes here
    aa_msg_tx: Arc<Mutex<Option<mpsc::Sender<android_auto::SendableAndroidAutoMessage>>>>,
    android_recv: Arc<Mutex<Option<mpsc::Receiver<android_auto::SendableAndroidAutoMessage>>>>,
    config: VideoConfiguration,
    sensors: android_auto::SensorInformation,
    input_config: android_auto::InputConfiguration,
}

#[async_trait::async_trait]
impl AndroidAutoVideoChannelTrait for AppHeadunit {
    async fn receive_video(&self, data: Vec<u8>, _timestamp: Option<u64>) {
        debug!("[VIDEO] Received H.264 NAL unit: {} bytes", data.len());
        let _ = self.video_tx.send(bytes::Bytes::from(data));
    }

    async fn setup_video(&self) -> Result<(), ()> {
        info!("[VIDEO] setup_video called - accepting video channel");
        Ok(())
    }

    async fn teardown_video(&self) {
        info!("[VIDEO] teardown_video called");
    }

    async fn wait_for_focus(&self) {
        info!("[VIDEO] wait_for_focus called");
    }

    async fn set_focus(&self, focus: bool) {
        info!("[VIDEO] set_focus: {}", focus);
    }

    fn retrieve_video_configuration(&self) -> &VideoConfiguration {
        &self.config
    }
}

#[async_trait::async_trait]
impl AndroidAutoAudioOutputTrait for AppHeadunit {
    async fn open_output_channel(&self, t: android_auto::AudioChannelType) -> Result<(), ()> {
        info!("[AUDIO-OUT] open_output_channel: {:?}", t);
        Ok(())
    }
    async fn close_output_channel(&self, t: android_auto::AudioChannelType) -> Result<(), ()> {
        info!("[AUDIO-OUT] close_output_channel: {:?}", t);
        Ok(())
    }
    async fn receive_output_audio(&self, _t: android_auto::AudioChannelType, data: Vec<u8>) {
        debug!("[AUDIO-OUT] received {} bytes of audio (discarding - no audio hardware)", data.len());
    }
    async fn start_output_audio(&self, t: android_auto::AudioChannelType) {
        info!("[AUDIO-OUT] start_output_audio: {:?}", t);
    }
    async fn stop_output_audio(&self, t: android_auto::AudioChannelType) {
        info!("[AUDIO-OUT] stop_output_audio: {:?}", t);
    }
}

#[async_trait::async_trait]
impl AndroidAutoAudioInputTrait for AppHeadunit {
    async fn open_input_channel(&self) -> Result<(), ()> {
        info!("[AUDIO-IN] open_input_channel");
        Ok(())
    }
    async fn close_input_channel(&self) -> Result<(), ()> {
        info!("[AUDIO-IN] close_input_channel");
        Ok(())
    }
    async fn start_input_audio(&self) {
        info!("[AUDIO-IN] start_input_audio");
    }
    async fn audio_input_ack(&self, chan: u8, _ack: android_auto::Wifi::AVMediaAckIndication) {
        debug!("[AUDIO-IN] audio_input_ack chan={}", chan);
    }
    async fn stop_input_audio(&self) {
        info!("[AUDIO-IN] stop_input_audio");
    }
}

#[async_trait::async_trait]
impl AndroidAutoSensorTrait for AppHeadunit {
    fn get_supported_sensors(&self) -> &android_auto::SensorInformation {
        &self.sensors
    }

    async fn start_sensor(&self, stype: android_auto::Wifi::sensor_type::Enum) -> Result<(), ()> {
        info!("[SENSOR] start_sensor: {:?}", stype);
        if self.sensors.sensors.contains(&stype) {
            let mut m3 = android_auto::Wifi::SensorEventIndication::new();
            match stype {
                android_auto::Wifi::sensor_type::Enum::DRIVING_STATUS => {
                    info!("[SENSOR] -> Reporting DRIVING_STATUS: UNRESTRICTED");
                    let mut ds = android_auto::Wifi::DrivingStatus::new();
                    ds.set_status(android_auto::Wifi::DrivingStatusEnum::UNRESTRICTED as i32);
                    m3.driving_status.push(ds);
                }
                android_auto::Wifi::sensor_type::Enum::NIGHT_DATA => {
                    info!("[SENSOR] -> Reporting NIGHT_DATA: Day mode (is_night=false)");
                    let mut ds = android_auto::Wifi::NightMode::new();
                    ds.set_is_night(false);
                    m3.night_mode.push(ds);
                }
                _ => {
                    warn!("[SENSOR] Unhandled sensor type: {:?}", stype);
                }
            }
            let m = android_auto::AndroidAutoMessage::Sensor(m3);
            let tx = self.aa_msg_tx.lock().await;
            if let Some(tx) = tx.as_ref() {
                let _ = tx.send(m.sendable()).await;
            }
            Ok(())
        } else {
            warn!("[SENSOR] Sensor {:?} not in supported list, rejecting", stype);
            Err(())
        }
    }
}

#[async_trait::async_trait]
impl AndroidAutoInputChannelTrait for AppHeadunit {
    async fn binding_request(&self, code: u32) -> Result<(), ()> {
        info!("[INPUT] binding_request: keycode={}", code);
        Ok(())
    }
    fn retrieve_input_configuration(&self) -> &android_auto::InputConfiguration {
        &self.input_config
    }
}

#[async_trait::async_trait]
impl AndroidAutoWiredTrait for AppHeadunit {}

#[async_trait::async_trait]
impl AndroidAutoMainTrait for AppHeadunit {
    async fn connect(&self) {
        info!("╔══════════════════════════════════════════╗");
        info!("║   ANDROID AUTO USB SESSION ESTABLISHED!  ║");
        info!("║   Spoofing: {} {}             ║", HU_MANUFACTURER, HU_MODEL);
        info!("╚══════════════════════════════════════════╝");
    }

    async fn disconnect(&self) {
        warn!("══ ANDROID AUTO SESSION DISCONNECTED ══");
    }

    async fn get_receiver(
        &self,
    ) -> Option<tokio::sync::mpsc::Receiver<android_auto::SendableAndroidAutoMessage>> {
        let mut inner = self.android_recv.lock().await;
        inner.take()
    }

    fn supports_wired(&self) -> Option<Arc<dyn AndroidAutoWiredTrait>> {
        info!("[WIRED] supports_wired() called -> returning wired support");
        Some(Arc::new(self.clone()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Init logging with env filter - use RUST_LOG=debug for full trace
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("══════════════════════════════════════════════════════");
    info!("  Android Auto WebUI Headunit Bridge - Starting");
    info!("  Spoofing: {} {} ({})", HU_MANUFACTURER, HU_MODEL, HU_CAR_YEAR);
    info!("  WebUI:    http://0.0.0.0:8080");
    info!("  TIP: Run with RUST_LOG=debug for more detail");
    info!("══════════════════════════════════════════════════════");

    let (video_tx, _) = broadcast::channel(1024);
    let (touch_tx, mut touch_rx) = mpsc::channel::<TouchEvent>(128);
    // ── Shared AA message sender: updated each session so touch task always has valid sender ──
    // Arc<Mutex<Option<Sender>>> — None between sessions, Some(sender) during active session
    let aa_msg_tx: Arc<Mutex<Option<mpsc::Sender<android_auto::SendableAndroidAutoMessage>>>> =
        Arc::new(Mutex::new(None));

    // ── Touch Event Translator: WebSocket JSON → Android Auto protobuf ──
    // Uses aa_msg_tx which is updated each session, so no SendError after reconnect
    let aa_msg_tx_touch = aa_msg_tx.clone();
    tokio::spawn(async move {
        info!("[TOUCH] Touch event translator started");
        while let Some(touch) = touch_rx.recv().await {
            debug!("[TOUCH] Injecting touch: action={} x={} y={}", touch.action, touch.x, touch.y);
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

            let action = match touch.action {
                0 => android_auto::Wifi::touch_action::Enum::POINTER_DOWN,
                1 => android_auto::Wifi::touch_action::Enum::POINTER_UP,
                _ => android_auto::Wifi::touch_action::Enum::DRAG,
            };
            te.set_touch_action(action);
            i_event.touch_event = android_auto::protobuf::MessageField::some(te);
            let e = android_auto::AndroidAutoMessage::Input(i_event);

            let tx = aa_msg_tx_touch.lock().await;
            if let Some(tx) = tx.as_ref() {
                if let Err(err) = tx.send(e.sendable()).await {
                    warn!("[TOUCH] Session not active, touch dropped: {:?}", err);
                }
            } else {
                debug!("[TOUCH] No active session, touch dropped");
            }
        }
    });

    // ── Static headunit config (video, sensor, input) — channels created fresh per session ──
    let mut sensors = HashSet::new();
    sensors.insert(android_auto::Wifi::sensor_type::Enum::DRIVING_STATUS);
    sensors.insert(android_auto::Wifi::sensor_type::Enum::NIGHT_DATA);

    let base_config = VideoConfiguration {
        resolution: android_auto::Wifi::video_resolution::Enum::_720p,
        fps: android_auto::Wifi::video_fps::Enum::_30,
        dpi: 160,
    };
    let base_sensors = android_auto::SensorInformation { sensors };
    let base_input = android_auto::InputConfiguration {
        keycodes: vec![1, 2, 3, 4, 5],
        touchscreen: Some((1280, 720)),
    };

    // ── Android Auto Protocol Session (with per-session channel recreation) ──
    let aa_msg_tx_session = aa_msg_tx.clone();
    let video_tx_loop = video_tx.clone(); // clone for use inside the spawned task
    tokio::task::spawn(async move {
        info!("[AA] Building Android Auto configuration...");
        info!("[AA] Device: {} {} ({})", HU_MANUFACTURER, HU_MODEL, HU_CAR_YEAR);
        info!("[AA] SW Version: {} Build: {}", HU_SW_VERSION, HU_SW_BUILD);
        info!("[AA] Certificate: Using built-in JVC/crate cert (patched webpki for v1 support)");

        let config = android_auto::AndroidAutoConfiguration {
            unit: android_auto::HeadUnitInfo {
                name:              HU_NAME.to_string(),
                car_model:         HU_CAR_MODEL.to_string(),
                car_year:          HU_CAR_YEAR.to_string(),
                car_serial:        HU_CAR_SERIAL.to_string(),
                left_hand:         false,
                head_manufacturer: HU_MANUFACTURER.to_string(),
                head_model:        HU_MODEL.to_string(),
                sw_build:          HU_SW_BUILD.to_string(),
                sw_version:        HU_SW_VERSION.to_string(),
                native_media:      false,
                hide_clock:        Some(true),
            },
            custom_certificate: None,
        };

        let setup = android_auto::setup();

        loop {
            info!("[AA] ══ Waiting for USB phone connection... ══");

            // Create a FRESH channel for each session.
            // This ensures the touch task always has a valid sender for the current session.
            let (session_tx, session_rx) =
                mpsc::channel::<android_auto::SendableAndroidAutoMessage>(50);

            // Publish sender so touch task can use it
            *aa_msg_tx_session.lock().await = Some(session_tx.clone());

            let headunit = AppHeadunit {
                video_tx: video_tx_loop.clone(),
                aa_msg_tx: aa_msg_tx_session.clone(),
                android_recv: Arc::new(Mutex::new(Some(session_rx))),
                config: base_config.clone(),
                sensors: base_sensors.clone(),
                input_config: base_input.clone(),
            };

            let b = Box::new(headunit);
            let mut joinset = tokio::task::JoinSet::new();
            match b.run(config.clone(), &mut joinset, &setup).await {
                Ok(_) => info!("[AA] Session ended normally."),
                Err(e) => error!("[AA] Session error: {}", e),
            }
            joinset.join_all().await;

            // Invalidate sender so touch events are cleanly dropped between sessions
            *aa_msg_tx_session.lock().await = None;
            info!("[AA] Restarting session loop in 2 seconds...");
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    });

    // ── Web UI Server ──
    let state = Arc::new(AppState { video_tx, touch_tx });
    let app = Router::new()
        .nest_service("/", ServeDir::new("public"))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    info!("[WEB] Serving UI at http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("[WS] New WebSocket client connecting...");
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    info!("[WS] Client connected");
    let (mut sender, mut receiver) = socket.split();
    let mut video_rx = state.video_tx.subscribe();

    // Video send loop: broadcast raw H.264 NAL units to browser
    let mut send_task = tokio::spawn(async move {
        loop {
            match video_rx.recv().await {
                Ok(frame) => {
                    if sender.send(WsMessage::Binary(frame.to_vec())).await.is_err() {
                        info!("[WS] Client disconnected (send failed)");
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    warn!("[WS] Video receiver lagged by {} frames - browser too slow?", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    info!("[WS] Video channel closed");
                    break;
                }
            }
        }
    });

    // Touch receive loop: parse JSON touch events from browser
    let touch_tx = state.touch_tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                WsMessage::Text(text) => {
                    debug!("[WS] Received touch JSON: {}", text);
                    match serde_json::from_str::<TouchEvent>(&text) {
                        Ok(touch_event) => {
                            if touch_tx.send(touch_event).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("[WS] Failed to parse touch event JSON: {}", e);
                        }
                    }
                }
                WsMessage::Close(_) => {
                    info!("[WS] Client sent close frame");
                    break;
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
    info!("[WS] Client disconnected");
}
