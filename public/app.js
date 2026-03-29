document.addEventListener("DOMContentLoaded", () => {
    const loader = document.getElementById("loader");

    // Must match Rust backend VideoConfiguration resolution
    const AA_WIDTH = 1280;
    const AA_HEIGHT = 720;

    // WebSocket message type prefix bytes (must match Rust constants)
    const MSG_VIDEO  = 0x01;
    const MSG_MEDIA  = 0x02;  // 48000 Hz, stereo, S16LE
    const MSG_SYSTEM = 0x03;  // 16000 Hz, mono,   S16LE
    const MSG_SPEECH = 0x04;  // 16000 Hz, mono,   S16LE

    // -----------------------------------------------------------------
    // 1. BROADWAY H.264 NAL DECODER SETUP
    // -----------------------------------------------------------------
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
        videoCanvas.style.width = "100%";
        videoCanvas.style.height = "100%";
        videoCanvas.style.display = "block";
        videoCanvas.style.objectFit = "contain";
        videoCanvas.style.touchAction = "none";

        console.log("[AA] Broadway.js player initialized OK");
    } catch (e) {
        console.error("[AA] Broadway.js Player init FAILED:", e);
    }

    // -----------------------------------------------------------------
    // 2. WEB AUDIO API SETUP (PCM Playback)
    // -----------------------------------------------------------------
    let audioCtx = null;
    let nextMediaTime = 0;   // next scheduled play time for media audio
    let nextSystemTime = 0;  // next scheduled play time for system/speech audio

    function getAudioCtx() {
        // AudioContext requires a user gesture to start — we create it lazily
        if (!audioCtx) {
            audioCtx = new (window.AudioContext || window.webkitAudioContext)();
            console.log("[AUDIO] AudioContext created, sample rate:", audioCtx.sampleRate);
        }
        // Resume if suspended (browser autoplay policy)
        if (audioCtx.state === "suspended") {
            audioCtx.resume();
        }
        return audioCtx;
    }

    /**
     * Play raw S16LE PCM data via Web Audio API with gapless scheduling.
     * @param {Uint8Array} rawBytes  - raw PCM bytes (S16LE, little-endian)
     * @param {number}     sampleRate - 48000 for media, 16000 for system/speech
     * @param {number}     numChannels - 2 for media (stereo), 1 for system (mono)
     * @param {Object}     timeRef    - object with { next: number } for scheduling
     */
    function playPCM(rawBytes, sampleRate, numChannels, timeRef) {
        const ctx = getAudioCtx();
        const numSamples = (rawBytes.byteLength / 2) / numChannels;
        if (numSamples < 1) return;

        const buffer = ctx.createBuffer(numChannels, numSamples, sampleRate);
        const view = new DataView(rawBytes.buffer, rawBytes.byteOffset, rawBytes.byteLength);

        // Convert interleaved S16LE → Float32 per channel
        for (let ch = 0; ch < numChannels; ch++) {
            const channelData = buffer.getChannelData(ch);
            for (let i = 0; i < numSamples; i++) {
                const byteOffset = (i * numChannels + ch) * 2;
                const sample = view.getInt16(byteOffset, /*littleEndian=*/true);
                channelData[i] = sample / 32768.0;
            }
        }

        const source = ctx.createBufferSource();
        source.buffer = buffer;
        source.connect(ctx.destination);

        // Schedule gaplessly: if behind current time, catch up with small buffer
        const now = ctx.currentTime;
        if (timeRef.next < now + 0.02) {
            timeRef.next = now + 0.05; // 50ms initial buffer to avoid underruns
        }
        source.start(timeRef.next);
        timeRef.next += buffer.duration;
    }

    const mediaTimeRef  = { next: 0 };
    const systemTimeRef = { next: 0 };

    // -----------------------------------------------------------------
    // 3. WEBSOCKET CONNECTION + AUTO-RECONNECT
    // -----------------------------------------------------------------
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
            // Unlock AudioContext on first user interaction after WS open
            document.addEventListener("pointerdown", getAudioCtx, { once: true });
        };

        ws.onmessage = (event) => {
            if (!(event.data instanceof ArrayBuffer)) return;
            const bytes = new Uint8Array(event.data);
            if (bytes.length < 2) return;

            const msgType = bytes[0];
            const payload = bytes.subarray(1);  // everything after the type byte

            switch (msgType) {
                case MSG_VIDEO:
                    // H.264 NAL unit → Broadway decoder
                    if (player) {
                        player.decode(payload);
                        if (!firstFrameReceived) {
                            firstFrameReceived = true;
                            loader.classList.add("hidden");
                            console.log(`[VIDEO] First frame! (${payload.length} bytes)`);
                        }
                    }
                    break;

                case MSG_MEDIA:
                    // 48000 Hz stereo PCM → Web Audio
                    playPCM(payload, 48000, 2, mediaTimeRef);
                    break;

                case MSG_SYSTEM:
                case MSG_SPEECH:
                    // 16000 Hz mono PCM → Web Audio (navigation prompts, voice assistant)
                    playPCM(payload, 16000, 1, systemTimeRef);
                    break;

                default:
                    console.warn(`[WS] Unknown message type: 0x${msgType.toString(16)}`);
            }
        };

        ws.onerror = (err) => console.error("[WS] Error:", err);

        ws.onclose = (evt) => {
            console.warn(`[WS] Disconnected (code=${evt.code}). Reconnecting in 3s...`);
            firstFrameReceived = false;
            loader.classList.remove("hidden");
            loader.querySelector("p").innerText = "Connection lost. Reconnecting...";
            clearTimeout(reconnectTimeout);
            reconnectTimeout = setTimeout(connect, 3000);
        };
    }

    connect();

    // -----------------------------------------------------------------
    // 4. TOUCH COORDINATE MAPPING (CSS → Android Auto 1280×720)
    // -----------------------------------------------------------------
    const ACTION_DOWN = 0;
    const ACTION_UP   = 1;
    const ACTION_MOVE = 2;

    const sendTouch = (action, clientX, clientY) => {
        if (!ws || ws.readyState !== WebSocket.OPEN) return;
        if (!videoCanvas) return;

        const rect = videoCanvas.getBoundingClientRect();
        const cssWidth  = rect.width;
        const cssHeight = rect.height;

        const videoRatio = AA_WIDTH / AA_HEIGHT;
        const cssRatio   = cssWidth / cssHeight;

        let displayedWidth  = cssWidth;
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
        const relY = clientY - rect.top  - offsetY;

        if (relX < 0 || relX > displayedWidth || relY < 0 || relY > displayedHeight) return;

        const mappedX = Math.max(0, Math.min(AA_WIDTH  - 1, Math.round((relX / displayedWidth)  * AA_WIDTH)));
        const mappedY = Math.max(0, Math.min(AA_HEIGHT - 1, Math.round((relY / displayedHeight) * AA_HEIGHT)));

        ws.send(JSON.stringify({ action, x: mappedX, y: mappedY }));
    };

    // -----------------------------------------------------------------
    // 5. POINTER EVENTS (mouse + touch unified)
    // -----------------------------------------------------------------
    document.addEventListener("pointerdown", (e) => {
        getAudioCtx(); // unlock audio on first interaction
        if (!videoCanvas) return;
        videoCanvas.setPointerCapture(e.pointerId);
        sendTouch(ACTION_DOWN, e.clientX, e.clientY);
    });

    document.addEventListener("pointermove", (e) => {
        if (e.buttons > 0) {
            sendTouch(ACTION_MOVE, e.clientX, e.clientY);
            e.preventDefault();
        }
    }, { passive: false });

    document.addEventListener("pointerup", (e) => {
        if (videoCanvas && videoCanvas.hasPointerCapture(e.pointerId)) {
            videoCanvas.releasePointerCapture(e.pointerId);
        }
        sendTouch(ACTION_UP, e.clientX, e.clientY);
    });

    document.addEventListener("contextmenu", (e) => e.preventDefault());
});
