// =================================================================
// 1. LOGIKA TAB UI MOBIL & SIDEBAR NAVIGATION
// (Tanpa DOMContentLoaded agar langsung dieksekusi browser!)
// =================================================================
const btnHome = document.getElementById('btn-home');
const btnAuto = document.getElementById('btn-auto');
const btnLaunchAa = document.getElementById('btn-launch-aa');

const homeDashboard = document.getElementById('home-dashboard');
const aaDashboard = document.getElementById('aa-dashboard');
const loader = document.getElementById('loader');

if (!btnHome || !btnAuto || !homeDashboard || !aaDashboard) {
    console.error("[UI] Ada ID HTML yang hilang! Cek index.html kamu.");
}

function showHome() {
    homeDashboard.classList.remove('hidden');
    aaDashboard.classList.add('hidden');

    btnHome.classList.replace('text-gray-500', 'text-[#81ecff]');
    if (btnHome.querySelector('span')) btnHome.querySelector('span').style.fontVariationSettings = "'FILL' 1";

    btnAuto.classList.replace('text-[#81ecff]', 'text-gray-500');
    if (btnAuto.querySelector('span')) btnAuto.querySelector('span').style.fontVariationSettings = "'FILL' 0";
}

function showAuto() {
    homeDashboard.classList.add('hidden');
    aaDashboard.classList.remove('hidden');

    btnAuto.classList.replace('text-gray-500', 'text-[#81ecff]');
    if (btnAuto.querySelector('span')) btnAuto.querySelector('span').style.fontVariationSettings = "'FILL' 1";

    btnHome.classList.replace('text-[#81ecff]', 'text-gray-500');
    if (btnHome.querySelector('span')) btnHome.querySelector('span').style.fontVariationSettings = "'FILL' 0";
}

if (btnHome) btnHome.addEventListener('click', showHome);
if (btnAuto) btnAuto.addEventListener('click', showAuto);
if (btnLaunchAa) btnLaunchAa.addEventListener('click', showAuto);


// =================================================================
// 2. BROADWAY H.264 NAL DECODER SETUP
// =================================================================
const AA_WIDTH = 1280;
const AA_HEIGHT = 720;

const MSG_VIDEO = 0x01;
const MSG_MEDIA = 0x02;
const MSG_SYSTEM = 0x03;
const MSG_SPEECH = 0x04;

let player = null;
let videoCanvas = null;
const container = document.getElementById("container");

try {
    player = new Player({
        useWorker: true,
        webgl: "auto",
        size: { width: AA_WIDTH, height: AA_HEIGHT }
    });

    container.innerHTML = "";
    container.appendChild(player.canvas);

    videoCanvas = player.canvas;
    videoCanvas.id = "videoCanvas";
    videoCanvas.style.width = "100%";
    videoCanvas.style.height = "100%";
    videoCanvas.style.display = "block";
    videoCanvas.style.objectFit = "contain";
    videoCanvas.style.touchAction = "none";

    console.log("[AA] Broadway.js player initialized OK");
} catch (e) {
    console.error("[AA] Broadway.js Player init FAILED:", e);
}


// =================================================================
// 3. WEB AUDIO API SETUP (PCM Playback)
// =================================================================
let audioCtx = null;
let nextMediaTime = 0;
let nextSystemTime = 0;

function getAudioCtx() {
    if (!audioCtx) {
        audioCtx = new (window.AudioContext || window.webkitAudioContext)();
    }
    if (audioCtx.state === "suspended") {
        audioCtx.resume();
    }
    return audioCtx;
}

function playPCM(rawBytes, sampleRate, numChannels, timeRef) {
    const ctx = getAudioCtx();
    const numSamples = (rawBytes.byteLength / 2) / numChannels;
    if (numSamples < 1) return;

    const buffer = ctx.createBuffer(numChannels, numSamples, sampleRate);
    const view = new DataView(rawBytes.buffer, rawBytes.byteOffset, rawBytes.byteLength);

    for (let ch = 0; ch < numChannels; ch++) {
        const channelData = buffer.getChannelData(ch);
        for (let i = 0; i < numSamples; i++) {
            const byteOffset = (i * numChannels + ch) * 2;
            const sample = view.getInt16(byteOffset, true);
            channelData[i] = sample / 32768.0;
        }
    }

    const source = ctx.createBufferSource();
    source.buffer = buffer;
    source.connect(ctx.destination);

    const now = ctx.currentTime;
    if (timeRef.next < now + 0.02) {
        timeRef.next = now + 0.05;
    }
    source.start(timeRef.next);
    timeRef.next += buffer.duration;
}

const mediaTimeRef = { next: 0 };
const systemTimeRef = { next: 0 };


// =================================================================
// 4. WEBSOCKET CONNECTION
// =================================================================
let ws = null;
let firstFrameReceived = false;
let reconnectTimeout = null;

function connect() {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    const wsUrl = `${protocol}//${window.location.host}/ws`;

    ws = new WebSocket(wsUrl);
    ws.binaryType = "arraybuffer";

    ws.onopen = () => {
        if (loader && loader.querySelector("p")) loader.querySelector("p").innerText = "Connected — Waiting for video...";
    };

    ws.onmessage = (event) => {
        if (!(event.data instanceof ArrayBuffer)) return;
        const bytes = new Uint8Array(event.data);
        if (bytes.length < 2) return;

        const msgType = bytes[0];
        const payload = bytes.subarray(1);

        switch (msgType) {
            case MSG_VIDEO:
                if (player) {
                    player.decode(payload);
                    if (!firstFrameReceived) {
                        firstFrameReceived = true;
                        if (loader) loader.classList.add("hidden");
                    }
                }
                break;
            case MSG_MEDIA:
                playPCM(payload, 48000, 2, mediaTimeRef);
                break;
            case MSG_SYSTEM:
            case MSG_SPEECH:
                playPCM(payload, 16000, 1, systemTimeRef);
                break;
        }
    };

    ws.onclose = (evt) => {
        firstFrameReceived = false;
        if (loader) {
            loader.classList.remove("hidden");
            if (loader.querySelector("p")) loader.querySelector("p").innerText = "Connection lost. Reconnecting...";
        }
        clearTimeout(reconnectTimeout);
        reconnectTimeout = setTimeout(connect, 3000);
    };
}

connect();


// =================================================================
// 5. TOUCH COORDINATE MAPPING (DIJAMIN PRESISI & AMAN)
// =================================================================
const ACTION_DOWN = 0;
const ACTION_UP = 1;
const ACTION_MOVE = 2;

const sendTouch = (action, clientX, clientY) => {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    if (!videoCanvas) return;

    const rect = videoCanvas.getBoundingClientRect();
    const cssWidth = rect.width;
    const cssHeight = rect.height;

    const videoRatio = AA_WIDTH / AA_HEIGHT;
    const cssRatio = cssWidth / cssHeight;

    let displayedWidth = cssWidth;
    let displayedHeight = cssHeight;
    let offsetX = 0;
    let offsetY = 0;

    if (cssRatio > videoRatio) {
        displayedWidth = cssHeight * videoRatio;
        offsetX = (cssWidth - displayedWidth) / 2;
    } else {
        displayedHeight = cssWidth / videoRatio;
        offsetY = (cssHeight - displayedHeight) / 2;
    }

    const relX = clientX - rect.left - offsetX;
    const relY = clientY - rect.top - offsetY;

    if (relX < 0 || relX > displayedWidth || relY < 0 || relY > displayedHeight) return;

    const mappedX = Math.max(0, Math.min(AA_WIDTH - 1, Math.round((relX / displayedWidth) * AA_WIDTH)));
    const mappedY = Math.max(0, Math.min(AA_HEIGHT - 1, Math.round((relY / displayedHeight) * AA_HEIGHT)));

    ws.send(JSON.stringify({ action: action, x: mappedX, y: mappedY }));
};

// PERBAIKAN: Pasang event listener di CONTAINER (Wadah Canvas)
// Menggunakan event MOUSE dan TOUCH terpisah agar dijamin terbaca oleh Chromium!
if (container) {
    // --- MOUSE EVENTS (Untuk Testing di Laptop) ---
    container.addEventListener("mousedown", (e) => {
        getAudioCtx();
        sendTouch(ACTION_DOWN, e.clientX, e.clientY);
    });

    container.addEventListener("mousemove", (e) => {
        if (e.buttons > 0) sendTouch(ACTION_MOVE, e.clientX, e.clientY);
    });

    container.addEventListener("mouseup", (e) => {
        sendTouch(ACTION_UP, e.clientX, e.clientY);
    });

    // --- TOUCH EVENTS (Untuk Head Unit Layar Sentuh Beneran) ---
    container.addEventListener("touchstart", (e) => {
        getAudioCtx();
        sendTouch(ACTION_DOWN, e.touches[0].clientX, e.touches[0].clientY);
        e.preventDefault(); // Mencegah layar ter-scroll saat jari menyentuh
    }, { passive: false });

    container.addEventListener("touchmove", (e) => {
        sendTouch(ACTION_MOVE, e.touches[0].clientX, e.touches[0].clientY);
        e.preventDefault();
    }, { passive: false });

    container.addEventListener("touchend", (e) => {
        // touchend menggunakan changedTouches karena jarinya sudah diangkat
        sendTouch(ACTION_UP, e.changedTouches[0].clientX, e.changedTouches[0].clientY);
    });

    container.addEventListener("contextmenu", (e) => e.preventDefault());
}