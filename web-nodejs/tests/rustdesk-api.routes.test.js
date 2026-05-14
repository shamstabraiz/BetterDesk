/**
 * BetterDesk Console - RustDesk Client API route tests
 */

const request = require('supertest');
const { createTestApp } = require('./helpers');

jest.mock('../services/database', () => ({
    getAllDevices: jest.fn(),
    getAllFolderAssignments: jest.fn(),
    getAllFolders: jest.fn(),
    getAllPeerSysinfo: jest.fn(),
    getAddressBook: jest.fn()
}));

jest.mock('../services/authService', () => ({
    validateAccessToken: jest.fn()
}));

jest.mock('../services/serverBackend', () => ({
    getAllDevices: jest.fn(),
    setPeerTags: jest.fn()
}));

const db = require('../services/database');
const authService = require('../services/authService');
const serverBackend = require('../services/serverBackend');
const rustdeskApiRoutes = require('../routes/rustdesk-api.routes');

describe('RustDesk Client API routes', () => {
    let app;

    beforeEach(() => {
        app = createTestApp();
        app.use('/', rustdeskApiRoutes);

        jest.clearAllMocks();
        authService.validateAccessToken.mockResolvedValue({ id: 2, username: 'viewer1', role: 'viewer' });
        db.getAllDevices.mockResolvedValue([
            { id: 'OWNED1', hostname: 'Owned', online: true, tags: 'Allowed' },
            { id: 'OTHER1', hostname: 'Other', online: true, tags: 'Private' }
        ]);
        serverBackend.getAllDevices.mockResolvedValue([
            { id: 'OWNED1', hostname: 'Owned', online: true, tags: 'Allowed' },
            { id: 'OTHER1', hostname: 'Other', online: true, tags: 'Private' }
        ]);
        db.getAllFolderAssignments.mockResolvedValue({});
        db.getAllFolders.mockResolvedValue([]);
        db.getAllPeerSysinfo.mockResolvedValue([]);
        db.getAddressBook.mockImplementation(async (_userId, abType) => {
            if (abType === 'legacy') {
                return { data: JSON.stringify({ peers: [{ id: 'OWNED1' }] }) };
            }
            return null;
        });
    });

    describe('GET /api/peers', () => {
        it('limits viewer inventory to peers in the authenticated user address book', async () => {
            const res = await request(app)
                .get('/api/peers?include_offline=true')
                .set('Authorization', 'Bearer viewer-token');

            expect(res.status).toBe(200);
            expect(res.body.total).toBe(1);
            expect(res.body.data.map(peer => peer.id)).toEqual(['OWNED1']);
            expect(db.getAddressBook).toHaveBeenCalledWith(2, 'legacy');
            expect(db.getAddressBook).toHaveBeenCalledWith(2, 'personal');
        });

        it('returns an empty list for viewer users without address book peers', async () => {
            db.getAddressBook.mockResolvedValue(null);

            const res = await request(app)
                .get('/api/peers?include_offline=true')
                .set('Authorization', 'Bearer viewer-token');

            expect(res.status).toBe(200);
            expect(res.body.total).toBe(0);
            expect(res.body.data).toEqual([]);
        });

        it('keeps editable operator inventory synchronization unchanged', async () => {
            authService.validateAccessToken.mockResolvedValue({ id: 3, username: 'operator1', role: 'operator' });

            const res = await request(app)
                .get('/api/peers?include_offline=true')
                .set('Authorization', 'Bearer operator-token');

            expect(res.status).toBe(200);
            expect(res.body.total).toBe(2);
            expect(res.body.data.map(peer => peer.id)).toEqual(['OWNED1', 'OTHER1']);
            expect(db.getAddressBook).not.toHaveBeenCalled();
        });
    });

    describe('GET /api/ab', () => {
        it('does not auto-add console inventory to editable user address books', async () => {
            authService.validateAccessToken.mockResolvedValue({ id: 3, username: 'operator1', role: 'operator' });
            db.getAddressBook.mockImplementation(async (_userId, abType) => {
                if (abType === 'legacy') {
                    return {
                        data: JSON.stringify({
                            peers: [{ id: 'OWNED1', tags: ['Client'] }],
                            tags: ['Client']
                        })
                    };
                }
                return null;
            });

            const res = await request(app)
                .get('/api/ab')
                .set('Authorization', 'Bearer operator-token');

            expect(res.status).toBe(200);
            const data = JSON.parse(res.body.data);
            expect(data.peers.map(peer => peer.id)).toEqual(['OWNED1']);
            expect(data.peers[0].tags).toEqual(['Client', 'Allowed']);
            expect(data.tags).toEqual(['Client', 'Allowed']);
            expect(serverBackend.getAllDevices).toHaveBeenCalled();
        });

        it('keeps an empty address book empty even when console devices exist', async () => {
            authService.validateAccessToken.mockResolvedValue({ id: 1, username: 'admin', role: 'admin' });
            db.getAddressBook.mockResolvedValue({ data: JSON.stringify({ peers: [], tags: [] }) });

            const res = await request(app)
                .get('/api/ab')
                .set('Authorization', 'Bearer admin-token');

            expect(res.status).toBe(200);
            const data = JSON.parse(res.body.data);
            expect(data.peers).toEqual([]);
            expect(data.tags).toEqual([]);
        });
    });
});