//! IPC command layer (port of `electron/controller/*` + `electron/core/IpcRouter.ts`).
//!
//! Every Tauri command returns an `ApiResponse` JSON with the same shape as
//! the Electron `ResponseUtils` (`{ bizCode, data, message }`).

pub mod commands;
pub mod router;

pub use commands::*;
