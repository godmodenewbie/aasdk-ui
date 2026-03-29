// =================================================================
// 1. LOGIKA TAB UI MOBIL & SIDEBAR NAVIGATION
// =================================================================
document.addEventListener('DOMContentLoaded', () => {
    const btnHome = document.getElementById('btn-home');
    const btnAuto = document.getElementById('btn-auto');
    const btnLaunchAa = document.getElementById('btn-launch-aa');

    const homeDashboard = document.getElementById('home-dashboard');
    const aaDashboard = document.getElementById('aa-dashboard');

    if (!btnHome || !btnAuto || !homeDashboard || !aaDashboard) {
        console.error("[UI] Ada ID HTML yang hilang! Cek index.html kamu.");
        return;
    }

    function showHome() {
        console.log("[UI] Pindah ke Home");
        homeDashboard.classList.remove('hidden');
        aaDashboard.classList.add('hidden');

        btnHome.classList.replace('text-gray-500', 'text-[#81ecff]');
        btnHome.querySelector('span').style.fontVariationSettings = "'FILL' 1";

        btnAuto.classList.replace('text-[#81ecff]', 'text-gray-500');
        btnAuto.querySelector('span').style.fontVariationSettings = "'FILL' 0";
    }

    function showAuto() {
        console.log("[UI] Pindah ke Android Auto");
        homeDashboard.classList.add('hidden');
        aaDashboard.classList.remove('hidden');

        btnAuto.classList.replace('text-gray-500', 'text-[#81ecff]');
        btnAuto.querySelector('span').style.fontVariationSettings = "'FILL' 1";

        btnHome.classList.replace('text-[#81ecff]', 'text-gray-500');
        btnHome.querySelector('span').style.fontVariationSettings = "'FILL' 0";
    }

    btnHome.addEventListener('click', showHome);
    btnAuto.addEventListener('click', showAuto);
    if (btnLaunchAa) btnLaunchAa.addEventListener('click', showAuto);


    // =================================================================
    // 2. BROADWAY H.264 NAL DECODER SETUP
    // =================================================================
    const loader = document.getElementById("loader");

    // Resolusi wajib dari Rust (jangan diubah)
    const AA_WIDTH = 1280;
    const AA_HEIGHT = 720;

    const MSG_VIDEO = 0x01;
    const MSG_MEDIA = 0x02;
    const MSG_SYSTEM = 0x03;
    const MSG_SPEECH = 0x04;

    let player = null;
    let videoCanvas = null;

    try {
        player = new Player({
            useWorker: true,
            webgl: "auto",
            size: { width: AA_WIDTH, height: AA_HEIGHT }
        });

        const container = document.getElementById("container");
        container.innerHTML = "";
        container.appendChild(player.canvas);

        videoCanvas = player.canvas;
        videoCanvas.id = "videoCanvas";
        // Styling CSS tambahan untuk canvas
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
            console.log("[AUDIO] AudioContext created, sample rate:", audioCtx.sampleRate);
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

        console.log(`[WS] Connecting to ${wsUrl}...`);
        ws = new WebSocket(wsUrl);
        ws.binaryType = "arraybuffer";

        ws.onopen = () => {
            console.log("[WS] Connected to Android Auto bridge");
            loader.querySelector("p").innerText = "Connected — Waiting for video...";

// Unlock audio pada sentuhan