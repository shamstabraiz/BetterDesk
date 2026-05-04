// Package db — convenience constructors.
package db

import "strings"

// Open opens a database at the given DSN.
//
// If dsn starts with "postgres://" or "postgresql://" the PostgreSQL driver
// is used (pgx/v5). Otherwise the dsn is treated as a local file path and
// the SQLite driver is used.
//
// Examples:
//
//	db.Open("db_v2.sqlite3")                            // SQLite (default)
//	db.Open("postgres://user:pass@localhost/yomie") // PostgreSQL
func Open(dsn string) (Database, error) {
	lower := strings.ToLower(dsn)
	if strings.HasPrefix(lower, "postgres://") || strings.HasPrefix(lower, "postgresql://") {
		return OpenPostgres(dsn)
	}
	return OpenSQLite(dsn)
}
