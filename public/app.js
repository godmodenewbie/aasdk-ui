document.addEventListener("DOMContentLoaded", () => {
    const loader = document.getElementById("loader");

    // Must match Rust backend VideoConfiguration resolution
    const AA_WIDTH = 1280;
    const AA_HEIGHT = 720;

    // -----------------------------------------------------------------
    // 1. BROADWAY H.264 NAL DECODER SETUP
    // -----------------------------------------------------------------
    let player = null;
    let videoCanvas = null;

    try {
        player = new Player({
            useWorker: true,
            webgl: "auto",   // use WebGL if available, fallback to 2D
            size: { width: AA_WIDTH, height: AA_HEIGHT }
        });

        // Broadway creates its own canvas — insert it into the container
        const container = document.getElementById("container");
        container.innerHTML = "";           // remove placeholder canvas
        container.appendChild(player.canvas);

        videoCanvas = player.canvas;
        videoCanvas.id = "videoCanvas";
        videoCanvas.style.width = "100%";
        videoCanvas.style.height = "100%";
        videoCanvas.style.display = "block";
        videoCanvas.style.objectFit = "contain";
        videoCanvas.style.touchAction = "none"; // prevent scroll/zoom on touch

        console.log("[AA] Broadway.js player initialized OK");
    } catch (e) {
        console.error("[AA] Broadway.js Player init FAILED:", e);
    }

    // -----------------------------------------------------------------
    // 2. WEBSOCKET CONNECTION + AUTO-RECONNECT
    // -----------------------------------------------------------------
    let ws = null;
    let firstFrameReceived = false;
    let reconnectTimeout = null;

    function connect() {
        const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
        const wsUrl = `${protocol}//${window.location.host}/ws`;

        console.log(`[WS] Connecting to ${wsUrl}...`);
        ws = new WebSocket(wsUrl);
        ws.binaryType = "arraybuffer"; // critical — receive raw bytes without base64 overhead

        ws.onopen = () => {
            console.log("[WS] Connected to Android Auto bridge");
            loader.querySelector("p").innerText = "Android Auto Connected — Waiting for video...";
        };

        ws.onmessage = (event) => {
            if (!(event.data instanceof ArrayBuffer)) return;

            const data = new Uint8Array(event.data);
            if (data.length === 0) return;

            if (player) {
                // Feed raw H.264 NAL unit directly to Broadway decoder
                player.decode(data);

                if (!firstFrameReceived) {
                    firstFrameReceived = true;
                    loader.classList.add("hidden");
                    console.log(`[VIDEO] First frame received! (${data.length} bytes)`);
                }
            } else {
                console.warn("[VIDEO] Frame received but player not ready");
            }
        };

        ws.onerror = (err) => {
            console.error("[WS] Error:", err);
        };

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
    // 3. TOUCH COORDINATE MAPPING (CSS → Android Auto 1920×1080)
    // -----------------------------------------------------------------
    const ACTION_DOWN = 0;
    const ACTION_UP = 1;
    const ACTION_MOVE = 2;

    const sendTouch = (action, clientX, clientY) => {
        if (!ws || ws.readyState !== WebSocket.OPEN) return;
        if (!videoCanvas) return;

        const rect = videoCanvas.getBoundingClientRect();
        const cssWidth = rect.width;
        const cssHeight = rect.height;

        // Handle letterbox/pillarbox offsets so clicks map to actual video area
        const videoRatio = AA_WIDTH / AA_HEIGHT;
        const cssRatio = cssWidth / cssHeight;

        let displayedWidth = cssWidth;
        let displayedHeight = cssHeight;
        let offsetX = 0;
        let offsetY = 0;

        if (cssRatio > videoRatio) {
            // Pillarbox (black bars left/right)
            displayedWidth = cssHeight * videoRatio;
            offsetX = (cssWidth - displayedWidth) / 2;
        } else {
            // Letterbox (black bars top/bottom)
            displayedHeight = cssWidth / videoRatio;
            offsetY = (cssHeight - displayedHeight) / 2;
        }

        const relX = clientX - rect.left - offsetX;
        const relY = clientY - rect.top - offsetY;

        // Ignore taps in black bar regions
        if (relX < 0 || relX > displayedWidth || relY < 0 || relY > displayedHeight) return;

        const mappedX = Math.max(0, Math.min(AA_WIDTH - 1, Math.round((relX / displayedWidth) * AA_WIDTH)));
        const mappedY = Math.max(0, Math.min(AA_HEIGHT - 1, Math.round((relY / displayedHeight) * AA_HEIGHT)));

        ws.send(JSON.stringify({ action, x: mappedX, y: mappedY }));
    };

    // -----------------------------------------------------------------
    // 4. POINTER EVENTS (mouse + touch unified API)
    // -----------------------------------------------------------------
    document.addEventListener("pointerdown", (e) => {
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
