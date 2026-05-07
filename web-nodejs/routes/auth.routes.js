/**
 * BetterDesk Console - Auth Routes
 * Login, logout, session verification
 */

const express = require('express');
const router = express.Router();
const authService = require('../services/authService');
const db = require('../services/database');
const { guestOnly, requireAuth } = require('../middleware/auth');
const { loginLimiter, passwordChangeLimiter } = require('../middleware/rateLimiter');

/**
 * GET /login - Login page
 * Serves desktop-style login when user previously had desktop mode active
 * (detected via localStorage preference or explicit ?desktop=1 query param).
 */
router.get('/login', guestOnly, async (req, res) => {
    const useDesktop = req.query.desktop === '1' || req.cookies.betterdesk_desktop_mode === 'true';

    if (useDesktop) {
        // Fetch user list for multi-user selector (usernames + roles only, no secrets)
        let loginUsers = [];
        try {
            const users = typeof db.getAllUsersForBackup === 'function'
                ? await db.getAllUsersForBackup() : [];
            loginUsers = (Array.isArray(users) ? users : []).map(u => ({
                username: u.username,
                role: u.role || 'operator'
            }));
        } catch (_) { /* empty list is fine */ }

        return res.render('desktop-login', {
            title: req.t('nav.login'),
            activePage: 'login',
            loginUsers
        });
    }

    res.render('login', {
        title: req.t('nav.login'),
        activePage: 'login'
    });
});

/**
 * POST /api/auth/login - Login API
 */
router.post('/api/auth/login', loginLimiter, async (req, res) => {
    try {
        const { username, password } = req.body;
        
        if (!username || !password) {
            return res.status(400).json({
                success: false,
                error: req.t('auth.invalid_credentials')
            });
        }

        // Input length validation to prevent DoS via bcrypt
        if (username.length > 128 || password.length > 128) {
            return res.status(400).json({
                success: false,
                error: req.t('auth.invalid_credentials')
            });
        }
        
        const user = await authService.authenticate(username, password);
        
        if (!user) {
            // Record failed attempt (shared brute-force tracker with /api/bd/operator/login)
            authService.recordAttempt(username, req.ip, false);
            await db.logAction(null, 'login_failed', `Username: ${username}`, req.ip);
            
            return res.status(401).json({
                success: false,
                error: req.t('auth.invalid_credentials')
            });
        }
        
        // Clear brute-force lockout on successful auth (shared with operator_login)
        authService.recordAttempt(username, req.ip, true);
        
        // Block pro-only accounts from web panel login
        if (user.role === 'pro') {
            return res.status(403).json({
                success: false,
                error: req.t('auth.pro_only_account')
            });
        }
        
        // Check if TOTP verification is required
        if (user.totpRequired) {
            // Store pending 2FA session
            req.session.pendingTotpUserId = user.id;
            req.session.pendingTotpUser = user;
            
            return res.json({
                success: true,
                totpRequired: true
            });
        }
        
        // Regenerate session to prevent session fixation
        const oldSession = req.session;
        req.session.regenerate(async (err) => {
            if (err) {
                console.error('Session regeneration error:', err);
                return res.status(500).json({ success: false, error: 'Server error' });
            }
            
            // Restore session data
            req.session.userId = user.id;
            req.session.user = {
                id: user.id,
                username: user.username,
                role: user.role
            };
            
            // Log successful login
            await db.logAction(user.id, 'login', `User logged in`, req.ip);
            
            res.json({
                success: true,
                user: {
                    username: user.username,
                    role: user.role
                }
            });
        });
    } catch (err) {
        console.error('Login error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * POST /api/auth/logout - Logout API
 */
router.post('/api/auth/logout', async (req, res) => {
    const userId = req.session?.userId;
    
    if (userId) {
        await db.logAction(userId, 'logout', 'User logged out', req.ip);
    }
    
    req.session.destroy((err) => {
        if (err) {
            console.error('Session destroy error:', err);
        }
        res.clearCookie(req.sessionID ? req.session?.cookie?.name : 'betterdesk.sid');
        res.clearCookie('betterdesk.sid');
        res.clearCookie('bd.sid');
        res.json({ success: true });
    });
});

/**
 * GET /api/auth/verify - Verify session is valid
 */
router.get('/api/auth/verify', requireAuth, (req, res) => {
    res.json({
        success: true,
        user: req.session.user
    });
});

/**
 * POST /api/auth/password - Change password
 */
router.post('/api/auth/password', requireAuth, passwordChangeLimiter, async (req, res) => {
    try {
        const { currentPassword, newPassword, confirmPassword } = req.body;
        
        if (!currentPassword || !newPassword) {
            return res.status(400).json({
                success: false,
                error: req.t('auth.password_required')
            });
        }
        
        if (newPassword !== confirmPassword) {
            return res.status(400).json({
                success: false,
                error: req.t('auth.passwords_mismatch')
            });
        }
        
        const result = await authService.changePassword(
            req.session.userId,
            currentPassword,
            newPassword
        );
        
        if (!result.success) {
            return res.status(400).json({
                success: false,
                error: result.error
            });
        }
        
        // Log password change
        await db.logAction(req.session.userId, 'password_changed', 'Password changed', req.ip);
        
        res.json({ success: true });
    } catch (err) {
        console.error('Password change error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * GET /logout - Logout (redirect)
 */
router.get('/logout', (req, res) => {
    req.session.destroy(() => {
        res.clearCookie('betterdesk.sid');
        res.clearCookie('bd.sid');
        res.redirect('/login');
    });
});

// ==================== TOTP (2FA) Routes ====================

/**
 * POST /api/auth/totp/verify - Verify TOTP code during login
 */
router.post('/api/auth/totp/verify', loginLimiter, async (req, res) => {
    try {
        const { code, recoveryCode } = req.body;
        const pendingUserId = req.session.pendingTotpUserId;
        const pendingUser = req.session.pendingTotpUser;
        
        if (!pendingUserId || !pendingUser) {
            return res.status(400).json({
                success: false,
                error: req.t('auth.totp_session_expired')
            });
        }
        
        let verified = false;
        let method = 'totp';
        
        if (recoveryCode) {
            // Try recovery code
            verified = await authService.verifyRecoveryCode(pendingUserId, recoveryCode);
            method = 'recovery';
        } else if (code) {
            // Try TOTP code
            verified = await authService.verifyTotpCode(pendingUserId, code);
        }
        
        if (!verified) {
            await db.logAction(pendingUserId, 'totp_failed', `Method: ${method}`, req.ip);
            return res.status(401).json({
                success: false,
                error: req.t('auth.totp_invalid_code')
            });
        }

        // Regenerate session to prevent session fixation across the 2FA boundary.
        // Mirrors the standard login flow: every successful authentication step
        // results in a fresh session ID before any privileged data is written.
        req.session.regenerate(async (regenErr) => {
            if (regenErr) {
                console.error('TOTP session regeneration error:', regenErr);
                return res.status(500).json({
                    success: false,
                    error: req.t('errors.server_error')
                });
            }

            // Pending fields belonged to the old session; the regenerated session
            // starts empty, so we only need to populate the authenticated user.
            req.session.userId = pendingUser.id;
            req.session.user = {
                id: pendingUser.id,
                username: pendingUser.username,
                role: pendingUser.role
            };

            try {
                await db.updateLastLogin(pendingUser.id);
                await db.logAction(pendingUser.id, 'login', `User logged in (2FA: ${method})`, req.ip);
            } catch (logErr) {
                console.error('TOTP post-auth bookkeeping error:', logErr);
            }

            res.json({
                success: true,
                user: {
                    username: pendingUser.username,
                    role: pendingUser.role
                }
            });
        });
    } catch (err) {
        console.error('TOTP verify error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * POST /api/auth/totp/setup - Generate TOTP setup (QR code + secret)
 */
router.post('/api/auth/totp/setup', requireAuth, async (req, res) => {
    try {
        // Check if already enabled
        if (await authService.isTotpEnabled(req.session.userId)) {
            return res.status(400).json({
                success: false,
                error: req.t('auth.totp_already_enabled')
            });
        }
        
        const result = await authService.generateTotpSetup(req.session.userId);
        
        if (!result.success) {
            return res.status(400).json({
                success: false,
                error: result.error
            });
        }
        
        res.json({
            success: true,
            qrCode: result.qrCode,
            secret: result.secret,
            otpauthUrl: result.otpauthUrl
        });
    } catch (err) {
        console.error('TOTP setup error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * POST /api/auth/totp/enable - Verify code and enable TOTP
 */
router.post('/api/auth/totp/enable', requireAuth, async (req, res) => {
    try {
        const { code } = req.body;
        
        if (!code || code.length !== 6) {
            return res.status(400).json({
                success: false,
                error: req.t('auth.totp_invalid_code')
            });
        }
        
        const result = await authService.verifyAndEnableTotp(req.session.userId, code);
        
        if (!result.success) {
            return res.status(400).json({
                success: false,
                error: result.error
            });
        }
        
        // Log action
        await db.logAction(req.session.userId, 'totp_enabled', '2FA enabled', req.ip);
        
        res.json({
            success: true,
            recoveryCodes: result.recoveryCodes
        });
    } catch (err) {
        console.error('TOTP enable error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * POST /api/auth/totp/disable - Disable TOTP
 */
router.post('/api/auth/totp/disable', requireAuth, async (req, res) => {
    try {
        const { password } = req.body;
        
        if (!password) {
            return res.status(400).json({
                success: false,
                error: req.t('auth.password_required')
            });
        }
        
        // Verify password before disabling (supports both bcrypt and PBKDF2 hashes)
        const user = await db.getUserById(req.session.userId);
        if (!user) {
            return res.status(404).json({
                success: false,
                error: req.t('users.not_found')
            });
        }
        
        const valid = await authService.verifyPassword(password, user.password_hash);
        if (!valid) {
            return res.status(401).json({
                success: false,
                error: req.t('auth.invalid_credentials')
            });
        }
        
        await authService.disableTotp(req.session.userId);
        
        // Log action
        await db.logAction(req.session.userId, 'totp_disabled', '2FA disabled', req.ip);
        
        res.json({ success: true });
    } catch (err) {
        console.error('TOTP disable error:', err);
        res.status(500).json({
            success: false,
            error: req.t('errors.server_error')
        });
    }
});

/**
 * GET /api/auth/totp/status - Check if TOTP is enabled for current user
 */
router.get('/api/auth/totp/status', requireAuth, async (req, res) => {
    try {
        const enabled = await authService.isTotpEnabled(req.session.userId);
        res.json({ success: true, enabled });
    } catch (err) {
        console.error('TOTP status error:', err);
        res.status(500).json({ success: false, error: req.t('errors.server_error') });
    }
});

module.exports = router;
