/**
 * Yomie Console - Rate Limiter Middleware
 */

const rateLimit = require('express-rate-limit');
const config = require('../config/config');

/**
 * General API rate limiter
 */
const apiLimiter = rateLimit({
    windowMs: config.rateLimitWindowMs,
    max: config.rateLimitMax,
    standardHeaders: true,
    legacyHeaders: false,
    message: {
        success: false,
        error: 'Too many requests. Please try again later.'
    },
    keyGenerator: (req) => {
        return req.ip || req.headers['x-forwarded-for'] || 'unknown';
    },
    skip: (req) => {
        // Skip rate limiting for widget data endpoints from same-origin panel requests
        // These are internal dashboard/widget refresh calls, not external abuse
        const widgetPaths = ['/api/stats', '/api/server/status', '/api/devices', '/api/audit/conn'];
        if (widgetPaths.some(p => req.path.startsWith(p)) && req.headers.referer && req.session && req.session.userId) {
            return true;
        }
        return false;
    }
});

/**
 * Strict rate limiter for login attempts
 */
const loginLimiter = rateLimit({
    windowMs: config.rateLimitWindowMs,
    max: config.loginRateLimitMax,
    standardHeaders: true,
    legacyHeaders: false,
    message: {
        success: false,
        error: 'Too many login attempts. Please try again in a minute.'
    },
    keyGenerator: (req) => {
        return req.ip || req.headers['x-forwarded-for'] || 'unknown';
    }
});

/**
 * Very strict limiter for password changes
 */
const passwordChangeLimiter = rateLimit({
    windowMs: 5 * 60 * 1000, // 5 minutes
    max: 3,
    standardHeaders: true,
    legacyHeaders: false,
    message: {
        success: false,
        error: 'Too many password change attempts. Please try again later.'
    }
});

module.exports = {
    apiLimiter,
    loginLimiter,
    passwordChangeLimiter
};
