//! Network helpers (ported from electron/utils/NetUtils.ts).

use std::net::TcpListener;

pub struct NetUtils;

impl NetUtils {
    /// Check whether a TCP port is in use on the given host.
    /// Returns `true` when the port is already bound.
    pub fn check_port_in_use(port: i64, host: &str) -> bool {
        match TcpListener::bind((host, port as u16)) {
            Ok(listener) => {
                drop(listener);
                false
            }
            Err(_) => true,
        }
    }
}
