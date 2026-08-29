//! Proxy repository (ported from electron/repository/ProxyRepository.ts).

use rusqlite::{params, Connection};

use crate::db::database_manager::SharedDb;
use crate::model::frp::FrpcProxy;
use crate::util::id_utils::IdUtils;

#[derive(Clone)]
pub struct ProxyRepository {
    db: SharedDb,
}

impl ProxyRepository {
    pub fn new(db: SharedDb) -> Self {
        Self { db }
    }

    pub fn conn(&self) -> &SharedDb {
        &self.db
    }

    /// Upsert a proxy row (canonical, used by CRUD and NeDB migration).
    pub fn upsert_for_migration(&self, proxy: &FrpcProxy) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        Self::upsert_with_conn(&conn, proxy)
    }

    /// Upsert with an already-locked connection (caller-held transaction).
    pub fn upsert_with_conn(conn: &Connection, proxy: &FrpcProxy) -> Result<(), rusqlite::Error> {
        conn.execute(
            "INSERT INTO t_frpcd_proxies (
               id, server_id, name, type, local_ip, local_port, remote_port,
               custom_domains_json, locations_json, host_header_rewrite,
               visitors_model, server_user, server_name, secret_key, bind_addr,
               bind_port, subdomain, basic_auth, http_user, http_password,
               fallback_to, fallback_timeout_ms, https2http, https2http_ca_file,
               https2http_key_file, keep_tunnel_open, status, transport_json
             ) VALUES (
               ?1, '1', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
               ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27
             )
             ON CONFLICT(id) DO UPDATE SET
               server_id = excluded.server_id,
               name = excluded.name,
               type = excluded.type,
               local_ip = excluded.local_ip,
               local_port = excluded.local_port,
               remote_port = excluded.remote_port,
               custom_domains_json = excluded.custom_domains_json,
               locations_json = excluded.locations_json,
               host_header_rewrite = excluded.host_header_rewrite,
               visitors_model = excluded.visitors_model,
               server_user = excluded.server_user,
               server_name = excluded.server_name,
               secret_key = excluded.secret_key,
               bind_addr = excluded.bind_addr,
               bind_port = excluded.bind_port,
               subdomain = excluded.subdomain,
               basic_auth = excluded.basic_auth,
               http_user = excluded.http_user,
               http_password = excluded.http_password,
               fallback_to = excluded.fallback_to,
               fallback_timeout_ms = excluded.fallback_timeout_ms,
               https2http = excluded.https2http,
               https2http_ca_file = excluded.https2http_ca_file,
               https2http_key_file = excluded.https2http_key_file,
               keep_tunnel_open = excluded.keep_tunnel_open,
               status = excluded.status,
               transport_json = excluded.transport_json",
            params![
                proxy.id,
                proxy.name,
                proxy.proxy_type,
                proxy.local_ip,
                proxy.local_port,
                proxy.remote_port,
                serde_json::to_string(&proxy.custom_domains).unwrap_or_else(|_| "[\"\"]".into()),
                serde_json::to_string(&proxy.locations).unwrap_or_else(|_| "[\"\"]".into()),
                proxy.host_header_rewrite,
                proxy.visitors_model,
                proxy.server_user,
                proxy.server_name,
                proxy.secret_key,
                proxy.bind_addr,
                proxy.bind_port,
                proxy.subdomain,
                proxy.basic_auth as i64,
                proxy.http_user,
                proxy.http_password,
                proxy.fallback_to,
                proxy.fallback_timeout_ms,
                proxy.https2http as i64,
                proxy.https2http_ca_file,
                proxy.https2http_key_file,
                proxy.keep_tunnel_open as i64,
                proxy.status,
                serde_json::to_string(&proxy.transport).unwrap_or_else(|_| "{}".into()),
            ],
        )?;
        Ok(())
    }

    pub fn insert(&self, proxy: &mut FrpcProxy) -> Result<FrpcProxy, rusqlite::Error> {
        proxy.id = IdUtils::gen_uuid();
        self.upsert_for_migration(proxy)?;
        Ok(proxy.clone())
    }

    pub fn insert_many(&self, proxies: &mut [FrpcProxy]) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let tx = conn.unchecked_transaction()?;
        for proxy in proxies.iter_mut() {
            proxy.id = IdUtils::gen_uuid();
            self.upsert_for_migration(proxy)?;
        }
        tx.commit()
    }

    pub fn update_by_id(
        &self,
        id: &str,
        proxy: &mut FrpcProxy,
    ) -> Result<FrpcProxy, rusqlite::Error> {
        proxy.id = id.to_string();
        self.upsert_for_migration(proxy)?;
        Ok(proxy.clone())
    }

    pub fn update_proxy_status(&self, id: &str, status: i64) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        conn.execute(
            "UPDATE t_frpcd_proxies SET status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    pub fn delete_by_id(&self, id: &str) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        conn.execute("DELETE FROM t_frpcd_proxies WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn find_all(&self) -> Result<Vec<FrpcProxy>, rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare("SELECT * FROM t_frpcd_proxies ORDER BY rowid")?;
        let rows = stmt.query_map([], row_to_proxy)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

pub fn row_to_proxy(row: &rusqlite::Row) -> rusqlite::Result<FrpcProxy> {
    Ok(FrpcProxy {
        id: row.get("id")?,
        name: row.get("name")?,
        proxy_type: row.get("type")?,
        local_ip: row.get("local_ip")?,
        local_port: row.get("local_port")?,
        remote_port: row.get("remote_port")?,
        custom_domains: serde_json::from_str(&row.get::<_, String>("custom_domains_json")?)
            .unwrap_or_else(|_| vec!["".to_string()]),
        locations: serde_json::from_str(&row.get::<_, String>("locations_json")?)
            .unwrap_or_else(|_| vec!["".to_string()]),
        host_header_rewrite: row.get("host_header_rewrite")?,
        visitors_model: row.get("visitors_model")?,
        server_user: row.get("server_user")?,
        server_name: row.get("server_name")?,
        secret_key: row.get("secret_key")?,
        bind_addr: row.get("bind_addr")?,
        bind_port: row.get("bind_port")?,
        subdomain: row.get("subdomain")?,
        basic_auth: row.get::<_, i64>("basic_auth")? != 0,
        http_user: row.get("http_user")?,
        http_password: row.get("http_password")?,
        fallback_to: row.get("fallback_to")?,
        fallback_timeout_ms: row.get("fallback_timeout_ms")?,
        https2http: row.get::<_, i64>("https2http")? != 0,
        https2http_ca_file: row.get("https2http_ca_file")?,
        https2http_key_file: row.get("https2http_key_file")?,
        keep_tunnel_open: row.get::<_, i64>("keep_tunnel_open")? != 0,
        status: row.get("status")?,
        transport: serde_json::from_str(&row.get::<_, String>("transport_json")?)
            .unwrap_or_default(),
    })
}
