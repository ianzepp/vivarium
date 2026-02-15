pub mod cli;
pub mod config;
pub mod error;
pub mod imap;
pub mod message;
pub mod outbox;
pub mod smtp;
pub mod store;
pub mod sync;
pub mod watch;

pub use error::VivariumError;
