/**
 * BetterDesk Console - Users Routes Tests
 */

const request = require('supertest');
const { createTestApp, withAuth } = require('./helpers');

const mockDb = {
    getAllUsers: jest.fn(),
    getUserById: jest.fn(),
    logAction: jest.fn().mockResolvedValue(undefined),
};

const mockUserSync = {
    backfillFromGo: jest.fn().mockResolvedValue({ imported: 0 }),
    resolveGoUserId: jest.fn(),
    mirrorCreate: jest.fn(),
    mirrorUpdate: jest.fn(),
    mirrorDelete: jest.fn(),
};

const mockApiClient = jest.fn();

jest.mock('../services/database', () => mockDb);
jest.mock('../services/userSync', () => mockUserSync);
jest.mock('../services/betterdeskApi', () => ({ apiClient: mockApiClient }));
jest.mock('../services/authService', () => ({
    validatePasswordStrength: jest.fn(() => ({ strength: 'strong', feedback: [] })),
    hashPassword: jest.fn().mockResolvedValue('hashed'),
}));
jest.mock('../middleware/rateLimiter', () => ({
    passwordChangeLimiter: (_req, _res, next) => next(),
}));

const usersRoutes = require('../routes/users.routes');

describe('Users Routes', () => {
    beforeEach(() => {
        jest.clearAllMocks();
        mockDb.getAllUsers.mockResolvedValue([
            { id: 1, username: 'admin', role: 'super_admin', created_at: '2026-05-01', last_login: null },
            { id: 12, username: 'operator1', role: 'operator', created_at: '2026-05-02', last_login: null },
        ]);
    });

    it('backfills Go users before returning the System Users list', async () => {
        const app = createTestApp();
        withAuth(app, { id: 1, username: 'admin', role: 'server_admin' });
        app.use(usersRoutes);

        const res = await request(app).get('/api/users');

        expect(res.status).toBe(200);
        expect(mockUserSync.backfillFromGo).toHaveBeenCalledTimes(1);
        expect(res.body.data.users).toHaveLength(2);
    });

    it('uses the Go user ID when assigning a local user to an organization', async () => {
        mockDb.getUserById.mockResolvedValue({ id: 12, username: 'operator1', role: 'operator' });
        mockUserSync.resolveGoUserId.mockResolvedValue(7);
        mockApiClient.mockResolvedValue({
            status: 201,
            data: { id: 'org-user-1', server_user_id: 7, org_id: 'org-1' },
        });

        const app = createTestApp();
        withAuth(app, { id: 1, username: 'admin', role: 'global_admin' });
        app.use(usersRoutes);

        const res = await request(app)
            .post('/api/users/12/organizations')
            .send({ org_id: 'org-1', role: 'operator' });

        expect(res.status).toBe(201);
        expect(mockApiClient).toHaveBeenCalledWith({
            method: 'post',
            url: '/users/7/organizations',
            data: { org_id: 'org-1', role: 'operator' },
        });
    });
});
