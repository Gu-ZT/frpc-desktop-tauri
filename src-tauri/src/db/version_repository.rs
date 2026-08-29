//! Frpc version repository (ported from electron/repository/VersionRepository.ts).

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::database_manager::SharedDb;
use crate::model::frp::FrpcVersion;
use crate::util::id_utils::IdUtils;

#[derive(Clone)]
pub struct VersionRepository {
    db: SharedDb,
}

impl VersionRepository {
    pub fn new(db: SharedDb) -> Self {
        Self { db }
    }

    pub fn conn(&self) -> &SharedDb {
        &self.db
    }

    pub fn upsert_for_migration(&self, version: &FrpcVersion) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        Self::upsert_with_conn(&conn, version)
    }

    /// Upsert with an already-locked connection (caller-held transaction).
    pub fn upsert_with_conn(
        conn: &Connection,
        version: &FrpcVersion,
    ) -> Result<(), rusqlite::Error> {
        conn.execute(
            "INSERT INTO t_frpcd_versions (
               id, github_release_id, github_asset_id, github_created_at,
               name, asset_name, version_download_count, asset_download_count,
               browser_download_url, downloaded, local_path, size
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT(id) DO UPDATE SET
               github_release_id = excluded.github_release_id,
               github_asset_id = excluded.github_asset_id,
               github_created_at = excluded.github_created_at,
               name = excluded.name,
               asset_name = excluded.asset_name,
               version_download_count = excluded.version_download_count,
               asset_download_count = excluded.asset_download_count,
               browser_download_url = excluded.browser_download_url,
               downloaded = excluded.downloaded,
               local_path = excluded.local_path,
               size = excluded.size",
            params![
                version.id,
                version.github_release_id,
                version.github_asset_id,
                version.github_created_at,
                version.name,
                version.asset_name,
                version.version_download_count,
                version.asset_download_count,
                version.browser_download_url,
                version.downloaded as i64,
                version.local_path,
                version.size,
            ],
        )?;
        Ok(())
    }

    pub fn insert(&self, version: &mut FrpcVersion) -> Result<FrpcVersion, rusqlite::Error> {
        version.id = IdUtils::gen_uuid();
        self.upsert_for_migration(version)?;
        Ok(version.clone())
    }

    pub fn find_by_github_release_id(
        &self,
        github_release_id: i64,
    ) -> Result<Option<FrpcVersion>, rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT * FROM t_frpcd_versions WHERE github_release_id = ?1",
                params![github_release_id],
                row_to_version,
            )
            .optional()?;
        Ok(row)
    }

    pub fn exists(&self, github_release_id: i64) -> Result<bool, rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let found: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM t_frpcd_versions WHERE github_release_id = ?1)",
            params![github_release_id],
            |row| row.get(0),
        )?;
        Ok(found == 1)
    }

    pub fn delete_by_id(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        conn.execute("DELETE FROM t_frpcd_versions WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn find_all(&self) -> Result<Vec<FrpcVersion>, rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM t_frpcd_versions ORDER BY rowid")?;
        let rows = stmt.query_map([], row_to_version)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

pub fn row_to_version(row: &rusqlite::Row) -> rusqlite::Result<FrpcVersion> {
    Ok(FrpcVersion {
        id: row.get("id")?,
        github_release_id: row.get("github_release_id")?,
        github_asset_id: row.get("github_asset_id")?,
        github_created_at: row.get("github_created_at")?,
        name: row.get("name")?,
        asset_name: row.get("asset_name")?,
        version_download_count: row.get("version_download_count")?,
        asset_download_count: row.get("asset_download_count")?,
        browser_download_url: row.get("browser_download_url")?,
        downloaded: row.get::<_, i64>("downloaded")? != 0,
        local_path: row.get("local_path")?,
        size: row.get("size")?,
    })
}
