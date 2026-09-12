//! Shinobi library crate.
//!
//! Exposes the same modules used by the binary so integration tests can build
//! routers, databases and parsers without shelling out to the executable.

pub mod api;
pub mod config;
pub mod contracts;
pub mod scraper;
pub mod storage;

pub use config::ScrapeConfig;
