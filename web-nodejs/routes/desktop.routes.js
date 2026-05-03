/**
 * BetterDesk Console - Desktop Widget Layout Routes
 * Persists per-user widget layouts and wallpaper selection server-side.
 */

const express = require('express');
const router = express.Router();
const db = require('../services/database');
const { requireAuth } = require('../middleware/auth');
const config = require('../config/config');
const path = require('path');
const fs = require('fs');

/* ── Key helpers ────────────────────────────────────────────── */

const LAYOUT_KEY_PREFIX = 'desktop_layout_';
const WALLPAPER_KEY_PREFIX = 'desktop_wallpaper_';

function layoutKey(userId) { return LAYOUT_KEY_PREFIX + userId; }
function wallpaperKey(userId) { return WALLPAPER_KEY_PREFIX + userId; }

function safeUserId(userId) {
    return String(userId || 'anonymous').replace(/[^a-zA-Z0-9_.-]/g, '_').slice(0, 80);
}

function fallbackLayoutPath(userId) {
    return path.join(config.dataDir, 'desktop-layouts', safeUserId(userId) + '.json');
}

function readFallbackLayout(userId) {
    const filePath = fallbackLayoutPath(userId);
    if (!fs.existsSync(filePath)) return null;
    try {
        return JSON.parse(fs.readFileSync(filePath, 'utf8'));
    } catch (_) {
        return null;
    }
}

function writeFallbackLayout(userId, widgets, wallpaper) {
    const filePath = fallbackLayoutPath(userId);
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    const payload = JSON.stringify({ widgets: widgets || [], wallpaper: wallpaper || '', updated_at: new Date().toISOString() });
    fs.writeFileSync(filePath, payload, { mode: 0o600 });
}

/* ── Maximum payload size (512 KB) ──────────────────────────── */
const MAX_LAYOUT_SIZE = 512 * 1024;

/* ── Validation ─────────────────────────────────────────────── */

function isValidWidgetArray(arr) {
    if (!Array.isArray(arr)) return false;
    if (arr.length > 100) return false; // max 100 widgets
    for (const w of arr) {
        if (!w || typeof w !== 'object') return false;
        if (typeof w.id !== 'string' || w.id.length > 40) return false;
        if (typeof w.type !== 'string' || w.type.length > 60) return false;
        if (typeof w.x !== 'number' || typeof w.y !== 'number') return false;
        if (typeof w.w !== 'number' || typeof w.h !== 'number') return false;
    }
    return true;
}

function isValidWallpaperPath(p) {
    if (p === null || p === '') return true;
    if (typeof p !== 'string') return false;
    if (p.length > 100) return false;
    // Allow /wallpapers/<number>.png or solid:<hex color>
    return /^\/wallpapers\/\d{1,4}\.png$/.test(p) || /^solid:#[0-9a-fA-F]{3,8}$/.test(p);
}

/* ── POST /layout — Save widget layout ──────────────────────── */

router.post('/layout', requireAuth, async (req, res) => {
    try {
        const userId = req.session.userId;
        if (!userId) {
            return res.status(400).json({ success: false, error: 'Missing user session' });
        }
        if (!req.body || typeof req.body !== 'object') {
            return res.status(400).json({ success: false, error: 'Invalid request body' });
        }
        const { widgets, wallpaper } = req.body;

        if (widgets !== undefined && !isValidWidgetArray(widgets)) {
            return res.status(400).json({ success: false, error: 'Invalid widget layout' });
        }

        const payload = JSON.stringify(widgets || []);
        if (payload.length > MAX_LAYOUT_SIZE) {
            return res.status(413).json({ success: false, error: 'Layout too large' });
        }

        if (wallpaper !== undefined && !isValidWallpaperPath(wallpaper)) {
            return res.status(400).json({ success: false, error: 'Invalid wallpaper path' });
        }

        try {
            await db.setSetting(layoutKey(userId), payload);

            if (wallpaper !== undefined) {
                await db.setSetting(wallpaperKey(userId), wallpaper);
            }
        } catch (dbErr) {
            console.warn('[Desktop] DB layout save failed, using file fallback:', dbErr.message);
            writeFallbackLayout(userId, widgets || [], wallpaper);
        }

        res.json({ success: true });
    } catch (err) {
        console.error('[Desktop] Save layout error:', err.message, err.stack);
        // Detect missing settings table (auth.db migration didn't run)
        if (err.message && err.message.includes('no such table')) {
            console.error('[Desktop] The settings table is missing from auth.db — run ensureAuthTables() or restart the server.');
        }
        res.status(500).json({ success: false, error: 'Failed to save layout' });
    }
});

/* ── GET /layout — Load widget layout ───────────────────────── */

router.get('/layout', requireAuth, async (req, res) => {
    try {
        const userId = req.session.userId;
        let layoutRaw = null;
        let wallpaper = '';

        try {
            layoutRaw = await db.getSetting(layoutKey(userId));
            wallpaper = await db.getSetting(wallpaperKey(userId));
        } catch (dbErr) {
            console.warn('[Desktop] DB layout load failed, using file fallback:', dbErr.message);
        }

        let widgets = [];
        if (layoutRaw) {
            try { widgets = JSON.parse(layoutRaw); } catch (_) { /* corrupt — return empty */ }
        }

        if (!layoutRaw) {
            const fallback = readFallbackLayout(userId);
            if (fallback) {
                widgets = Array.isArray(fallback.widgets) ? fallback.widgets : [];
                wallpaper = wallpaper || fallback.wallpaper || '';
            }
        }

        res.json({ success: true, data: { widgets, wallpaper: wallpaper || '' } });
    } catch (err) {
        console.error('[Desktop] Load layout error:', err.message);
        res.status(500).json({ success: false, error: 'Failed to load layout' });
    }
});

/* ── GET /wallpapers — List available wallpapers ────────────── */

router.get('/wallpapers', requireAuth, (req, res) => {
    try {
        const dir = path.join(__dirname, '..', 'wallpapers');
        if (!fs.existsSync(dir)) {
            return res.json({ success: true, data: [] });
        }
        const files = fs.readdirSync(dir)
            .filter(f => /^\d+\.png$/i.test(f))
            .sort((a, b) => parseInt(a) - parseInt(b))
            .map(f => '/wallpapers/' + f);

        res.json({ success: true, data: files });
    } catch (err) {
        console.error('[Desktop] List wallpapers error:', err.message);
        res.status(500).json({ success: false, error: 'Failed to list wallpapers' });
    }
});

module.exports = router;
