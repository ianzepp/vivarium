pub mod cli;
pub mod config;
pub mod error;
pub mod imap;
pub mod init;
pub mod message;
pub mod oauth;

#[cfg(feature = "outbox")]
pub mod outbox;

#[cfg(feature = "outbox")]
pub mod smtp;
pub mod store;
pub mod sync;

#[cfg(feature = "outbox")]
pub mod watch;

pub use error::VivariumError;
