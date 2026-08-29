//! Server config repository (ported from electron/repository/ServerRepository.ts).

use rusqlite::{params, Connection, OptionalExtension};

use crate::db::app_config_repository::AppConfigRepository;
use crate::db::database_manager::{sqlite_error, SharedDb};
use crate::model::frp::OpenSourceFrpcDesktopServer;

#[derive(Clone)]
pub struct ServerRepository {
    db: SharedDb,
    app_config: AppConfigRepository,
}

impl ServerRepository {
    pub fn new(db: SharedDb, app_config: AppConfigRepository) -> Self {
        Self { db, app_config }
    }

    pub fn conn(&self) -> &SharedDb {
        &self.db
    }

    /// The canonical upsert used both by insert/update and NeDB migration.
    pub fn upsert_for_migration(
        &self,
        server: &OpenSourceFrpcDesktopServer,
    ) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        Self::upsert_with_conn(&conn, server)?;
        drop(conn);
        self.app_config.save_system_config(&server.system)
    }

    /// Upsert using an already-locked connection (used inside a caller-held
    /// transaction, e.g. the NeDB migration, to avoid recursive locking).
    pub fn upsert_with_conn(
        conn: &Connection,
        server: &OpenSourceFrpcDesktopServer,
    ) -> Result<(), rusqlite::Error> {
        let mut server = server.clone();
        server.id = "1".to_string();
        conn.execute(
            "INSERT INTO t_frpcd_servers (
               id, frpc_version, multiuser, user, server_addr, server_port,
               login_fail_exit, udp_packet_size, auth_json, log_json,
               web_server_json, transport_json, metadatas_json
             ) VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13
             )
             ON CONFLICT(id) DO UPDATE SET
               frpc_version = excluded.frpc_version,
               multiuser = excluded.multiuser,
               user = excluded.user,
               server_addr = excluded.server_addr,
               server_port = excluded.server_port,
               login_fail_exit = excluded.login_fail_exit,
               udp_packet_size = excluded.udp_packet_size,
               auth_json = excluded.auth_json,
               log_json = excluded.log_json,
               web_server_json = excluded.web_server_json,
               transport_json = excluded.transport_json,
               metadatas_json = excluded.metadatas_json",
            params![
                "1",
                server.frpc_version,
                server.multiuser as i64,
                server.user,
                server.server_addr,
                server.server_port,
                server.login_fail_exit as i64,
                server.udp_packet_size,
                serde_json::to_string(&server.auth).unwrap_or_else(|_| "{}".into()),
                serde_json::to_string(&server.log).unwrap_or_else(|_| "{}".into()),
                serde_json::to_string(&server.web_server).unwrap_or_else(|_| "{}".into()),
                serde_json::to_string(&server.transport).unwrap_or_else(|_| "{}".into()),
                serde_json::to_string(&server.metadatas).unwrap_or_else(|_| "{}".into()),
            ],
        )?;
        Ok(())
    }

    pub fn update_by_id(
        &self,
        id: &str,
        server: &OpenSourceFrpcDesktopServer,
    ) -> Result<OpenSourceFrpcDesktopServer, rusqlite::Error> {
        if id != "1" {
            return Err(sqlite_error("Only server id 1 is supported."));
        }
        self.upsert_for_migration(server)?;
        Ok(server.clone())
    }

    pub fn find_by_id(
        &self,
        id: &str,
    ) -> Result<Option<OpenSourceFrpcDesktopServer>, rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let row = conn
            .query_row(
                "SELECT * FROM t_frpcd_servers WHERE id = ?1",
                params![id],
                row_to_server_fields,
            )
            .optional()?;
        drop(conn);
        let Some(fields) = row else {
            return Ok(None);
        };
        let system = self.app_config.get_system_config()?;
        Ok(Some(fields.into_server(system)))
    }

    pub fn exists(&self, id: &str) -> Result<bool, rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let found: i64 = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM t_frpcd_servers WHERE id = ?1)",
            params![id],
            |row| row.get(0),
        )?;
        Ok(found == 1)
    }

    pub fn truncate(&self) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM t_frpcd_servers", [])?;
        tx.execute(
            "DELETE FROM t_frpcd_app_config WHERE namespace = 'desktop'",
            [],
        )?;
        tx.commit()
    }
}

/// Raw server row fields (kept in a plain struct to avoid borrowing issues).
struct ServerRowFields {
    frpc_version: Option<i64>,
    multiuser: bool,
    user: String,
    server_addr: String,
    server_port: i64,
    login_fail_exit: bool,
    udp_packet_size: i64,
    auth_json: String,
    log_json: String,
    web_server_json: String,
    transport_json: String,
    metadatas_json: String,
}

impl ServerRowFields {
    fn into_server(
        self,
        system: crate::model::frp::FrpcSystemConfiguration,
    ) -> OpenSourceFrpcDesktopServer {
        OpenSourceFrpcDesktopServer {
            id: "1".to_string(),
            frpc_version: self.frpc_version,
            multiuser: self.multiuser,
            user: self.user,
            server_addr: self.server_addr,
            server_port: self.server_port,
            login_fail_exit: self.login_fail_exit,
            udp_packet_size: self.udp_packet_size,
            auth: serde_json::from_str(&self.auth_json).unwrap_or_default(),
            log: serde_json::from_str(&self.log_json).unwrap_or_default(),
            web_server: serde_json::from_str(&self.web_server_json).unwrap_or_default(),
            transport: serde_json::from_str(&self.transport_json).unwrap_or_default(),
            metadatas: serde_json::from_str(&self.metadatas_json)
                .unwrap_or_else(|_| serde_json::json!({})),
            system,
        }
    }
}

fn row_to_server_fields(row: &rusqlite::Row) -> rusqlite::Result<ServerRowFields> {
    Ok(ServerRowFields {
        frpc_version: row.get::<_, Option<i64>>("frpc_version")?,
        multiuser: row.get::<_, i64>("multiuser")? != 0,
        user: row.get::<_, String>("user")?,
        server_addr: row.get::<_, String>("server_addr")?,
        server_port: row.get::<_, i64>("server_port")?,
        login_fail_exit: row.get::<_, i64>("login_fail_exit")? != 0,
        udp_packet_size: row.get::<_, i64>("udp_packet_size")?,
        auth_json: row.get::<_, String>("auth_json")?,
        log_json: row.get::<_, String>("log_json")?,
        web_server_json: row.get::<_, String>("web_server_json")?,
        transport_json: row.get::<_, String>("transport_json")?,
        metadatas_json: row.get::<_, String>("metadatas_json")?,
    })
}
