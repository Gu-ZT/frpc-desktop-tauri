//! Frpc-Desktop Tauri backend.
//!
//! This crate contains the complete Rust port of the former Electron main
//! process: database, services (frpc process management, version download,
//! TOML generation), the IPC command layer and the window/tray lifecycle.

pub mod app;
pub mod core;
pub mod db;
pub mod ipc;
pub mod model;
pub mod service;
pub mod util;

/// Setup the Tauri application (plugins, state, listeners, tray).
pub fn run() {
    app::FrpcDesktopApp::run();
}
