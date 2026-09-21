#![deny(clippy::pedantic)]

//! Email transport, sync, and local archive for Vivi.
//!
//! This crate owns everything that talks to a mail provider or to the local
//! email archive: IMAP, SMTP, the direct Proton API, sync, send, indexing,
//! embeddings, search, drafts, and the agent mailbox. It holds no project
//! mailspace state.

pub mod agent;
pub mod agent_runner;
pub mod catalog;
pub mod cli;
pub mod config;
pub mod doctor_command;
pub mod draft_runner;
pub mod duration;
pub mod email_index;
pub mod embeddings;
pub mod error;
pub mod extract;
pub mod folders_command;
pub mod imap;
pub mod index_runner;
pub mod init;
pub mod label_runner;
pub mod labels;
pub mod list;
pub mod list_runner;
pub mod message;
pub mod mutation_command;
pub mod mutation_runner;
pub mod oauth;
pub mod policy;
pub mod proton_api;
pub mod proton_api_command;
pub mod proton_decrypt;
pub mod proton_encrypt;
pub mod proton_events;
pub mod proton_fixture_command;
pub mod proton_send;
pub mod proton_sync;
pub mod queue;
pub mod queue_runner;
pub mod render;
pub mod render_command;
pub mod retrieve;
pub mod runtime;
pub mod search;
pub mod smtp;
pub mod storage;
pub mod store;
pub mod sync;
pub mod sync_command;
pub mod sync_events_command;
pub mod thread;
pub mod watch_inbox;

#[cfg(feature = "outbox")]
pub mod outbox;

// Names the moved command modules reach through `super::`.
pub use error::VivariumError;
pub use runtime::Runtime;
pub(crate) use runtime::print_sync_result;
pub use store::MailStore;
