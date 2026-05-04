#!/usr/bin/env node
/**
 * Yomie Console — i18n Key Completeness Checker
 *
 * Compares all language files against en.json (reference) and reports:
 *   - Missing keys (present in en.json but absent in target)
 *   - Extra keys (present in target but absent in en.json)
 *   - Empty values (key exists but value is empty string)
 *
 * Usage:
 *   node scripts/i18n-check.js          # Check all languages
 *   node scripts/i18n-check.js --fix    # Add missing keys with English fallback
 *
 * Exit codes:
 *   0 = All languages complete
 *   1 = Missing or extra keys found
 */

const fs = require('fs');
const path = require('path');

const LANG_DIR = path.join(__dirname, '..', 'lang');
const REFERENCE = 'en.json';
const FIX_MODE = process.argv.includes('--fix');

/**
 * Recursively flatten nested JSON object into dot-notation keys
 * @param {object} obj
 * @param {string} prefix
 * @returns {Map<string, string>}
 */
function flattenKeys(obj, prefix = '') {
    const result = new Map();
    for (const [key, value] of Object.entries(obj)) {
        const fullKey = prefix ? `${prefix}.${key}` : key;
        if (value && typeof value === 'object' && !Array.isArray(value)) {
            for (const [k, v] of flattenKeys(value, fullKey)) {
                result.set(k, v);
            }
        } else {
            result.set(fullKey, String(value ?? ''));
        }
    }
    return result;
}

/**
 * Set a nested key in an object using dot notation
 * @param {object} obj
 * @param {string} dotKey
 * @param {string} value
 */
function setNestedKey(obj, dotKey, value) {
    const parts = dotKey.split('.');
    let current = obj;
    for (let i = 0; i < parts.length - 1; i++) {
        if (!(parts[i] in current) || typeof current[parts[i]] !== 'object') {
            current[parts[i]] = {};
        }
        current = current[parts[i]];
    }
    current[parts[parts.length - 1]] = value;
}

// --- Main ---

const refPath = path.join(LANG_DIR, REFERENCE);
if (!fs.existsSync(refPath)) {
    console.error(`Reference file not found: ${refPath}`);
    process.exit(1);
}

const refData = JSON.parse(fs.readFileSync(refPath, 'utf8'));
const refKeys = flattenKeys(refData);
const refKeySet = new Set(refKeys.keys());

console.log(`\n  Yomie i18n Checker`);
console.log(`  Reference: ${REFERENCE} (${refKeySet.size} keys)\n`);

const langFiles = fs.readdirSync(LANG_DIR)
    .filter(f => f.endsWith('.json') && f !== REFERENCE)
    .sort();

let hasErrors = false;
const summary = [];

for (const file of langFiles) {
    const filePath = path.join(LANG_DIR, file);
    let data;
    try {
        data = JSON.parse(fs.readFileSync(filePath, 'utf8'));
    } catch (e) {
        console.error(`  ✗ ${file}: Invalid JSON — ${e.message}`);
        hasErrors = true;
        summary.push({ file, missing: '?', extra: '?', empty: '?', status: 'PARSE ERROR' });
        continue;
    }

    const langKeys = flattenKeys(data);
    const langKeySet = new Set(langKeys.keys());

    const missing = [...refKeySet].filter(k => !langKeySet.has(k));
    const extra = [...langKeySet].filter(k => !refKeySet.has(k));
    const empty = [...langKeySet].filter(k => refKeySet.has(k) && langKeys.get(k) === '');

    const ok = missing.length === 0 && extra.length === 0;
    const icon = ok ? '✓' : '✗';

    console.log(`  ${icon} ${file}: ${langKeySet.size} keys`);

    if (missing.length > 0) {
        console.log(`    Missing (${missing.length}):`);
        for (const k of missing.slice(0, 20)) {
            console.log(`      - ${k}`);
        }
        if (missing.length > 20) {
            console.log(`      ... and ${missing.length - 20} more`);
        }
    }

    if (extra.length > 0) {
        console.log(`    Extra (${extra.length}):`);
        for (const k of extra.slice(0, 10)) {
            console.log(`      + ${k}`);
        }
        if (extra.length > 10) {
            console.log(`      ... and ${extra.length - 10} more`);
        }
    }

    if (empty.length > 0) {
        console.log(`    Empty values (${empty.length}):`);
        for (const k of empty.slice(0, 10)) {
            console.log(`      ~ ${k}`);
        }
    }

    // Fix mode: add missing keys with English values
    if (FIX_MODE && missing.length > 0) {
        for (const k of missing) {
            setNestedKey(data, k, refKeys.get(k));
        }
        fs.writeFileSync(filePath, JSON.stringify(data, null, 2) + '\n', 'utf8');
        console.log(`    → Fixed: added ${missing.length} missing keys with English fallback`);
    }

    if (!ok) hasErrors = true;

    const coverage = refKeySet.size > 0
        ? Math.round(((refKeySet.size - missing.length) / refKeySet.size) * 100)
        : 100;

    summary.push({
        file,
        missing: missing.length,
        extra: extra.length,
        empty: empty.length,
        coverage: `${coverage}%`,
        status: ok ? 'OK' : 'INCOMPLETE'
    });
}

// Summary table
console.log('\n  ┌──────────────┬─────────┬───────┬───────┬──────────┬──────────┐');
console.log('  │ Language      │ Missing │ Extra │ Empty │ Coverage │ Status   │');
console.log('  ├──────────────┼─────────┼───────┼───────┼──────────┼──────────┤');
for (const s of summary) {
    const lang = s.file.padEnd(12);
    const miss = String(s.missing).padStart(7);
    const ext = String(s.extra).padStart(5);
    const emp = String(s.empty).padStart(5);
    const cov = (s.coverage || '?').padStart(8);
    const stat = (s.status || '?').padStart(8);
    console.log(`  │ ${lang} │${miss} │${ext} │${emp} │${cov} │${stat} │`);
}
console.log('  └──────────────┴─────────┴───────┴───────┴──────────┴──────────┘');

if (hasErrors) {
    console.log('\n  ⚠ Some languages are incomplete. Run with --fix to add missing keys.\n');
    process.exit(1);
} else {
    console.log('\n  ✓ All languages have 100% key coverage.\n');
    process.exit(0);
}
