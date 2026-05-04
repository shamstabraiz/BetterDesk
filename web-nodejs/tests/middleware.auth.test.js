/**
 * Yomie Console - Auth Middleware Tests
 */

const request = require('supertest');
const { createTestApp } = require('./helpers');

const { requireAuth, requireRole, guestOnly } = require('../middleware/auth');

describe('Auth Middleware', () => {
    describe('requireAuth', () => {
        it('should return 401 for unauthenticated API requests', async () => {
            const app = createTestApp();
            app.get('/api/test', requireAuth, (_req, res) => {
                res.json({ success: true });
            });

            const res = await request(app).get('/api/test');

            expect(res.status).toBe(401);
            expect(res.body.success).toBe(false);
        });

        it('should redirect to login for unauthenticated HTML requests', async () => {
            const app = createTestApp();
            app.get('/dashboard', requireAuth, (_req, res) => {
                res.send('OK');
            });

            const res = await request(app).get('/dashboard');

            expect(res.status).toBe(302);
            expect(res.headers.location).toBe('/login');
        });

        it('should pass through for authenticated requests', async () => {
            const app = createTestApp();
            app.use((req, _res, next) => {
                req.session.userId = 1;
                req.session.user = { id: 1, username: 'admin', role: 'admin' };
                next();
            });
            app.get('/api/test', requireAuth, (_req, res) => {
                res.json({ success: true });
            });

            const res = await request(app).get('/api/test');

            expect(res.status).toBe(200);
            expect(res.body.success).toBe(true);
        });
    });

    describe('requireRole', () => {
        it('should allow admin access to admin-only routes', async () => {
            const app = createTestApp();
            app.use((req, _res, next) => {
                req.session.userId = 1;
                req.session.user = { id: 1, username: 'admin', role: 'admin' };
                next();
            });
            app.get('/api/admin', requireRole('admin'), (_req, res) => {
                res.json({ success: true });
            });

            const res = await request(app).get('/api/admin');

            expect(res.status).toBe(200);
        });

        it('should deny operator access to admin-only routes', async () => {
            const app = createTestApp();
            app.use((req, _res, next) => {
                req.session.userId = 2;
                req.session.user = { id: 2, username: 'operator1', role: 'operator' };
                next();
            });
            app.get('/api/admin', requireRole('admin'), (_req, res) => {
                res.json({ success: true });
            });

            const res = await request(app).get('/api/admin');

            expect(res.status).toBe(403);
        });

        it('should allow admin to access operator routes', async () => {
            const app = createTestApp();
            app.use((req, _res, next) => {
                req.session.userId = 1;
                req.session.user = { id: 1, username: 'admin', role: 'admin' };
                next();
            });
            app.get('/api/op', requireRole('operator'), (_req, res) => {
                res.json({ success: true });
            });

            const res = await request(app).get('/api/op');

            expect(res.status).toBe(200);
        });

        it('should return 401 for unauthenticated API requests', async () => {
            const app = createTestApp();
            app.get('/api/admin', requireRole('admin'), (_req, res) => {
                res.json({ success: true });
            });

            const res = await request(app).get('/api/admin');

            expect(res.status).toBe(401);
        });
    });

    describe('guestOnly', () => {
        it('should allow unauthenticated users', async () => {
            const app = createTestApp();
            app.get('/login', guestOnly, (_req, res) => {
                res.send('login page');
            });

            const res = await request(app).get('/login');

            expect(res.status).toBe(200);
        });

        it('should redirect authenticated users to dashboard', async () => {
            const app = createTestApp();
            app.use((req, _res, next) => {
                req.session.userId = 1;
                req.session.user = { id: 1, username: 'admin', role: 'admin' };
                next();
            });
            app.get('/login', guestOnly, (_req, res) => {
                res.send('login page');
            });

            const res = await request(app).get('/login');

            expect(res.status).toBe(302);
            expect(res.headers.location).toBe('/');
        });
    });
});
