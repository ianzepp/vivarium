pub mod catalog;
pub mod cli;
pub mod config;
pub mod error;
pub mod extract;
pub mod imap;
pub mod init;
pub mod list;
pub mod message;
pub mod oauth;
pub mod retrieve;
pub mod search;
pub mod thread;

#[cfg(feature = "outbox")]
pub mod outbox;

#[cfg(feature = "outbox")]
pub mod smtp;
pub mod store;
pub mod sync;

#[cfg(feature = "outbox")]
pub mod watch;

pub use error::VivariumError;
