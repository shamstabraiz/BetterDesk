/**
 * Yomie Console — CDAP Video Stream Widget
 * Provides read-only video stream viewing from CDAP devices via WebSocket.
 * Renders frames on canvas from base64-encoded images.
 * Supports: quality reporting, codec negotiation, keyframe requests.
 */

(function () {
    'use strict';

    const activeSessions = {};

    const QUALITY_REPORT_INTERVAL = 5000;

    // ── Video Session Manager ────────────────────────────────────────────

    function openVideo(deviceId, widgetId) {
        const key = `${deviceId}:${widgetId}`;
        if (activeSessions[key]) return;

        const widgetEl = document.getElementById(`wval-${CSS.escape(widgetId)}`);
        if (!widgetEl) return;

        const canvas = widgetEl.querySelector('.cdap-video-canvas');
        const overlay = widgetEl.querySelector('.cdap-video-overlay');
        if (!canvas) return;

        const ctx = canvas.getContext('2d');

        if (overlay) {
            overlay.querySelector('span:last-child').textContent = 'Connecting...';
        }

        const proto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
        const wsUrl = `${proto}//${window.location.host}/api/cdap/devices/${encodeURIComponent(deviceId)}/video`;
        let ws;
        try {
            ws = new WebSocket(wsUrl, ['cdap-video']);
        } catch (err) {
            console.error('[CDAPVideo] WS creation failed:', err);
            return;
        }

        const session = {
            ws,
            canvas,
            ctx,
            overlay,
            widgetEl,
            widgetId,
            deviceId,
            sessionId: null,
            connected: false,
            _frameImg: new Image(),
            _frameCount: 0,
            _fpsTimer: null,
            _fps: 0,
            // Quality reporting
            _frameBytes: 0,
            _droppedFrames: 0,
            _lastFrameTime: 0,
            _qualityTimer: null,
            _qrPrevFrames: 0,
            _qrPrevBytes: 0,
            _qrPrevDropped: 0,
            _qrPrevTime: 0
        };
        activeSessions[key] = session;

        ws.onopen = () => {
            ws.send(JSON.stringify({
                stream_id: widgetId,
                quality: 60,
                fps: 10
            }));
        };

        ws.onmessage = (event) => {
            try {
                const msg = JSON.parse(event.data);
                handleMessage(session, msg);
            } catch (_) {}
        };

        ws.onerror = () => {
            console.error('[CDAPVideo] WS error for', deviceId);
        };

        ws.onclose = () => {
            if (activeSessions[key]) {
                setDisconnected(session);
                delete activeSessions[key];
            }
        };

        // FPS counter
        session._fpsTimer = setInterval(() => {
            session._fps = session._frameCount;
            session._frameCount = 0;
        }, 1000);
    }

    function handleMessage(session, msg) {
        switch (msg.type) {
            case 'ready':
                session.sessionId = msg.session_id;
                session.connected = true;
                if (session.overlay) {
                    session.overlay.classList.add('hidden');
                }
                startQualityReporting(session);
                sendCodecOffer(session);
                break;

            case 'frame':
                renderFrame(session, msg);
                break;

            case 'codec_answer':
                // Informational: device chose codecs
                break;

            case 'quality_adjust':
                // Server-initiated quality change
                break;

            case 'error':
                console.error('[CDAPVideo] Error:', msg.error);
                break;

            case 'end':
                setDisconnected(session);
                closeVideo(session.deviceId, session.widgetId);
                break;
        }
    }

    function renderFrame(session, msg) {
        if (!msg.data) return;

        const { canvas, ctx, _frameImg } = session;
        const format = msg.format || 'jpeg';

        session._frameCount++;
        session._frameBytes += msg.data.length * 0.75;
        session._lastFrameTime = Date.now();

        if (msg.width && msg.height) {
            if (canvas.width !== msg.width || canvas.height !== msg.height) {
                canvas.width = msg.width;
                canvas.height = msg.height;
            }
        }

        const src = `data:image/${format};base64,${msg.data}`;
        const img = _frameImg;
        img.onload = () => {
            ctx.drawImage(img, 0, 0, canvas.width, canvas.height);
        };
        img.onerror = () => {
            session._droppedFrames++;
        };
        img.src = src;
    }

    // ── Quality Reporting ────────────────────────────────────────────────

    function startQualityReporting(session) {
        if (session._qualityTimer) clearInterval(session._qualityTimer);
        session._qrPrevFrames = 0;
        session._qrPrevBytes = 0;
        session._qrPrevDropped = 0;
        session._qrPrevTime = Date.now();
        session._qualityTimer = setInterval(() => sendQualityReport(session), QUALITY_REPORT_INTERVAL);
    }

    function sendQualityReport(session) {
        if (!session.connected || !session.sessionId) return;
        const now = Date.now();
        const elapsed = (now - session._qrPrevTime) / 1000;
        if (elapsed <= 0) return;

        const totalFrames = session._frameCount + (session._fps > 0 ? 0 : 0);
        const frames = totalFrames - session._qrPrevFrames;
        const bytes = session._frameBytes - session._qrPrevBytes;
        const dropped = session._droppedFrames - session._qrPrevDropped;
        const fps = Math.round(frames / elapsed);
        const bandwidthKB = bytes / 1024 / elapsed;
        const frameLoss = frames > 0 ? dropped / (frames + dropped) : 0;
        const latencyMs = session._lastFrameTime > 0 ? Math.max(0, now - session._lastFrameTime) : 0;

        sendMsg(session, {
            type: 'quality_report',
            session_id: session.sessionId,
            bandwidth_kb: Math.round(bandwidthKB * 10) / 10,
            latency_ms: latencyMs,
            frame_loss: Math.round(frameLoss * 1000) / 1000,
            fps: fps
        });

        session._qrPrevFrames = totalFrames;
        session._qrPrevBytes = session._frameBytes;
        session._qrPrevDropped = session._droppedFrames;
        session._qrPrevTime = now;
    }

    // ── Codec Negotiation ────────────────────────────────────────────────

    function sendCodecOffer(session) {
        if (!session.sessionId) return;
        sendMsg(session, {
            type: 'codec_offer',
            session_id: session.sessionId,
            video: ['jpeg', 'png'],
            audio: [],
            preferred: 'jpeg'
        });
    }

    // ── Keyframe Request ─────────────────────────────────────────────────

    function requestKeyframe(deviceId, widgetId) {
        const key = `${deviceId}:${widgetId}`;
        const session = activeSessions[key];
        if (!session || !session.connected || !session.sessionId) return;
        sendMsg(session, {
            type: 'keyframe_request',
            session_id: session.sessionId
        });
    }

    function sendMsg(session, payload) {
        if (session.ws && session.ws.readyState === WebSocket.OPEN) {
            session.ws.send(JSON.stringify(payload));
        }
    }

    // ── Disconnect / Cleanup ─────────────────────────────────────────────

    function setDisconnected(session) {
        session.connected = false;
        if (session._fpsTimer) {
            clearInterval(session._fpsTimer);
            session._fpsTimer = null;
        }
        if (session._qualityTimer) {
            clearInterval(session._qualityTimer);
            session._qualityTimer = null;
        }
        if (session.overlay) {
            session.overlay.classList.remove('hidden');
            session.overlay.querySelector('span:last-child').textContent =
                window.Yomie?.t?.('cdap.disconnected') || 'Disconnected';
        }
        const connectDiv = session.widgetEl?.querySelector('.cdap-video-connect');
        if (connectDiv) connectDiv.classList.remove('hidden');
    }

    function closeVideo(deviceId, widgetId) {
        const key = `${deviceId}:${widgetId}`;
        const session = activeSessions[key];
        if (!session) return;

        if (session.ws && session.ws.readyState === WebSocket.OPEN) {
            session.ws.send(JSON.stringify({ type: 'close' }));
            session.ws.close();
        }
        setDisconnected(session);
        delete activeSessions[key];
    }

    // ── Public API ───────────────────────────────────────────────────────

    window.CDAPVideo = {
        open: openVideo,
        close: closeVideo,
        isActive: (deviceId, widgetId) => !!activeSessions[`${deviceId}:${widgetId}`],
        getFps: (deviceId, widgetId) => {
            const s = activeSessions[`${deviceId}:${widgetId}`];
            return s ? s._fps : 0;
        },
        requestKeyframe: requestKeyframe
    };

})();
