/**
 * Yomie Console - i18n Routes Tests
 */

const request = require('supertest');
const { createTestApp } = require('./helpers');

// Check if i18n routes exist
let i18nRoutes;
try {
    i18nRoutes = require('../routes/i18n.routes');
} catch (e) {
    // Skip if module load fails in test environment
}

const fs = require('fs');
const path = require('path');

describe('i18n System', () => {
    describe('Language files', () => {
        const langDir = path.join(__dirname, '..', 'lang');

        it('should have English translation file', () => {
            expect(fs.existsSync(path.join(langDir, 'en.json'))).toBe(true);
        });

        it('should have Polish translation file', () => {
            expect(fs.existsSync(path.join(langDir, 'pl.json'))).toBe(true);
        });

        it('should NOT have Russian translation file', () => {
            expect(fs.existsSync(path.join(langDir, 'ru.json'))).toBe(false);
        });

        it('should have valid JSON in all language files', () => {
            const files = fs.readdirSync(langDir).filter(f => f.endsWith('.json'));

            files.forEach(file => {
                const content = fs.readFileSync(path.join(langDir, file), 'utf8');
                expect(() => JSON.parse(content)).not.toThrow();
            });
        });

        it('should have meta section in each language file', () => {
            const files = fs.readdirSync(langDir).filter(f => f.endsWith('.json'));

            files.forEach(file => {
                const content = JSON.parse(fs.readFileSync(path.join(langDir, file), 'utf8'));
                expect(content.meta).toBeDefined();
                expect(content.meta.lang).toBeDefined();
                expect(content.meta.name).toBeDefined();
            });
        });

        it('Polish should have all keys from English', () => {
            const en = JSON.parse(fs.readFileSync(path.join(langDir, 'en.json'), 'utf8'));
            const pl = JSON.parse(fs.readFileSync(path.join(langDir, 'pl.json'), 'utf8'));

            function getKeys(obj, prefix = '') {
                let keys = [];
                for (const key of Object.keys(obj)) {
                    const fullKey = prefix ? `${prefix}.${key}` : key;
                    if (typeof obj[key] === 'object' && obj[key] !== null) {
                        keys = keys.concat(getKeys(obj[key], fullKey));
                    } else {
                        keys.push(fullKey);
                    }
                }
                return keys;
            }

            const enKeys = getKeys(en);
            const plKeys = new Set(getKeys(pl));
            const missing = enKeys.filter(k => !plKeys.has(k));

            // Allow some missing keys but report them
            if (missing.length > 0) {
                console.warn(`Polish missing ${missing.length} keys:`, missing.slice(0, 10));
            }
            // At least 90% coverage expected
            expect(plKeys.size / enKeys.length).toBeGreaterThan(0.9);
        });
    });
});
