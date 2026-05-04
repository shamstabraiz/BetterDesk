const bcrypt = require('./node_modules/bcrypt');
const Database = require('better-sqlite3');
const path = require('path');

// Platform-aware default path (C:\Yomie on Windows, /opt/rustdesk on Linux)
const isWindows = process.platform === 'win32';
const defaultPath = isWindows ? 'C:\\Yomie\\db_v2.sqlite3' : '/opt/rustdesk/db_v2.sqlite3';
const DB_PATH = process.env.DB_PATH || defaultPath;
const NEW_PASSWORD = process.argv[2] || 'admin';

async function resetPassword() {
    const hash = await bcrypt.hash(NEW_PASSWORD, 12);
    
    const db = new Database(DB_PATH);
    
    const result = db.prepare('UPDATE users SET password_hash = ? WHERE username = ?').run(hash, 'admin');
    
    if (result.changes > 0) {
        console.log('Password reset successful!');
        console.log('Username: admin');
        console.log('Password:', NEW_PASSWORD);
    } else {
        console.log('No admin user found, creating one...');
        db.prepare('INSERT INTO users (username, password_hash, role) VALUES (?, ?, ?)').run('admin', hash, 'admin');
        console.log('Admin user created!');
        console.log('Username: admin');
        console.log('Password:', NEW_PASSWORD);
    }
    
    db.close();
}

resetPassword().catch(console.error);
