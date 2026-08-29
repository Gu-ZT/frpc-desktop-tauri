//! SQLite connection manager (ported from electron/database/DatabaseManager.ts).
//!
//! Uses rusqlite with the bundled SQLite. Migrations are embedded at compile
//! time from `src-tauri/migrations/*.sql` (copied verbatim from the Electron
//! repo) so no runtime resource path handling is needed.
//!
//! The connection is shared across threads through `Arc<Mutex<Connection>>`
//! (rusqlite Connection is Send but not Sync).

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

use crate::core::logger::Logger;
use crate::core::paths::PathUtils;

pub type SharedDb = Arc<Mutex<Connection>>;

/// Build a rusqlite error carrying a custom message (rusqlite has no
/// user-message variant that is a plain constructor).
pub fn sqlite_error(msg: &str) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::ErrorCode::Unknown as i32),
        Some(msg.to_string()),
    )
}

pub struct DatabaseManager {
    database_path: PathBuf,
    migrations_path: PathBuf,
}

impl Default for DatabaseManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DatabaseManager {
    pub fn new() -> Self {
        Self {
            database_path: PathUtils::get_database_file_path(),
            migrations_path: PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")),
        }
    }
    /// Open the database, run migrations and return a shared handle.
    pub fn initialize(&self) -> Result<SharedDb, rusqlite::Error> {
        let db = Connection::open(&self.database_path)?;
        db.pragma_update(None, "foreign_keys", "ON")?;
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "NORMAL")?;
        db.pragma_update(None, "busy_timeout", "5000")?;
        self.run_migrations(&db)?;
        self.verify_database(&db)?;
        Logger::info("DatabaseManager", "SQLite database initialized.");
        Ok(Arc::new(Mutex::new(db)))
    }

    fn run_migrations(&self, db: &Connection) -> Result<(), rusqlite::Error> {
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS t_frpcd_schema_migrations (
                version INTEGER CONSTRAINT pk_t_frpcd_schema_migrations PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL
            )",
        )?;

        let migrations = self.load_migrations()?;

        // Validate the applied history is a prefix of the available migrations.
        let mut stmt =
            db.prepare("SELECT version, name FROM t_frpcd_schema_migrations ORDER BY version")?;
        let applied: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<_, _>>()?;
        drop(stmt);

        for (index, (version, name)) in applied.iter().enumerate() {
            let file = migrations.iter().find(|m| m.0 == *version).ok_or_else(|| {
                sqlite_error(&format!(
                    "Applied migration {version} is missing from the application."
                ))
            })?;
            if let Some(expected) = migrations.get(index) {
                if expected.0 != *version {
                    return Err(sqlite_error(
                        "SQLite migration history is not a valid prefix.",
                    ));
                }
            }
            if file.1 != *name {
                return Err(sqlite_error(&format!(
                    "Applied migration {version} name does not match {}.",
                    file.2
                )));
            }
        }

        let applied_versions: std::collections::HashSet<i64> =
            applied.iter().map(|(v, _)| *v).collect();

        for (version, name, filename, sql) in &migrations {
            if applied_versions.contains(version) {
                continue;
            }
            let tx = db.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute(
                "INSERT INTO t_frpcd_schema_migrations (version, name, applied_at)
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![version, name, chrono::Utc::now().to_rfc3339()],
            )?;
            tx.commit()?;
            Logger::info(
                "DatabaseManager",
                &format!("Applied SQLite migration {filename}."),
            );
        }
        Ok(())
    }

    /// Load migration files from the migrations directory, sorted by version.
    fn load_migrations(&self) -> Result<Vec<(i64, String, String, String)>, rusqlite::Error> {
        let entries = fs::read_dir(&self.migrations_path)
            .map_err(|e| sqlite_error(&format!("No migrations dir: {e}")))?;
        let mut migrations: Vec<(i64, String, String, String)> = Vec::new();
        for entry in entries.flatten() {
            let filename = entry.file_name().to_string_lossy().to_string();
            if !filename.ends_with(".sql") {
                continue;
            }
            // Expect NNN_name.sql
            let Some(stem) = filename.strip_suffix(".sql") else {
                continue;
            };
            let (version_str, name) = stem.split_once('_').ok_or_else(|| {
                sqlite_error(&format!("Invalid SQLite migration filename: {filename}."))
            })?;
            let version: i64 = version_str.parse().map_err(|_| {
                sqlite_error(&format!("Invalid SQLite migration version in {filename}."))
            })?;
            if version < 1 {
                return Err(sqlite_error(&format!(
                    "Invalid SQLite migration version in {filename}."
                )));
            }
            let sql = fs::read_to_string(entry.path())
                .map_err(|e| sqlite_error(&format!("Cannot read {filename}: {e}")))?;
            migrations.push((version, name.to_string(), filename, sql));
        }
        migrations.sort_by_key(|m| m.0);
        if migrations.is_empty() {
            return Err(sqlite_error(&format!(
                "No SQLite migrations found in {}.",
                self.migrations_path.display()
            )));
        }
        Ok(migrations)
    }

    fn verify_database(&self, db: &Connection) -> Result<(), rusqlite::Error> {
        let fk: i64 = db.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
        if fk != 1 {
            return Err(sqlite_error(
                "SQLite foreign key enforcement is not enabled.",
            ));
        }
        let fk_errors: i64 = db
            .prepare("PRAGMA foreign_key_check")?
            .query([])?
            .next()?
            .map_or(0, |_| 1);
        if fk_errors != 0 {
            return Err(sqlite_error(
                "SQLite foreign key check failed after migration.",
            ));
        }
        let integrity: String = db.pragma_query_value(None, "integrity_check", |row| row.get(0))?;
        if integrity != "ok" {
            return Err(sqlite_error(
                "SQLite integrity check failed after migration.",
            ));
        }
        Ok(())
    }

    /// Reset all business data in a single transaction (resetAllConfig).
    pub fn reset_data(db: &SharedDb) -> Result<(), rusqlite::Error> {
        let conn = db.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM t_frpcd_proxies", [])?;
        tx.execute("DELETE FROM t_frpcd_servers", [])?;
        tx.execute("DELETE FROM t_frpcd_versions", [])?;
        tx.execute(
            "DELETE FROM t_frpcd_app_config WHERE namespace <> 'migration'",
            [],
        )?;
        tx.commit()
    }

    /// WAL checkpoint (called on app exit; the connection lives as long as
    /// the process, so we only force a checkpoint).
    pub fn close(db: &SharedDb) {
        let conn = db.lock().unwrap();
        let _ = conn.pragma_update(None, "wal_checkpoint", "TRUNCATE");
    }
}

/// Embed the initial schema for tests and fallback use.
pub const INITIAL_SCHEMA_SQL: &str = include_str!("../../migrations/001_initial_schema.sql");

/// Create an in-memory database with migrations applied (used by tests).
pub fn open_in_memory() -> Result<SharedDb, rusqlite::Error> {
    let db = Connection::open_in_memory()?;
    db.pragma_update(None, "foreign_keys", "ON")?;
    db.execute_batch(INITIAL_SCHEMA_SQL)?;
    Ok(Arc::new(Mutex::new(db)))
}
