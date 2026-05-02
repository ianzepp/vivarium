pub mod agent;
pub mod catalog;
pub mod cli;
pub mod config;
pub mod email_index;
pub mod error;
pub mod extract;
pub mod imap;
pub mod init;
pub mod labels;
pub mod list;
pub mod message;
pub mod mutation_command;
pub mod oauth;
pub mod retrieve;
pub mod search;
pub mod thread;

#[cfg(feature = "outbox")]
pub mod outbox;

pub mod smtp;
pub mod store;
pub mod sync;

#[cfg(feature = "outbox")]
pub mod watch;

pub use error::VivariumError;
