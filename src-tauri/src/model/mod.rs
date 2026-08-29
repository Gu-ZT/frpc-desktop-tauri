//! Data models matching the global TypeScript types under `types/`.
//!
//! Field names and JSON shapes must stay identical to the Electron version so
//! that the renderer (which still uses the TS types) keeps working unchanged.

pub mod frp;
pub mod frpc;
pub mod github;
