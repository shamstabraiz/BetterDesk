// migrate — Yomie database migration tool.
//
// Supports multiple migration modes:
//
//	rust2go   — Rust hbbs-patch-v2 SQLite → Yomie Go schema (SQLite or PG)
//	sqlite2pg — Yomie Go SQLite → PostgreSQL
//	pg2sqlite — PostgreSQL → Yomie Go SQLite
//	nodejs2go — Node.js console SQLite → Yomie Go schema (SQLite or PG)
//	backup    — Create timestamped backup of a SQLite file
//
// Usage:
//
//	./migrate -mode rust2go   -src /opt/rustdesk/db_v2.sqlite3
//	./migrate -mode rust2go   -src db_v2.sqlite3 -dst postgres://user:pass@host/db
//	./migrate -mode sqlite2pg -src ./db_v2.sqlite3 -dst postgres://user:pass@host/db
//	./migrate -mode pg2sqlite -src postgres://user:pass@host/db -dst ./db_v2.sqlite3
//	./migrate -mode nodejs2go -src /opt/yomie/data/db_v2.sqlite3 -node-auth /opt/yomie/data/auth.sqlite3
//	./migrate -mode nodejs2go -src db_v2.sqlite3 -node-auth auth.sqlite3 -dst postgres://user:pass@host/db
//	./migrate -mode backup    -src db_v2.sqlite3
package main

import (
	"context"
	"database/sql"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	_ "modernc.org/sqlite"
)

// ── Types ─────────────────────────────────────────────────────────────

// rustPeer represents a row from the Rust hbbs-patch-v2 database.
type rustPeer struct {
	GUID        []byte
	ID          string
	UUID        []byte
	PK          []byte
	CreatedAt   string
	User        []byte
	Status      *int64
	Note        *string
	Info        string
	PreviousIDs *string
	IDChangedAt *string
	IsDeleted   *int64
	IsBanned    *int64
	LastOnline  *string
}

// peerInfo is the JSON structure stored in the Rust "info" field.
type peerInfo struct {
	Hostname string `json:"hostname"`
	OS       string `json:"os"`
	Version  string `json:"version"`
	Username string `json:"username"`
}

// ── Main / CLI ────────────────────────────────────────────────────────

func main() {
	mode := flag.String("mode", "", "Migration mode: rust2go, sqlite2pg, pg2sqlite, nodejs2go, backup")
	src := flag.String("src", "", "Source: SQLite file path or postgres:// DSN")
	dst := flag.String("dst", "", "Destination: SQLite file path or postgres:// DSN (default: auto)")
	nodeAuth := flag.String("node-auth", "", "Path to Node.js auth.sqlite3 (nodejs2go mode)")
	backupOnly := flag.Bool("backup-only", false, "Create backup only (legacy flag, same as -mode backup)")
	flag.Parse()

	// Legacy compatibility: -src without -mode assumes rust2go
	if *mode == "" && *src != "" {
		if *backupOnly {
			*mode = "backup"
		} else {
			*mode = "rust2go"
		}
	}

	switch *mode {
	case "rust2go":
		runRust2Go(*src, *dst)
	case "sqlite2pg":
		runSQLite2PG(*src, *dst)
	case "pg2sqlite":
		runPG2SQLite(*src, *dst)
	case "nodejs2go":
		runNodeJS2Go(*src, *nodeAuth, *dst)
	case "backup":
		if *src == "" {
			log.Fatal("Usage: migrate -mode backup -src <path>")
		}
		bp := createBackup(*src)
		log.Printf("Backup created: %s", bp)
	default:
		printUsage()
		os.Exit(1)
	}
}

func printUsage() {
	fmt.Println("Yomie Database Migration Tool")
	fmt.Println()
	fmt.Println("Usage: migrate -mode <mode> -src <source> [-dst <dest>] [options]")
	fmt.Println()
	fmt.Println("Modes:")
	fmt.Println("  rust2go   - Migrate Rust hbbs-patch-v2 → Yomie Go schema")
	fmt.Println("  sqlite2pg - Migrate Yomie Go SQLite → PostgreSQL")
	fmt.Println("  pg2sqlite - Migrate PostgreSQL → Yomie Go SQLite")
	fmt.Println("  nodejs2go - Migrate Node.js console → Yomie Go schema")
	fmt.Println("  backup    - Create timestamped backup of a SQLite file")
	fmt.Println()
	fmt.Println("Examples:")
	fmt.Println("  ./migrate -mode rust2go   -src /opt/rustdesk/db_v2.sqlite3")
	fmt.Println("  ./migrate -mode sqlite2pg -src ./db_v2.sqlite3 -dst postgres://user:pass@host/yomie")
	fmt.Println("  ./migrate -mode pg2sqlite -src postgres://user:pass@host/yomie -dst ./db_v2.sqlite3")
	fmt.Println("  ./migrate -mode nodejs2go -src db_v2.sqlite3 -node-auth auth.sqlite3 -dst postgres://...")
	fmt.Println("  ./migrate -mode backup    -src db_v2.sqlite3")
}

// isPG returns true if the DSN looks like a PostgreSQL connection string.
func isPG(dsn string) bool {
	lower := strings.ToLower(dsn)
	return strings.HasPrefix(lower, "postgres://") || strings.HasPrefix(lower, "postgresql://")
}

// ── Mode: rust2go ─────────────────────────────────────────────────────

func runRust2Go(srcPath, dstPath string) {
	if srcPath == "" {
		log.Fatal("Usage: migrate -mode rust2go -src <rust-db-path> [-dst <dest>]")
	}
	requireFile(srcPath)
	bp := createBackup(srcPath)
	log.Printf("Backup created: %s", bp)

	peers, err := readRustDB(srcPath)
	if err != nil {
		log.Fatalf("Failed to read source database: %v", err)
	}
	log.Printf("Read %d peers from Rust database", len(peers))

	if dstPath == "" {
		dstPath = filepath.Join(filepath.Dir(srcPath), "yomie.sqlite3")
	}

	if isPG(dstPath) {
		count, err := writeRustPeersPG(dstPath, peers)
		if err != nil {
			log.Fatalf("Failed to write to PostgreSQL: %v", err)
		}
		log.Printf("Migration complete: %d peers → PostgreSQL", count)
	} else {
		count, err := writeRustPeersSQLite(dstPath, peers)
		if err != nil {
			log.Fatalf("Failed to write to SQLite: %v", err)
		}
		log.Printf("Migration complete: %d peers → %s", count, dstPath)
	}
}

// readRustDB reads all peers from the old Rust hbbs-patch-v2 database.
func readRustDB(path string) ([]rustPeer, error) {
	db, err := sql.Open("sqlite", fmt.Sprintf("file:%s?mode=ro&_journal_mode=WAL", path))
	if err != nil {
		return nil, fmt.Errorf("open: %w", err)
	}
	defer db.Close()

	var tableName string
	err = db.QueryRow("SELECT name FROM sqlite_master WHERE type='table' AND name='peer'").Scan(&tableName)
	if err != nil {
		return nil, fmt.Errorf("source database has no 'peer' table — is this a Rust hbbs database? %w", err)
	}

	cols := getColumns(db, "peer")
	hasPreviousIDs := contains(cols, "previous_ids")
	hasIDChangedAt := contains(cols, "id_changed_at")
	hasIsDeleted := contains(cols, "is_deleted")
	hasIsBanned := contains(cols, "is_banned")
	hasLastOnline := contains(cols, "last_online")

	query := "SELECT guid, id, uuid, pk, created_at, user, status, note, info"
	if hasPreviousIDs {
		query += ", previous_ids"
	}
	if hasIDChangedAt {
		query += ", id_changed_at"
	}
	if hasIsDeleted {
		query += ", is_deleted"
	}
	if hasIsBanned {
		query += ", is_banned"
	}
	if hasLastOnline {
		query += ", last_online"
	}
	query += " FROM peer"

	rows, err := db.Query(query)
	if err != nil {
		return nil, fmt.Errorf("query peers: %w", err)
	}
	defer rows.Close()

	var peers []rustPeer
	for rows.Next() {
		var p rustPeer
		scanArgs := []any{&p.GUID, &p.ID, &p.UUID, &p.PK, &p.CreatedAt, &p.User, &p.Status, &p.Note, &p.Info}
		if hasPreviousIDs {
			scanArgs = append(scanArgs, &p.PreviousIDs)
		}
		if hasIDChangedAt {
			scanArgs = append(scanArgs, &p.IDChangedAt)
		}
		if hasIsDeleted {
			scanArgs = append(scanArgs, &p.IsDeleted)
		}
		if hasIsBanned {
			scanArgs = append(scanArgs, &p.IsBanned)
		}
		if hasLastOnline {
			scanArgs = append(scanArgs, &p.LastOnline)
		}
		if err := rows.Scan(scanArgs...); err != nil {
			log.Printf("WARN: skipping row: %v", err)
			continue
		}
		peers = append(peers, p)
	}
	return peers, rows.Err()
}

// writeRustPeersSQLite writes Rust peers to a Yomie Go SQLite database.
func writeRustPeersSQLite(path string, peers []rustPeer) (int, error) {
	os.Remove(path)
	db, err := sql.Open("sqlite", fmt.Sprintf("file:%s?_journal_mode=WAL&_busy_timeout=5000", path))
	if err != nil {
		return 0, fmt.Errorf("open destination: %w", err)
	}
	defer db.Close()

	for _, stmt := range sqliteSchemaStatements() {
		if _, err := db.Exec(stmt); err != nil {
			return 0, fmt.Errorf("migration: %w", err)
		}
	}

	tx, err := db.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()

	stmt, err := tx.Prepare(`INSERT OR REPLACE INTO peers
		(id, uuid, pk, user, hostname, os, version, status, last_online, created_at,
		 banned, soft_deleted, note)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return 0, err
	}
	defer stmt.Close()

	count := 0
	for _, p := range peers {
		info, uuid, status, user, lastOnline, note, banned, deleted := convertRustPeer(p)
		_, err := stmt.Exec(p.ID, uuid, p.PK, user, info.Hostname, info.OS, info.Version,
			status, lastOnline, p.CreatedAt, banned, deleted, note)
		if err != nil {
			log.Printf("WARN: peer %s: %v", p.ID, err)
			continue
		}
		count++
	}
	return count, tx.Commit()
}

// writeRustPeersPG writes Rust peers to a Yomie Go PostgreSQL database.
func writeRustPeersPG(dsn string, peers []rustPeer) (int, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, dsn)
	if err != nil {
		return 0, fmt.Errorf("connect: %w", err)
	}
	defer pool.Close()

	for _, stmt := range pgSchemaStatements() {
		if _, err := pool.Exec(ctx, stmt); err != nil {
			return 0, fmt.Errorf("schema: %w", err)
		}
	}

	count := 0
	for _, p := range peers {
		info, uuid, status, user, lastOnline, note, banned, deleted := convertRustPeer(p)
		_, err := pool.Exec(ctx, `
			INSERT INTO peers (id, uuid, pk, "user", hostname, os, version,
			                   status, last_online, created_at, banned, soft_deleted, note)
			VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
			ON CONFLICT (id) DO NOTHING`,
			p.ID, uuid, p.PK, user, info.Hostname, info.OS, info.Version,
			status, parseTime(lastOnline), parseTime(p.CreatedAt),
			banned != 0, deleted != 0, note)
		if err != nil {
			log.Printf("WARN: peer %s: %v", p.ID, err)
			continue
		}
		count++
	}
	return count, nil
}

// convertRustPeer extracts common fields from a Rust peer struct.
func convertRustPeer(p rustPeer) (info peerInfo, uuid, status, user, lastOnline, note string, banned, deleted int) {
	if p.Info != "" {
		_ = json.Unmarshal([]byte(p.Info), &info)
	}
	uuid = fmt.Sprintf("%x", p.UUID)
	status = "OFFLINE"
	if p.Status != nil && *p.Status == 1 {
		status = "ONLINE"
	}
	if p.User != nil {
		user = string(p.User)
	}
	if p.LastOnline != nil {
		lastOnline = *p.LastOnline
	}
	if p.Note != nil {
		note = *p.Note
	}
	if p.IsBanned != nil && *p.IsBanned == 1 {
		banned = 1
	}
	if p.IsDeleted != nil && *p.IsDeleted == 1 {
		deleted = 1
	}
	return
}

// ── Mode: sqlite2pg ───────────────────────────────────────────────────

func runSQLite2PG(srcPath, dstDSN string) {
	if srcPath == "" || dstDSN == "" || !isPG(dstDSN) {
		log.Fatal("Usage: migrate -mode sqlite2pg -src <sqlite-path> -dst <postgres://...>")
	}
	requireFile(srcPath)
	bp := createBackup(srcPath)
	log.Printf("Backup created: %s", bp)

	srcDB, err := sql.Open("sqlite", fmt.Sprintf("file:%s?mode=ro&_journal_mode=WAL", srcPath))
	if err != nil {
		log.Fatalf("Open source: %v", err)
	}
	defer srcDB.Close()

	ctx := context.Background()
	pool, err := pgxpool.New(ctx, dstDSN)
	if err != nil {
		log.Fatalf("Connect to PostgreSQL: %v", err)
	}
	defer pool.Close()
	if err := pool.Ping(ctx); err != nil {
		log.Fatalf("PostgreSQL ping: %v", err)
	}

	log.Println("Creating PostgreSQL schema...")
	for _, stmt := range pgSchemaStatements() {
		if _, err := pool.Exec(ctx, stmt); err != nil {
			log.Fatalf("Schema creation failed: %v", err)
		}
	}

	// Detect source schema type
	schema := detectSQLiteSchema(srcDB)
	log.Printf("Detected source schema: %s", schema)

	var total int

	// Copy peers
	if tableExists(srcDB, "peers") {
		n, err := copyPeersSQLite2PG(ctx, srcDB, pool)
		if err != nil {
			log.Fatalf("Copy peers: %v", err)
		}
		log.Printf("  peers: %d rows", n)
		total += n
	}

	// Copy server_config
	if tableExists(srcDB, "server_config") {
		n, err := copyConfigSQLite2PG(ctx, srcDB, pool)
		if err != nil {
			log.Fatalf("Copy server_config: %v", err)
		}
		log.Printf("  server_config: %d rows", n)
		total += n
	}

	// Copy id_change_history
	if tableExists(srcDB, "id_change_history") {
		n, err := copyIDHistorySQLite2PG(ctx, srcDB, pool)
		if err != nil {
			log.Fatalf("Copy id_change_history: %v", err)
		}
		log.Printf("  id_change_history: %d rows", n)
		total += n
	}

	// Copy users
	if tableExists(srcDB, "users") {
		n, err := copyUsersSQLite2PG(ctx, srcDB, pool)
		if err != nil {
			log.Fatalf("Copy users: %v", err)
		}
		log.Printf("  users: %d rows", n)
		total += n
	}

	// Copy api_keys
	if tableExists(srcDB, "api_keys") {
		n, err := copyAPIKeysSQLite2PG(ctx, srcDB, pool)
		if err != nil {
			log.Fatalf("Copy api_keys: %v", err)
		}
		log.Printf("  api_keys: %d rows", n)
		total += n
	}

	log.Printf("Migration complete: %d total rows copied to PostgreSQL", total)
}

func copyPeersSQLite2PG(ctx context.Context, src *sql.DB, dst *pgxpool.Pool) (int, error) {
	rows, err := src.Query(`SELECT id, uuid, pk, ip, user, hostname, os, version,
		status, nat_type, last_online, created_at,
		disabled, banned, ban_reason, banned_at,
		soft_deleted, deleted_at, note, tags, heartbeat_seq FROM peers`)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	count := 0
	for rows.Next() {
		var (
			id, uuid, ip, user, hostname, osStr, version, status string
			banReason, note, tags                                string
			natType                                              int
			heartbeatSeq                                         int64
			pk                                                   []byte
			lastOnlineStr, createdAtStr                          string
			bannedAtStr, deletedAtStr                            sql.NullString
			disabledInt, bannedInt, softDeletedInt               int
		)
		if err := rows.Scan(&id, &uuid, &pk, &ip, &user, &hostname, &osStr, &version,
			&status, &natType, &lastOnlineStr, &createdAtStr,
			&disabledInt, &bannedInt, &banReason, &bannedAtStr,
			&softDeletedInt, &deletedAtStr, &note, &tags, &heartbeatSeq); err != nil {
			log.Printf("WARN: skip peer row: %v", err)
			continue
		}

		_, err := dst.Exec(ctx, `
			INSERT INTO peers (id, uuid, pk, ip, "user", hostname, os, version,
			    status, nat_type, last_online, created_at,
			    disabled, banned, ban_reason, banned_at,
			    soft_deleted, deleted_at, note, tags, heartbeat_seq)
			VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21)
			ON CONFLICT (id) DO NOTHING`,
			id, uuid, pk, ip, user, hostname, osStr, version,
			status, natType, parseTime(lastOnlineStr), parseTimeDefault(createdAtStr),
			disabledInt != 0, bannedInt != 0, banReason, parseNullTime(bannedAtStr),
			softDeletedInt != 0, parseNullTime(deletedAtStr), note, tags, heartbeatSeq)
		if err != nil {
			log.Printf("WARN: peer %s: %v", id, err)
			continue
		}
		count++
	}
	return count, rows.Err()
}

func copyConfigSQLite2PG(ctx context.Context, src *sql.DB, dst *pgxpool.Pool) (int, error) {
	rows, err := src.Query(`SELECT key, value FROM server_config`)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	count := 0
	for rows.Next() {
		var key, value string
		if err := rows.Scan(&key, &value); err != nil {
			continue
		}
		_, err := dst.Exec(ctx, `INSERT INTO server_config (key, value) VALUES ($1, $2)
			ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value`, key, value)
		if err != nil {
			log.Printf("WARN: config %s: %v", key, err)
			continue
		}
		count++
	}
	return count, rows.Err()
}

func copyIDHistorySQLite2PG(ctx context.Context, src *sql.DB, dst *pgxpool.Pool) (int, error) {
	rows, err := src.Query(`SELECT old_id, new_id, changed_at, reason FROM id_change_history`)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	count := 0
	for rows.Next() {
		var oldID, newID, reason string
		var changedAtStr string
		if err := rows.Scan(&oldID, &newID, &changedAtStr, &reason); err != nil {
			continue
		}
		_, err := dst.Exec(ctx, `INSERT INTO id_change_history (old_id, new_id, changed_at, reason)
			VALUES ($1, $2, $3, $4)`, oldID, newID, parseTimeDefault(changedAtStr), reason)
		if err != nil {
			log.Printf("WARN: id_history %s→%s: %v", oldID, newID, err)
			continue
		}
		count++
	}
	return count, rows.Err()
}

func copyUsersSQLite2PG(ctx context.Context, src *sql.DB, dst *pgxpool.Pool) (int, error) {
	rows, err := src.Query(`SELECT username, password_hash, role, totp_secret, totp_enabled,
		created_at, last_login FROM users`)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	count := 0
	for rows.Next() {
		var (
			username, passwordHash, role string
			totpSecret                   sql.NullString
			totpEnabled                  int
			createdAt, lastLogin         string
		)
		if err := rows.Scan(&username, &passwordHash, &role, &totpSecret,
			&totpEnabled, &createdAt, &lastLogin); err != nil {
			log.Printf("WARN: skip user row: %v", err)
			continue
		}

		secret := ""
		if totpSecret.Valid {
			secret = totpSecret.String
		}

		_, err := dst.Exec(ctx, `
			INSERT INTO users (username, password_hash, role, totp_secret, totp_enabled,
			    created_at, last_login)
			VALUES ($1, $2, $3, $4, $5, $6, $7)
			ON CONFLICT (username) DO NOTHING`,
			username, passwordHash, role, secret, totpEnabled != 0,
			parseTimeDefault(createdAt), parseTime(lastLogin))
		if err != nil {
			log.Printf("WARN: user %s: %v", username, err)
			continue
		}
		count++
	}
	return count, rows.Err()
}

func copyAPIKeysSQLite2PG(ctx context.Context, src *sql.DB, dst *pgxpool.Pool) (int, error) {
	rows, err := src.Query(`SELECT key_hash, key_prefix, name, role, created_at, expires_at, last_used
		FROM api_keys`)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	count := 0
	for rows.Next() {
		var keyHash, keyPrefix, name, role string
		var createdAt, expiresAt, lastUsed string
		if err := rows.Scan(&keyHash, &keyPrefix, &name, &role, &createdAt, &expiresAt, &lastUsed); err != nil {
			continue
		}
		_, err := dst.Exec(ctx, `
			INSERT INTO api_keys (key_hash, key_prefix, name, role, created_at, expires_at, last_used)
			VALUES ($1, $2, $3, $4, $5, $6, $7)
			ON CONFLICT (key_hash) DO NOTHING`,
			keyHash, keyPrefix, name, role,
			parseTimeDefault(createdAt), parseTime(expiresAt), parseTime(lastUsed))
		if err != nil {
			log.Printf("WARN: api_key %s: %v", keyPrefix, err)
			continue
		}
		count++
	}
	return count, rows.Err()
}

// ── Mode: pg2sqlite ───────────────────────────────────────────────────

func runPG2SQLite(srcDSN, dstPath string) {
	if srcDSN == "" || !isPG(srcDSN) || dstPath == "" {
		log.Fatal("Usage: migrate -mode pg2sqlite -src <postgres://...> -dst <sqlite-path>")
	}

	ctx := context.Background()
	pool, err := pgxpool.New(ctx, srcDSN)
	if err != nil {
		log.Fatalf("Connect to PostgreSQL: %v", err)
	}
	defer pool.Close()
	if err := pool.Ping(ctx); err != nil {
		log.Fatalf("PostgreSQL ping: %v", err)
	}

	// Create fresh SQLite
	os.Remove(dstPath)
	dstDB, err := sql.Open("sqlite", fmt.Sprintf("file:%s?_journal_mode=WAL&_busy_timeout=5000", dstPath))
	if err != nil {
		log.Fatalf("Open SQLite: %v", err)
	}
	defer dstDB.Close()

	log.Println("Creating SQLite schema...")
	for _, stmt := range sqliteSchemaStatements() {
		if _, err := dstDB.Exec(stmt); err != nil {
			log.Fatalf("Schema creation failed: %v", err)
		}
	}

	var total int

	// Copy peers
	n, err := copyPeersPG2SQLite(ctx, pool, dstDB)
	if err != nil {
		log.Fatalf("Copy peers: %v", err)
	}
	log.Printf("  peers: %d rows", n)
	total += n

	// Copy server_config
	n, err = copyConfigPG2SQLite(ctx, pool, dstDB)
	if err != nil {
		log.Fatalf("Copy server_config: %v", err)
	}
	log.Printf("  server_config: %d rows", n)
	total += n

	// Copy id_change_history
	n, err = copyIDHistoryPG2SQLite(ctx, pool, dstDB)
	if err != nil {
		log.Fatalf("Copy id_change_history: %v", err)
	}
	log.Printf("  id_change_history: %d rows", n)
	total += n

	// Copy users
	n, err = copyUsersPG2SQLite(ctx, pool, dstDB)
	if err != nil {
		log.Fatalf("Copy users: %v", err)
	}
	log.Printf("  users: %d rows", n)
	total += n

	// Copy api_keys
	n, err = copyAPIKeysPG2SQLite(ctx, pool, dstDB)
	if err != nil {
		log.Fatalf("Copy api_keys: %v", err)
	}
	log.Printf("  api_keys: %d rows", n)
	total += n

	log.Printf("Migration complete: %d total rows copied to %s", total, dstPath)
}

func copyPeersPG2SQLite(ctx context.Context, src *pgxpool.Pool, dst *sql.DB) (int, error) {
	rows, err := src.Query(ctx, `SELECT id, uuid, pk, ip, "user", hostname, os, version,
		status, nat_type, last_online, created_at,
		disabled, banned, ban_reason, banned_at,
		soft_deleted, deleted_at, note, tags, heartbeat_seq FROM peers`)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	tx, err := dst.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()

	stmt, err := tx.Prepare(`INSERT OR REPLACE INTO peers
		(id, uuid, pk, ip, user, hostname, os, version,
		 status, nat_type, last_online, created_at,
		 disabled, banned, ban_reason, banned_at,
		 soft_deleted, deleted_at, note, tags, heartbeat_seq)
		VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`)
	if err != nil {
		return 0, err
	}
	defer stmt.Close()

	count := 0
	for rows.Next() {
		var (
			id, uuid, ip, user, hostname, osStr, version, status string
			banReason, note, tags                                string
			natType                                              int
			heartbeatSeq                                         int64
			pk                                                   []byte
			lastOnline, createdAt                                *time.Time
			bannedAt, deletedAt                                  *time.Time
			disabled, banned, softDeleted                        bool
		)
		if err := rows.Scan(&id, &uuid, &pk, &ip, &user, &hostname, &osStr, &version,
			&status, &natType, &lastOnline, &createdAt,
			&disabled, &banned, &banReason, &bannedAt,
			&softDeleted, &deletedAt, &note, &tags, &heartbeatSeq); err != nil {
			log.Printf("WARN: skip peer: %v", err)
			continue
		}
		_, err := stmt.Exec(id, uuid, pk, ip, user, hostname, osStr, version,
			status, natType, formatTime(lastOnline), formatTimeDefault(createdAt),
			boolToInt(disabled), boolToInt(banned), banReason, formatTime(bannedAt),
			boolToInt(softDeleted), formatTime(deletedAt), note, tags, heartbeatSeq)
		if err != nil {
			log.Printf("WARN: peer %s: %v", id, err)
			continue
		}
		count++
	}
	if err := rows.Err(); err != nil {
		return count, err
	}
	return count, tx.Commit()
}

func copyConfigPG2SQLite(ctx context.Context, src *pgxpool.Pool, dst *sql.DB) (int, error) {
	rows, err := src.Query(ctx, `SELECT key, value FROM server_config`)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	tx, err := dst.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()

	count := 0
	for rows.Next() {
		var key, value string
		if err := rows.Scan(&key, &value); err != nil {
			continue
		}
		tx.Exec(`INSERT OR REPLACE INTO server_config (key, value) VALUES (?, ?)`, key, value)
		count++
	}
	if err := rows.Err(); err != nil {
		return count, err
	}
	return count, tx.Commit()
}

func copyIDHistoryPG2SQLite(ctx context.Context, src *pgxpool.Pool, dst *sql.DB) (int, error) {
	rows, err := src.Query(ctx, `SELECT old_id, new_id, changed_at, reason FROM id_change_history ORDER BY id`)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	tx, err := dst.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()

	count := 0
	for rows.Next() {
		var oldID, newID, reason string
		var changedAt time.Time
		if err := rows.Scan(&oldID, &newID, &changedAt, &reason); err != nil {
			continue
		}
		tx.Exec(`INSERT INTO id_change_history (old_id, new_id, changed_at, reason) VALUES (?, ?, ?, ?)`,
			oldID, newID, changedAt.Format("2006-01-02 15:04:05"), reason)
		count++
	}
	if err := rows.Err(); err != nil {
		return count, err
	}
	return count, tx.Commit()
}

func copyUsersPG2SQLite(ctx context.Context, src *pgxpool.Pool, dst *sql.DB) (int, error) {
	rows, err := src.Query(ctx, `SELECT username, password_hash, role, totp_secret, totp_enabled,
		created_at, last_login FROM users ORDER BY id`)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	tx, err := dst.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()

	count := 0
	for rows.Next() {
		var (
			username, passwordHash, role, totpSecret string
			totpEnabled                              bool
			createdAt                                *time.Time
			lastLogin                                *time.Time
		)
		if err := rows.Scan(&username, &passwordHash, &role, &totpSecret, &totpEnabled,
			&createdAt, &lastLogin); err != nil {
			log.Printf("WARN: skip user: %v", err)
			continue
		}
		tx.Exec(`INSERT OR REPLACE INTO users
			(username, password_hash, role, totp_secret, totp_enabled, created_at, last_login)
			VALUES (?, ?, ?, ?, ?, ?, ?)`,
			username, passwordHash, role, totpSecret, boolToInt(totpEnabled),
			formatTimeDefault(createdAt), formatTime(lastLogin))
		count++
	}
	if err := rows.Err(); err != nil {
		return count, err
	}
	return count, tx.Commit()
}

func copyAPIKeysPG2SQLite(ctx context.Context, src *pgxpool.Pool, dst *sql.DB) (int, error) {
	rows, err := src.Query(ctx, `SELECT key_hash, key_prefix, name, role,
		created_at, expires_at, last_used FROM api_keys ORDER BY id`)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	tx, err := dst.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()

	count := 0
	for rows.Next() {
		var keyHash, keyPrefix, name, role string
		var createdAt, expiresAt, lastUsed *time.Time
		if err := rows.Scan(&keyHash, &keyPrefix, &name, &role, &createdAt, &expiresAt, &lastUsed); err != nil {
			continue
		}
		tx.Exec(`INSERT OR REPLACE INTO api_keys
			(key_hash, key_prefix, name, role, created_at, expires_at, last_used)
			VALUES (?, ?, ?, ?, ?, ?, ?)`,
			keyHash, keyPrefix, name, role,
			formatTimeDefault(createdAt), formatTime(expiresAt), formatTime(lastUsed))
		count++
	}
	if err := rows.Err(); err != nil {
		return count, err
	}
	return count, tx.Commit()
}

// ── Mode: nodejs2go ───────────────────────────────────────────────────

func runNodeJS2Go(srcPeerPath, srcAuthPath, dstPath string) {
	if srcPeerPath == "" {
		log.Fatal("Usage: migrate -mode nodejs2go -src <peer-db> [-node-auth <auth-db>] [-dst <dest>]")
	}
	requireFile(srcPeerPath)

	bp := createBackup(srcPeerPath)
	log.Printf("Peer DB backup: %s", bp)

	if srcAuthPath != "" {
		requireFile(srcAuthPath)
		bp = createBackup(srcAuthPath)
		log.Printf("Auth DB backup: %s", bp)
	}

	if dstPath == "" {
		dstPath = filepath.Join(filepath.Dir(srcPeerPath), "yomie.sqlite3")
	}

	if isPG(dstPath) {
		total, err := migrateNodeJS2PG(srcPeerPath, srcAuthPath, dstPath)
		if err != nil {
			log.Fatalf("Migration failed: %v", err)
		}
		log.Printf("Migration complete: %d total rows → PostgreSQL", total)
	} else {
		total, err := migrateNodeJS2SQLite(srcPeerPath, srcAuthPath, dstPath)
		if err != nil {
			log.Fatalf("Migration failed: %v", err)
		}
		log.Printf("Migration complete: %d total rows → %s", total, dstPath)
	}
}

func migrateNodeJS2SQLite(srcPeerPath, srcAuthPath, dstPath string) (int, error) {
	os.Remove(dstPath)
	dstDB, err := sql.Open("sqlite", fmt.Sprintf("file:%s?_journal_mode=WAL&_busy_timeout=5000", dstPath))
	if err != nil {
		return 0, err
	}
	defer dstDB.Close()

	for _, stmt := range sqliteSchemaStatements() {
		if _, err := dstDB.Exec(stmt); err != nil {
			return 0, fmt.Errorf("schema: %w", err)
		}
	}

	total := 0

	// Migrate Node.js peer table → Go peers table
	srcPeerDB, err := sql.Open("sqlite", fmt.Sprintf("file:%s?mode=ro", srcPeerPath))
	if err != nil {
		return 0, err
	}
	defer srcPeerDB.Close()

	if tableExists(srcPeerDB, "peer") {
		n, err := copyNodePeers2SQLite(srcPeerDB, dstDB)
		if err != nil {
			return total, fmt.Errorf("peers: %w", err)
		}
		log.Printf("  peers: %d rows (from Node.js peer table)", n)
		total += n
	}

	// Migrate Node.js users table → Go users table
	if srcAuthPath != "" {
		srcAuthDB, err := sql.Open("sqlite", fmt.Sprintf("file:%s?mode=ro", srcAuthPath))
		if err != nil {
			return total, err
		}
		defer srcAuthDB.Close()

		if tableExists(srcAuthDB, "users") {
			n, err := copyNodeUsers2SQLite(srcAuthDB, dstDB)
			if err != nil {
				return total, fmt.Errorf("users: %w", err)
			}
			log.Printf("  users: %d rows (from Node.js auth)", n)
			total += n
		}
	}

	return total, nil
}

func migrateNodeJS2PG(srcPeerPath, srcAuthPath, dstDSN string) (int, error) {
	ctx := context.Background()
	pool, err := pgxpool.New(ctx, dstDSN)
	if err != nil {
		return 0, fmt.Errorf("connect: %w", err)
	}
	defer pool.Close()

	for _, stmt := range pgSchemaStatements() {
		if _, err := pool.Exec(ctx, stmt); err != nil {
			return 0, fmt.Errorf("schema: %w", err)
		}
	}

	total := 0

	srcPeerDB, err := sql.Open("sqlite", fmt.Sprintf("file:%s?mode=ro", srcPeerPath))
	if err != nil {
		return 0, err
	}
	defer srcPeerDB.Close()

	if tableExists(srcPeerDB, "peer") {
		n, err := copyNodePeers2PG(ctx, srcPeerDB, pool)
		if err != nil {
			return total, fmt.Errorf("peers: %w", err)
		}
		log.Printf("  peers: %d rows (from Node.js peer table)", n)
		total += n
	}

	if srcAuthPath != "" {
		srcAuthDB, err := sql.Open("sqlite", fmt.Sprintf("file:%s?mode=ro", srcAuthPath))
		if err != nil {
			return total, err
		}
		defer srcAuthDB.Close()

		if tableExists(srcAuthDB, "users") {
			n, err := copyNodeUsers2PG(ctx, srcAuthDB, pool)
			if err != nil {
				return total, fmt.Errorf("users: %w", err)
			}
			log.Printf("  users: %d rows (from Node.js auth)", n)
			total += n
		}
	}

	return total, nil
}

// copyNodePeers2SQLite converts Node.js peer table → Go peers table (SQLite).
func copyNodePeers2SQLite(src, dst *sql.DB) (int, error) {
	cols := getColumns(src, "peer")
	hasIP := contains(cols, "ip")
	hasUser := contains(cols, "user")
	hasIsBanned := contains(cols, "is_banned")
	hasBannedAt := contains(cols, "banned_at")
	hasBannedReason := contains(cols, "banned_reason")
	hasStatusOnline := contains(cols, "status_online")
	hasLastOnline := contains(cols, "last_online")
	hasIsDeleted := contains(cols, "is_deleted")

	query := "SELECT id, uuid, pk, note, created_at, info"
	if hasStatusOnline {
		query += ", status_online"
	}
	if hasLastOnline {
		query += ", last_online"
	}
	if hasIsDeleted {
		query += ", is_deleted"
	}
	if hasIP {
		query += ", ip"
	}
	if hasUser {
		query += ", user"
	}
	if hasIsBanned {
		query += ", is_banned"
	}
	if hasBannedAt {
		query += ", banned_at"
	}
	if hasBannedReason {
		query += ", banned_reason"
	}
	query += " FROM peer"

	rows, err := src.Query(query)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	tx, err := dst.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()

	stmt, err := tx.Prepare(`INSERT OR REPLACE INTO peers
		(id, uuid, pk, ip, user, hostname, os, version, status, last_online, created_at,
		 banned, ban_reason, banned_at, soft_deleted, note)
		VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`)
	if err != nil {
		return 0, err
	}
	defer stmt.Close()

	count := 0
	for rows.Next() {
		var (
			id, uuid, note, createdAt, infoStr string
			pk                                 []byte
			statusOnline, isDeleted, isBanned  *int64
			lastOnline, ip, user               *string
			bannedAt, bannedReason             *string
		)

		scanArgs := []any{&id, &uuid, &pk, &note, &createdAt, &infoStr}
		if hasStatusOnline {
			scanArgs = append(scanArgs, &statusOnline)
		}
		if hasLastOnline {
			scanArgs = append(scanArgs, &lastOnline)
		}
		if hasIsDeleted {
			scanArgs = append(scanArgs, &isDeleted)
		}
		if hasIP {
			scanArgs = append(scanArgs, &ip)
		}
		if hasUser {
			scanArgs = append(scanArgs, &user)
		}
		if hasIsBanned {
			scanArgs = append(scanArgs, &isBanned)
		}
		if hasBannedAt {
			scanArgs = append(scanArgs, &bannedAt)
		}
		if hasBannedReason {
			scanArgs = append(scanArgs, &bannedReason)
		}

		if err := rows.Scan(scanArgs...); err != nil {
			log.Printf("WARN: skip Node.js peer: %v", err)
			continue
		}

		var info peerInfo
		if infoStr != "" {
			_ = json.Unmarshal([]byte(infoStr), &info)
		}

		status := "OFFLINE"
		if statusOnline != nil && *statusOnline == 1 {
			status = "ONLINE"
		}

		stmt.Exec(id, uuid, pk, ptrStr(ip), ptrStr(user),
			info.Hostname, info.OS, info.Version, status,
			ptrStr(lastOnline), createdAt,
			ptrIntBool(isBanned), ptrStr(bannedReason), ptrStr(bannedAt),
			ptrIntBool(isDeleted), note)
		count++
	}
	if err := rows.Err(); err != nil {
		return count, err
	}
	return count, tx.Commit()
}

// copyNodePeers2PG converts Node.js peer table → Go peers table (PostgreSQL).
func copyNodePeers2PG(ctx context.Context, src *sql.DB, dst *pgxpool.Pool) (int, error) {
	cols := getColumns(src, "peer")
	hasIP := contains(cols, "ip")
	hasUser := contains(cols, "user")
	hasIsBanned := contains(cols, "is_banned")
	hasBannedAt := contains(cols, "banned_at")
	hasBannedReason := contains(cols, "banned_reason")
	hasStatusOnline := contains(cols, "status_online")
	hasLastOnline := contains(cols, "last_online")
	hasIsDeleted := contains(cols, "is_deleted")

	query := "SELECT id, uuid, pk, note, created_at, info"
	if hasStatusOnline {
		query += ", status_online"
	}
	if hasLastOnline {
		query += ", last_online"
	}
	if hasIsDeleted {
		query += ", is_deleted"
	}
	if hasIP {
		query += ", ip"
	}
	if hasUser {
		query += ", user"
	}
	if hasIsBanned {
		query += ", is_banned"
	}
	if hasBannedAt {
		query += ", banned_at"
	}
	if hasBannedReason {
		query += ", banned_reason"
	}
	query += " FROM peer"

	rows, err := src.Query(query)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	count := 0
	for rows.Next() {
		var (
			id, uuid, note, createdAt, infoStr string
			pk                                 []byte
			statusOnline, isDeleted, isBanned  *int64
			lastOnline, ip, user               *string
			bannedAt, bannedReason             *string
		)

		scanArgs := []any{&id, &uuid, &pk, &note, &createdAt, &infoStr}
		if hasStatusOnline {
			scanArgs = append(scanArgs, &statusOnline)
		}
		if hasLastOnline {
			scanArgs = append(scanArgs, &lastOnline)
		}
		if hasIsDeleted {
			scanArgs = append(scanArgs, &isDeleted)
		}
		if hasIP {
			scanArgs = append(scanArgs, &ip)
		}
		if hasUser {
			scanArgs = append(scanArgs, &user)
		}
		if hasIsBanned {
			scanArgs = append(scanArgs, &isBanned)
		}
		if hasBannedAt {
			scanArgs = append(scanArgs, &bannedAt)
		}
		if hasBannedReason {
			scanArgs = append(scanArgs, &bannedReason)
		}

		if err := rows.Scan(scanArgs...); err != nil {
			log.Printf("WARN: skip Node.js peer: %v", err)
			continue
		}

		var info peerInfo
		if infoStr != "" {
			_ = json.Unmarshal([]byte(infoStr), &info)
		}

		status := "OFFLINE"
		if statusOnline != nil && *statusOnline == 1 {
			status = "ONLINE"
		}

		_, err := dst.Exec(ctx, `
			INSERT INTO peers (id, uuid, pk, ip, "user", hostname, os, version,
			    status, last_online, created_at, banned, ban_reason, banned_at,
			    soft_deleted, note)
			VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)
			ON CONFLICT (id) DO NOTHING`,
			id, uuid, pk, ptrStr(ip), ptrStr(user),
			info.Hostname, info.OS, info.Version, status,
			parseTime(ptrStr(lastOnline)), parseTime(createdAt),
			ptrIntBool(isBanned) != 0, ptrStr(bannedReason), parseTime(ptrStr(bannedAt)),
			ptrIntBool(isDeleted) != 0, note)
		if err != nil {
			log.Printf("WARN: Node.js peer %s: %v", id, err)
			continue
		}
		count++
	}
	return count, rows.Err()
}

// copyNodeUsers2SQLite converts Node.js users → Go users (SQLite).
func copyNodeUsers2SQLite(src, dst *sql.DB) (int, error) {
	cols := getColumns(src, "users")
	hasTotpSecret := contains(cols, "totp_secret")
	hasTotpEnabled := contains(cols, "totp_enabled")

	query := "SELECT username, password_hash, role, created_at, last_login"
	if hasTotpSecret {
		query += ", totp_secret"
	}
	if hasTotpEnabled {
		query += ", totp_enabled"
	}
	query += " FROM users"

	rows, err := src.Query(query)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	tx, err := dst.Begin()
	if err != nil {
		return 0, err
	}
	defer tx.Rollback()

	count := 0
	for rows.Next() {
		var (
			username, passwordHash, role string
			createdAt                    sql.NullString
			lastLogin                    sql.NullString
			totpSecret                   sql.NullString
			totpEnabled                  *int64
		)
		scanArgs := []any{&username, &passwordHash, &role, &createdAt, &lastLogin}
		if hasTotpSecret {
			scanArgs = append(scanArgs, &totpSecret)
		}
		if hasTotpEnabled {
			scanArgs = append(scanArgs, &totpEnabled)
		}
		if err := rows.Scan(scanArgs...); err != nil {
			log.Printf("WARN: skip Node.js user: %v", err)
			continue
		}

		secret := ""
		if totpSecret.Valid {
			secret = totpSecret.String
		}
		enabled := 0
		if totpEnabled != nil && *totpEnabled == 1 {
			enabled = 1
		}

		tx.Exec(`INSERT OR REPLACE INTO users
			(username, password_hash, role, totp_secret, totp_enabled, created_at, last_login)
			VALUES (?, ?, ?, ?, ?, ?, ?)`,
			username, passwordHash, role, secret, enabled,
			nullStr(createdAt), nullStr(lastLogin))
		count++
	}
	if err := rows.Err(); err != nil {
		return count, err
	}
	return count, tx.Commit()
}

// copyNodeUsers2PG converts Node.js users → Go users (PostgreSQL).
func copyNodeUsers2PG(ctx context.Context, src *sql.DB, dst *pgxpool.Pool) (int, error) {
	cols := getColumns(src, "users")
	hasTotpSecret := contains(cols, "totp_secret")
	hasTotpEnabled := contains(cols, "totp_enabled")

	query := "SELECT username, password_hash, role, created_at, last_login"
	if hasTotpSecret {
		query += ", totp_secret"
	}
	if hasTotpEnabled {
		query += ", totp_enabled"
	}
	query += " FROM users"

	rows, err := src.Query(query)
	if err != nil {
		return 0, err
	}
	defer rows.Close()

	count := 0
	for rows.Next() {
		var (
			username, passwordHash, role string
			createdAt                    sql.NullString
			lastLogin                    sql.NullString
			totpSecret                   sql.NullString
			totpEnabled                  *int64
		)
		scanArgs := []any{&username, &passwordHash, &role, &createdAt, &lastLogin}
		if hasTotpSecret {
			scanArgs = append(scanArgs, &totpSecret)
		}
		if hasTotpEnabled {
			scanArgs = append(scanArgs, &totpEnabled)
		}
		if err := rows.Scan(scanArgs...); err != nil {
			log.Printf("WARN: skip Node.js user: %v", err)
			continue
		}

		secret := ""
		if totpSecret.Valid {
			secret = totpSecret.String
		}
		enabled := totpEnabled != nil && *totpEnabled == 1

		_, err := dst.Exec(ctx, `
			INSERT INTO users (username, password_hash, role, totp_secret, totp_enabled,
			    created_at, last_login)
			VALUES ($1, $2, $3, $4, $5, $6, $7)
			ON CONFLICT (username) DO NOTHING`,
			username, passwordHash, role, secret, enabled,
			parseTimeDefault(nullStr(createdAt)), parseTime(nullStr(lastLogin)))
		if err != nil {
			log.Printf("WARN: Node.js user %s: %v", username, err)
			continue
		}
		count++
	}
	return count, rows.Err()
}

// ── Schema Definitions ────────────────────────────────────────────────

// pgSchemaStatements returns PostgreSQL DDL for all Yomie Go tables.
func pgSchemaStatements() []string {
	return []string{
		`CREATE TABLE IF NOT EXISTS peers (
			id            TEXT PRIMARY KEY,
			uuid          TEXT NOT NULL DEFAULT '',
			pk            BYTEA DEFAULT NULL,
			ip            TEXT NOT NULL DEFAULT '',
			"user"        TEXT NOT NULL DEFAULT '',
			hostname      TEXT NOT NULL DEFAULT '',
			os            TEXT NOT NULL DEFAULT '',
			version       TEXT NOT NULL DEFAULT '',
			status        TEXT NOT NULL DEFAULT 'OFFLINE',
			nat_type      INTEGER NOT NULL DEFAULT 0,
			last_online   TIMESTAMPTZ,
			created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
			disabled      BOOLEAN NOT NULL DEFAULT FALSE,
			banned        BOOLEAN NOT NULL DEFAULT FALSE,
			ban_reason    TEXT NOT NULL DEFAULT '',
			banned_at     TIMESTAMPTZ,
			soft_deleted  BOOLEAN NOT NULL DEFAULT FALSE,
			deleted_at    TIMESTAMPTZ,
			note          TEXT NOT NULL DEFAULT '',
			tags          TEXT NOT NULL DEFAULT '',
			heartbeat_seq BIGINT NOT NULL DEFAULT 0
		)`,
		`CREATE INDEX IF NOT EXISTS idx_peers_uuid ON peers(uuid)`,
		`CREATE INDEX IF NOT EXISTS idx_peers_status ON peers(status)`,
		`CREATE INDEX IF NOT EXISTS idx_peers_banned ON peers(banned) WHERE banned = TRUE`,

		`CREATE TABLE IF NOT EXISTS server_config (
			key   TEXT PRIMARY KEY,
			value TEXT NOT NULL DEFAULT ''
		)`,

		`CREATE TABLE IF NOT EXISTS id_change_history (
			id         BIGSERIAL PRIMARY KEY,
			old_id     TEXT NOT NULL,
			new_id     TEXT NOT NULL,
			changed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
			reason     TEXT NOT NULL DEFAULT ''
		)`,
		`CREATE INDEX IF NOT EXISTS idx_id_history_old ON id_change_history(old_id)`,
		`CREATE INDEX IF NOT EXISTS idx_id_history_new ON id_change_history(new_id)`,

		`CREATE TABLE IF NOT EXISTS users (
			id            BIGSERIAL PRIMARY KEY,
			username      TEXT UNIQUE NOT NULL,
			password_hash TEXT NOT NULL,
			role          TEXT NOT NULL DEFAULT 'viewer',
			totp_secret   TEXT NOT NULL DEFAULT '',
			totp_enabled  BOOLEAN NOT NULL DEFAULT FALSE,
			created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
			last_login    TIMESTAMPTZ
		)`,

		`CREATE TABLE IF NOT EXISTS api_keys (
			id         BIGSERIAL PRIMARY KEY,
			key_hash   TEXT UNIQUE NOT NULL,
			key_prefix TEXT NOT NULL DEFAULT '',
			name       TEXT NOT NULL DEFAULT '',
			role       TEXT NOT NULL DEFAULT 'viewer',
			created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
			expires_at TIMESTAMPTZ,
			last_used  TIMESTAMPTZ
		)`,
	}
}

// sqliteSchemaStatements returns SQLite DDL for all Yomie Go tables.
func sqliteSchemaStatements() []string {
	return []string{
		`CREATE TABLE IF NOT EXISTS peers (
			id TEXT PRIMARY KEY,
			uuid TEXT DEFAULT '',
			pk BLOB DEFAULT NULL,
			ip TEXT DEFAULT '',
			user TEXT DEFAULT '',
			hostname TEXT DEFAULT '',
			os TEXT DEFAULT '',
			version TEXT DEFAULT '',
			status TEXT DEFAULT 'OFFLINE',
			nat_type INTEGER DEFAULT 0,
			last_online TEXT DEFAULT '',
			created_at TEXT DEFAULT (datetime('now')),
			disabled INTEGER DEFAULT 0,
			banned INTEGER DEFAULT 0,
			ban_reason TEXT DEFAULT '',
			banned_at TEXT DEFAULT NULL,
			soft_deleted INTEGER DEFAULT 0,
			deleted_at TEXT DEFAULT NULL,
			note TEXT DEFAULT '',
			tags TEXT DEFAULT '',
			heartbeat_seq INTEGER DEFAULT 0
		)`,
		`CREATE INDEX IF NOT EXISTS idx_peers_uuid ON peers(uuid)`,
		`CREATE INDEX IF NOT EXISTS idx_peers_status ON peers(status)`,
		`CREATE INDEX IF NOT EXISTS idx_peers_banned ON peers(banned)`,

		`CREATE TABLE IF NOT EXISTS server_config (
			key TEXT PRIMARY KEY,
			value TEXT DEFAULT ''
		)`,

		`CREATE TABLE IF NOT EXISTS id_change_history (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			old_id TEXT NOT NULL,
			new_id TEXT NOT NULL,
			changed_at TEXT DEFAULT (datetime('now')),
			reason TEXT DEFAULT ''
		)`,
		`CREATE INDEX IF NOT EXISTS idx_id_history_old ON id_change_history(old_id)`,
		`CREATE INDEX IF NOT EXISTS idx_id_history_new ON id_change_history(new_id)`,

		`CREATE TABLE IF NOT EXISTS users (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			username TEXT UNIQUE NOT NULL,
			password_hash TEXT NOT NULL,
			role TEXT NOT NULL DEFAULT 'viewer',
			totp_secret TEXT DEFAULT '',
			totp_enabled INTEGER DEFAULT 0,
			created_at TEXT DEFAULT (datetime('now')),
			last_login TEXT DEFAULT ''
		)`,
		`CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username ON users(username)`,

		`CREATE TABLE IF NOT EXISTS api_keys (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			key_hash TEXT UNIQUE NOT NULL,
			key_prefix TEXT NOT NULL DEFAULT '',
			name TEXT NOT NULL DEFAULT '',
			role TEXT NOT NULL DEFAULT 'viewer',
			created_at TEXT DEFAULT (datetime('now')),
			expires_at TEXT DEFAULT '',
			last_used TEXT DEFAULT ''
		)`,
		`CREATE UNIQUE INDEX IF NOT EXISTS idx_api_keys_hash ON api_keys(key_hash)`,
	}
}

// ── Backup & Utilities ────────────────────────────────────────────────

// createBackup copies a SQLite database file with a timestamp suffix.
func createBackup(srcPath string) string {
	ts := time.Now().Format("20060102_150405")
	ext := filepath.Ext(srcPath)
	base := srcPath[:len(srcPath)-len(ext)]
	backupPath := fmt.Sprintf("%s_backup_%s%s", base, ts, ext)

	src, err := os.Open(srcPath)
	if err != nil {
		log.Fatalf("Cannot open source for backup: %v", err)
	}
	defer src.Close()

	dst, err := os.Create(backupPath)
	if err != nil {
		log.Fatalf("Cannot create backup file: %v", err)
	}
	defer dst.Close()

	if _, err := io.Copy(dst, src); err != nil {
		log.Fatalf("Backup copy failed: %v", err)
	}

	// Also copy -wal and -shm if they exist
	for _, suffix := range []string{"-wal", "-shm"} {
		walPath := srcPath + suffix
		if _, err := os.Stat(walPath); err == nil {
			walSrc, err := os.Open(walPath)
			if err == nil {
				walDst, err := os.Create(backupPath + suffix)
				if err == nil {
					io.Copy(walDst, walSrc)
					walDst.Close()
				}
				walSrc.Close()
			}
		}
	}

	return backupPath
}

// requireFile verifies that a local file exists.
func requireFile(path string) {
	if _, err := os.Stat(path); os.IsNotExist(err) {
		log.Fatalf("File not found: %s", path)
	}
}

// detectSQLiteSchema returns the detected schema type of a SQLite database.
func detectSQLiteSchema(db *sql.DB) string {
	if tableExists(db, "peers") {
		return "yomie-go"
	}
	if tableExists(db, "peer") {
		cols := getColumns(db, "peer")
		if contains(cols, "status_online") {
			return "nodejs-console"
		}
		if contains(cols, "guid") {
			return "rust-hbbs"
		}
		return "unknown-peer"
	}
	return "unknown"
}

// tableExists checks if a table exists in a SQLite database.
func tableExists(db *sql.DB, name string) bool {
	var n string
	err := db.QueryRow("SELECT name FROM sqlite_master WHERE type='table' AND name=?", name).Scan(&n)
	return err == nil
}

// safeIdentifier validates that a string is a safe SQL identifier (letters, digits, underscores).
func safeIdentifier(name string) bool {
	if len(name) == 0 || len(name) > 64 {
		return false
	}
	for _, c := range name {
		if !((c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_') {
			return false
		}
	}
	return true
}

// getColumns returns column names for a table.
func getColumns(db *sql.DB, table string) []string {
	if !safeIdentifier(table) {
		return nil
	}
	rows, err := db.Query(fmt.Sprintf("PRAGMA table_info(%s)", table))
	if err != nil {
		return nil
	}
	defer rows.Close()
	var cols []string
	for rows.Next() {
		var cid int
		var name, typ string
		var notnull int
		var dflt *string
		var pk int
		if err := rows.Scan(&cid, &name, &typ, &notnull, &dflt, &pk); err != nil {
			continue
		}
		cols = append(cols, name)
	}
	return cols
}

func contains(s []string, e string) bool {
	for _, a := range s {
		if a == e {
			return true
		}
	}
	return false
}

// ── Time Conversion Helpers ───────────────────────────────────────────

// parseTime converts a SQLite TEXT timestamp to *time.Time for PostgreSQL.
// Returns nil for empty strings (maps to NULL in PG).
func parseTime(s string) *time.Time {
	if s == "" {
		return nil
	}
	for _, layout := range []string{
		"2006-01-02 15:04:05",
		time.RFC3339,
		"2006-01-02T15:04:05Z",
		"2006-01-02",
	} {
		if t, err := time.Parse(layout, s); err == nil {
			return &t
		}
	}
	return nil
}

// parseTimeDefault converts a SQLite TEXT to time.Time, defaulting to now().
func parseTimeDefault(s string) time.Time {
	t := parseTime(s)
	if t == nil {
		return time.Now()
	}
	return *t
}

// parseNullTime converts a sql.NullString timestamp to *time.Time.
func parseNullTime(ns sql.NullString) *time.Time {
	if !ns.Valid || ns.String == "" {
		return nil
	}
	return parseTime(ns.String)
}

// formatTime converts a *time.Time to SQLite TEXT format.
func formatTime(t *time.Time) string {
	if t == nil {
		return ""
	}
	return t.Format("2006-01-02 15:04:05")
}

// formatTimeDefault converts a *time.Time to SQLite TEXT, defaulting to now().
func formatTimeDefault(t *time.Time) string {
	if t == nil {
		return time.Now().Format("2006-01-02 15:04:05")
	}
	return t.Format("2006-01-02 15:04:05")
}

// boolToInt converts a bool to 0/1 for SQLite storage.
func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
}

// ── Nullable Pointer Helpers ──────────────────────────────────────────

func ptrStr(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

func ptrIntBool(i *int64) int {
	if i != nil && *i == 1 {
		return 1
	}
	return 0
}

func nullStr(ns sql.NullString) string {
	if ns.Valid {
		return ns.String
	}
	return ""
}

// Compile-time check: ensure pgx import is used (via ErrNoRows reference).
var _ = pgx.ErrNoRows
