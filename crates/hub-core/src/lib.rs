//! ASTERION Hub core: everything except the window. Signed catalog, trust, resumable verified
//! downloads, installing and detecting apps, history and settings. The desktop shell (Tauri) and the
//! publisher CLI are thin layers over this crate.

pub mod catalog;
pub mod download;
pub mod engine;
pub mod error;
pub mod manifest;
pub mod msi_info;
pub mod net;
pub mod platform;
pub mod store;
pub mod trust;

pub use error::{HubError, Result};
