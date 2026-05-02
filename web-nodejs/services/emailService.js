/**
 * BetterDesk Console — Email Notification Service
 *
 * SMTP-based email delivery for alert notifications.
 * Configuration is stored in the database (admin-configurable).
 *
 * @author shamstabraiz
 * @version 1.0.0
 */

'use strict';

let nodemailer;
try {
    nodemailer = require('nodemailer');
} catch (_) {
    // nodemailer is optional — alerts still work without email
    nodemailer = null;
}

const { getAdapter } = require('./dbAdapter');
const config = require('../config/config');

let _transporter = null;
let _cachedConfig = null;

/**
 * Load SMTP configuration from DB.
 * Falls back to environment variables.
 */
async function loadSmtpConfig() {
    const adapter = getAdapter();
    try {
        const cfg = await adapter.getSetting('smtp_config');
        if (cfg) {
            const parsed = typeof cfg === 'string' ? JSON.parse(cfg) : cfg;
            if (parsed.host) return parsed;
        }
    } catch (_) { /* ignore */ }

    // Env fallback
    if (process.env.SMTP_HOST) {
        return {
            host: process.env.SMTP_HOST,
            port: parseInt(process.env.SMTP_PORT || '587', 10),
            secure: process.env.SMTP_SECURE === 'true',
            user: process.env.SMTP_USER || '',
            pass: process.env.SMTP_PASS || '',
            from: process.env.SMTP_FROM || 'betterdesk@localhost',
        };
    }
    return null;
}

/**
 * Get or create the SMTP transporter.
 */
async function getTransporter() {
    if (!nodemailer) {
        console.warn('[Email] nodemailer not installed — email disabled');
        return null;
    }

    const config = await loadSmtpConfig();
    if (!config) return null;

    // Reuse cached transporter if config unchanged
    if (_transporter && _cachedConfig && JSON.stringify(_cachedConfig) === JSON.stringify(config)) {
        return _transporter;
    }

    _transporter = nodemailer.createTransport({
        host: config.host,
        port: config.port || 587,
        secure: config.secure || false,
        auth: (config.user && config.pass) ? { user: config.user, pass: config.pass } : undefined,
        tls: { rejectUnauthorized: config.smtpTlsVerify },
    });

    _cachedConfig = config;
    console.log(`[Email] SMTP transporter configured: ${config.host}:${config.port}`);
    return _transporter;
}

/**
 * Send an email notification.
 */
async function sendEmail({ to, subject, text, html }) {
    const transporter = await getTransporter();
    if (!transporter) {
        console.warn('[Email] Cannot send — no SMTP config');
        return false;
    }

    const config = _cachedConfig;
    try {
        await transporter.sendMail({
            from: config.from || 'betterdesk@localhost',
            to,
            subject,
            text,
            html,
        });
        console.log(`[Email] Sent to ${to}: ${subject}`);
        return true;
    } catch (err) {
        console.error(`[Email] Send failed: ${err.message}`);
        return false;
    }
}

/**
 * Send an alert notification email.
 */
async function sendAlertEmail(alert, rule) {
    const subject = `[BetterDesk Alert] ${rule.name} — ${alert.severity.toUpperCase()}`;
    const text = [
        `Alert: ${rule.name}`,
        `Severity: ${alert.severity}`,
        `Device: ${alert.device_id || 'N/A'}`,
        `Message: ${alert.message}`,
        `Triggered at: ${alert.triggered_at}`,
        '',
        `Rule: ${rule.description || rule.condition_type} ${rule.condition_op} ${rule.condition_value}`,
    ].join('\n');

    const html = `
        <div style="font-family: sans-serif; max-width: 600px;">
            <h2 style="color: #e74c3c;">BetterDesk Alert: ${escapeHtml(rule.name)}</h2>
            <table style="border-collapse: collapse; width: 100%;">
                <tr><td style="padding: 8px; border-bottom: 1px solid #eee; font-weight: bold;">Severity</td>
                    <td style="padding: 8px; border-bottom: 1px solid #eee;">${alert.severity}</td></tr>
                <tr><td style="padding: 8px; border-bottom: 1px solid #eee; font-weight: bold;">Device</td>
                    <td style="padding: 8px; border-bottom: 1px solid #eee;">${alert.device_id || 'N/A'}</td></tr>
                <tr><td style="padding: 8px; border-bottom: 1px solid #eee; font-weight: bold;">Message</td>
                    <td style="padding: 8px; border-bottom: 1px solid #eee;">${escapeHtml(alert.message)}</td></tr>
                <tr><td style="padding: 8px; font-weight: bold;">Time</td>
                    <td style="padding: 8px;">${alert.triggered_at}</td></tr>
            </table>
            <p style="color: #888; font-size: 12px; margin-top: 16px;">
                Rule: ${escapeHtml(rule.description || '')}
            </p>
        </div>`;

    const recipients = rule.notify_emails || '';
    if (!recipients) return false;

    return sendEmail({ to: recipients, subject, text, html });
}

function escapeHtml(str) {
    return String(str)
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;');
}

/**
 * Test SMTP connection.
 */
async function testConnection() {
    const transporter = await getTransporter();
    if (!transporter) {
        return { success: false, error: 'No SMTP configuration' };
    }
    try {
        await transporter.verify();
        return { success: true };
    } catch (err) {
        return { success: false, error: err.message };
    }
}

/**
 * Reset cached transporter (e.g. after config change).
 */
function resetTransporter() {
    _transporter = null;
    _cachedConfig = null;
}

module.exports = {
    sendEmail,
    sendAlertEmail,
    testConnection,
    resetTransporter,
    loadSmtpConfig,
};
