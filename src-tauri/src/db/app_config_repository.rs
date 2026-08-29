//! Desktop application config repository (ported from
//! electron/repository/AppConfigRepository.ts).

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::database_manager::SharedDb;
use crate::model::frp::FrpcSystemConfiguration;
use crate::util::id_utils::IdUtils;

#[derive(Clone)]
pub struct AppConfigRepository {
    db: SharedDb,
}

impl AppConfigRepository {
    pub fn new(db: SharedDb) -> Self {
        Self { db }
    }

    pub fn conn(&self) -> &SharedDb {
        &self.db
    }

    pub fn get_system_config(&self) -> Result<FrpcSystemConfiguration, rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT config_key, config_value
             FROM t_frpcd_app_config
             WHERE scope_type = 'global'
               AND scope_id IS NULL
               AND namespace = 'desktop'
               AND deleted_at IS NULL",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut values = std::collections::HashMap::new();
        for row in rows {
            let (key, value) = row?;
            values.insert(key, value);
        }
        Ok(FrpcSystemConfiguration {
            launch_at_startup: read_boolean(
                values.get("launch_at_startup").map(|s| s.as_str()),
                false,
            ),
            silent_startup: read_boolean(values.get("silent_startup").map(|s| s.as_str()), false),
            auto_connect_on_startup: read_boolean(
                values.get("auto_connect_on_startup").map(|s| s.as_str()),
                false,
            ),
            language: values
                .get("language")
                .cloned()
                .unwrap_or_else(|| "en-US".to_string()),
        })
    }

    pub fn save_system_config(
        &self,
        system: &FrpcSystemConfiguration,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        Self::save_system_config_with_conn(&conn, system)
    }

    /// Same as [`save_system_config`] but with an already-locked connection
    /// (used inside caller-held transactions to avoid recursive locking).
    pub fn save_system_config_with_conn(
        conn: &Connection,
        system: &FrpcSystemConfiguration,
    ) -> Result<(), rusqlite::Error> {
        Self::upsert_with_conn(
            conn,
            "desktop",
            "launch_at_startup",
            "boolean",
            &system.launch_at_startup.to_string(),
        )?;
        Self::upsert_with_conn(
            conn,
            "desktop",
            "silent_startup",
            "boolean",
            &system.silent_startup.to_string(),
        )?;
        Self::upsert_with_conn(
            conn,
            "desktop",
            "auto_connect_on_startup",
            "boolean",
            &system.auto_connect_on_startup.to_string(),
        )?;
        Self::upsert_with_conn(conn, "desktop", "language", "string", &system.language)
    }

    pub fn has_nedb_migration_marker(&self) -> Result<bool, rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let found: i64 = conn.query_row(
            "SELECT EXISTS(
                SELECT 1
                FROM t_frpcd_app_config
                WHERE scope_type = 'global'
                  AND scope_id IS NULL
                  AND namespace = 'migration'
                  AND config_key = 'nedb_v2_imported'
                  AND config_value = 'true'
                  AND deleted_at IS NULL
             )",
            [],
            |row| row.get(0),
        )?;
        Ok(found == 1)
    }

    pub fn save_nedb_migration_marker(&self) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        Self::upsert_with_conn(&conn, "migration", "nedb_v2_imported", "boolean", "true")
    }

    /// Same as [`save_nedb_migration_marker`] with an already-locked connection.
    pub fn save_nedb_migration_marker_with_conn(conn: &Connection) -> Result<(), rusqlite::Error> {
        Self::upsert_with_conn(conn, "migration", "nedb_v2_imported", "boolean", "true")
    }

    pub fn delete_all(&self) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        conn.execute("DELETE FROM t_frpcd_app_config", [])?;
        Ok(())
    }

    /// Insert-or-update a config row using an already-locked connection.
    fn upsert_with_conn(
        conn: &Connection,
        namespace: &str,
        key: &str,
        value_type: &str,
        value: &str,
    ) -> Result<(), rusqlite::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        let existing: Option<String> = conn
            .query_row(
                "SELECT id
                 FROM t_frpcd_app_config
                 WHERE scope_type = 'global'
                   AND scope_id IS NULL
                   AND namespace = ?1
                   AND config_key = ?2
                   AND deleted_at IS NULL",
                params![namespace, key],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = existing {
            conn.execute(
                "UPDATE t_frpcd_app_config
                 SET value_type = ?1,
                     config_value = ?2,
                     version = version + 1,
                     updated_at = ?3
                 WHERE id = ?4",
                params![value_type, value, now, id],
            )?;
            return Ok(());
        }
        conn.execute(
            "INSERT INTO t_frpcd_app_config (
               id, scope_type, scope_id, namespace, config_key,
               value_type, config_value, is_secret, encryption_type,
               version, created_at, updated_at, deleted_at
             ) VALUES (?1, 'global', NULL, ?2, ?3, ?4, ?5, 0, NULL, 1, ?6, ?7, NULL)",
            params![
                IdUtils::gen_uuid(),
                namespace,
                key,
                value_type,
                value,
                now,
                now
            ],
        )?;
        Ok(())
    }
}

fn read_boolean(value: Option<&str>, fallback: bool) -> bool {
    match value {
        Some("true") => true,
        Some("false") => false,
        _ => fallback,
    }
}
