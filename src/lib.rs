#![deny(clippy::pedantic)]

pub mod agent;
pub mod catalog;
pub mod cli;
pub mod config;
pub mod duration;
pub mod email_index;
pub mod embeddings;
pub mod error;
pub mod extract;
pub mod imap;
pub mod init;
pub mod labels;
pub mod list;
pub mod mailspace;
pub mod message;
pub mod mutation_command;
pub mod oauth;
pub mod policy;
pub mod proton_api;
pub mod proton_decrypt;
pub mod proton_encrypt;
pub mod proton_events;
pub mod proton_send;
pub mod proton_sync;
pub mod queue;
pub mod render;
pub mod retrieve;
pub mod role_schedule;
pub mod role_status;
pub mod search;
pub mod storage;
pub mod thread;
pub mod watch_inbox;

#[cfg(feature = "outbox")]
pub mod outbox;

pub mod smtp;
pub mod store;
pub mod sync;

pub use error::VivariumError;
