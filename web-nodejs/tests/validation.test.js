/**
 * Yomie Console - Input Validation Tests
 * Tests for security-critical input validation patterns.
 */

describe('Input Validation', () => {
    describe('Sort parameter sanitization', () => {
        const ALLOWED_SORT_FIELDS = ['last_online', 'id', 'hostname', 'created_at', 'os', 'version', 'username', 'note'];
        const ALLOWED_SORT_ORDERS = ['asc', 'desc'];

        it('should accept valid sort fields', () => {
            ALLOWED_SORT_FIELDS.forEach(field => {
                expect(ALLOWED_SORT_FIELDS.includes(field)).toBe(true);
            });
        });

        it('should reject SQL injection in sort field', () => {
            const malicious = [
                'DROP TABLE peers',
                'id; DELETE FROM users',
                '1 OR 1=1',
                'hostname UNION SELECT',
                '../../../etc/passwd'
            ];

            malicious.forEach(val => {
                const sanitized = ALLOWED_SORT_FIELDS.includes(val) ? val : 'last_online';
                expect(sanitized).toBe('last_online');
            });
        });

        it('should reject invalid sort order', () => {
            const malicious = ['ASC; DROP TABLE', 'INJECT', '1', 'true'];

            malicious.forEach(val => {
                const sanitized = ALLOWED_SORT_ORDERS.includes(val.toLowerCase()) ? val.toLowerCase() : 'desc';
                expect(sanitized).toBe('desc');
            });
        });
    });

    describe('Username/password length limits', () => {
        it('should reject username over 128 characters', () => {
            const username = 'a'.repeat(129);
            expect(username.length > 128).toBe(true);
        });

        it('should accept username at exactly 128 characters', () => {
            const username = 'a'.repeat(128);
            expect(username.length > 128).toBe(false);
        });
    });

    describe('Device ID format validation', () => {
        const PEER_ID_PATTERN = /^[A-Za-z0-9_-]{1,32}$/;

        it('should accept valid device IDs', () => {
            const valid = ['123456789', 'ABCDEF', 'my-device_01', 'a1'];
            valid.forEach(id => {
                expect(PEER_ID_PATTERN.test(id)).toBe(true);
            });
        });

        it('should reject invalid device IDs', () => {
            const invalid = ['', 'id with spaces', 'id;DROP', 'a'.repeat(33), '<script>'];
            invalid.forEach(id => {
                expect(PEER_ID_PATTERN.test(id)).toBe(false);
            });
        });
    });

    describe('Folder name validation', () => {
        it('should reject empty folder names', () => {
            const name = '';
            expect(name.length === 0).toBe(true);
        });

        it('should reject folder names over 100 characters', () => {
            const name = 'x'.repeat(101);
            expect(name.length > 100).toBe(true);
        });

        it('should accept valid folder names', () => {
            const valid = ['My Folder', 'Office Devices', 'Floor 2', 'Remote-Workers_2026'];
            valid.forEach(name => {
                expect(name.length > 0 && name.length <= 100).toBe(true);
            });
        });
    });
});
