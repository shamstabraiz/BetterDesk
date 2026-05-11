/**
 * BetterDesk Console - Devices Routes Tests
 */

const request = require('supertest');
const { createTestApp } = require('./helpers');

// Mock dependencies
jest.mock('../services/database', () => ({
    logAction: jest.fn().mockResolvedValue(undefined),
    getPeerSysinfo: jest.fn().mockResolvedValue(null),
    getLatestPeerMetric: jest.fn().mockResolvedValue(null),
    getPeerMetrics: jest.fn().mockResolvedValue([]),
    getDeviceGroupsForPeer: jest.fn().mockResolvedValue([]),
    getAllFolders: jest.fn().mockResolvedValue([]),
    getAllFolderAssignments: jest.fn().mockResolvedValue({}),
    cleanupDeletedPeerData: jest.fn().mockResolvedValue(undefined)
}));

jest.mock('../services/serverBackend', () => ({
    getAllDevices: jest.fn().mockResolvedValue([]),
    getDeviceById: jest.fn().mockResolvedValue(null),
    updateDevice: jest.fn().mockResolvedValue(undefined),
    deleteDevice: jest.fn().mockResolvedValue(undefined)
}));

const serverBackend = require('../services/serverBackend');
const db = require('../services/database');
const devicesRoutes = require('../routes/devices.routes');

describe('Devices Routes', () => {
    let app;

    beforeEach(() => {
        app = createTestApp();
        // Inject auth for all requests
        app.use((req, _res, next) => {
            req.session.userId = 1;
            req.session.user = { id: 1, username: 'admin', role: 'admin' };
            next();
        });
        app.use('/', devicesRoutes);
        jest.clearAllMocks();
    });

    describe('GET /api/devices', () => {
        it('should return empty device list', async () => {
            serverBackend.getAllDevices.mockResolvedValue([]);

            const res = await request(app).get('/api/devices');

            expect(res.status).toBe(200);
            expect(res.body.success).toBe(true);
            expect(res.body.data.devices).toEqual([]);
            expect(res.body.data.total).toBe(0);
        });

        it('should return devices with default sort', async () => {
            const mockDevices = [
                { id: '123456789', hostname: 'PC-1', last_online: '2026-03-26T12:00:00Z' },
                { id: '987654321', hostname: 'PC-2', last_online: '2026-03-25T12:00:00Z' }
            ];
            serverBackend.getAllDevices.mockResolvedValue(mockDevices);

            const res = await request(app).get('/api/devices');

            expect(res.status).toBe(200);
            expect(res.body.data.devices).toHaveLength(2);
            expect(serverBackend.getAllDevices).toHaveBeenCalledWith(
                expect.objectContaining({
                    sortBy: 'last_online',
                    sortOrder: 'desc'
                })
            );
        });

        it('should sanitize sort parameters', async () => {
            serverBackend.getAllDevices.mockResolvedValue([]);

            await request(app).get('/api/devices?sortBy=DROP_TABLE&sortOrder=INJECT');

            expect(serverBackend.getAllDevices).toHaveBeenCalledWith(
                expect.objectContaining({
                    sortBy: 'last_online',
                    sortOrder: 'desc'
                })
            );
        });

        it('should accept valid sort parameters', async () => {
            serverBackend.getAllDevices.mockResolvedValue([]);

            await request(app).get('/api/devices?sortBy=hostname&sortOrder=asc');

            expect(serverBackend.getAllDevices).toHaveBeenCalledWith(
                expect.objectContaining({
                    sortBy: 'hostname',
                    sortOrder: 'asc'
                })
            );
        });

        it('should pass search filter', async () => {
            serverBackend.getAllDevices.mockResolvedValue([]);

            await request(app).get('/api/devices?search=test');

            expect(serverBackend.getAllDevices).toHaveBeenCalledWith(
                expect.objectContaining({
                    search: 'test'
                })
            );
        });
    });

    describe('GET /api/devices/:id', () => {
        it('should return 404 for unknown device', async () => {
            serverBackend.getDeviceById.mockResolvedValue(null);

            const res = await request(app).get('/api/devices/UNKNOWN');

            expect(res.status).toBe(404);
            expect(res.body.success).toBe(false);
        });

        it('should return device with sysinfo and metrics', async () => {
            serverBackend.getDeviceById.mockResolvedValue({
                id: '123456789',
                hostname: 'PC-1'
            });
            db.getPeerSysinfo.mockResolvedValue({
                hostname: 'PC-1',
                os: 'Windows 11',
                version: '10.0'
            });
            db.getLatestPeerMetric.mockResolvedValue({
                cpu_usage: 45.2,
                memory_usage: 67.8,
                disk_usage: 55.0,
                created_at: '2026-03-26T12:00:00Z'
            });

            const res = await request(app).get('/api/devices/123456789');

            expect(res.status).toBe(200);
            expect(res.body.success).toBe(true);
            expect(res.body.data.id).toBe('123456789');
            expect(res.body.data.sysinfo).toBeDefined();
            expect(res.body.data.sysinfo.os).toBe('Windows 11');
            expect(res.body.data.metrics).toBeDefined();
            expect(res.body.data.metrics.cpu_usage).toBe(45.2);
        });

        it('should return device even if sysinfo fails', async () => {
            serverBackend.getDeviceById.mockResolvedValue({
                id: '123456789',
                hostname: 'PC-1'
            });
            db.getPeerSysinfo.mockRejectedValue(new Error('table missing'));

            const res = await request(app).get('/api/devices/123456789');

            expect(res.status).toBe(200);
            expect(res.body.success).toBe(true);
            expect(res.body.data.id).toBe('123456789');
        });
    });

    describe('GET /api/tags', () => {
        it('should return unique device tags and folder names', async () => {
            serverBackend.getAllDevices.mockResolvedValue([
                { id: '123456789', tags: ['Internal', 'Windows'], folder_id: 1 },
                { id: '987654321', tags: 'External,Windows' }
            ]);
            db.getAllFolders.mockResolvedValue([{ id: 1, name: 'Servers' }]);
            db.getAllFolderAssignments.mockResolvedValue({ '123456789': 1 });

            const res = await request(app).get('/api/tags');

            expect(res.status).toBe(200);
            expect(res.body.success).toBe(true);
            expect(res.body.data.tags).toEqual(['External', 'Internal', 'Servers', 'Windows']);
        });
    });

    describe('PATCH /api/devices/:id', () => {
        it('should update display name and note', async () => {
            serverBackend.getDeviceById.mockResolvedValue({ id: '123456789', hostname: 'PC-1' });
            serverBackend.updateDevice.mockResolvedValue({ changes: 1 });

            const res = await request(app)
                .patch('/api/devices/123456789')
                .send({ display_name: 'Accounting PC', note: 'Front desk' });

            expect(res.status).toBe(200);
            expect(res.body.success).toBe(true);
            expect(res.body.data.changes).toBe(1);
            expect(serverBackend.updateDevice).toHaveBeenCalledWith('123456789', {
                user: undefined,
                note: 'Front desk',
                display_name: 'Accounting PC'
            });
        });

        it('should return backend update errors instead of reporting success', async () => {
            serverBackend.getDeviceById.mockResolvedValue({ id: '123456789', hostname: 'PC-1' });
            serverBackend.updateDevice.mockResolvedValue({ changes: 0, error: 'Go API update failed' });

            const res = await request(app)
                .patch('/api/devices/123456789')
                .send({ display_name: 'Accounting PC' });

            expect(res.status).toBe(502);
            expect(res.body.success).toBe(false);
            expect(res.body.error).toBe('Go API update failed');
        });

        it('should not fail the update when audit logging fails', async () => {
            const warnSpy = jest.spyOn(console, 'warn').mockImplementation(() => {});
            serverBackend.getDeviceById.mockResolvedValue({ id: '123456789', hostname: 'PC-1' });
            serverBackend.updateDevice.mockResolvedValue({ changes: 1 });
            db.logAction.mockRejectedValueOnce(new Error('audit unavailable'));

            const res = await request(app)
                .patch('/api/devices/123456789')
                .send({ display_name: 'Accounting PC' });

            expect(res.status).toBe(200);
            expect(res.body.success).toBe(true);
            expect(warnSpy).toHaveBeenCalledWith('Device update audit log failed:', 'audit unavailable');
            warnSpy.mockRestore();
        });
    });

    describe('GET /api/devices (unauthenticated)', () => {
        it('should return 401 without session', async () => {
            const unauthApp = createTestApp();
            unauthApp.use('/', devicesRoutes);

            const res = await request(unauthApp).get('/api/devices');

            expect(res.status).toBe(401);
        });
    });
});
