use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "vivarium", about = "Local-first IMAP email sync for LLMs")]
pub struct Cli {
    /// Path to config file
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Account name to operate on
    #[arg(long, global = true)]
    pub account: Option<String>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize vivarium config directory and files
    Init,

    /// Sync mail from IMAP to local store
    Sync {
        /// Account to sync (overrides --account)
        #[arg(long)]
        account: Option<String>,
    },

    /// Watch for new mail via IMAP IDLE and outbox changes
    Watch {
        /// Account to watch (overrides --account)
        #[arg(long)]
        account: Option<String>,
    },

    /// Send a message from a file
    Send {
        /// Path to the .eml file
        path: PathBuf,
    },

    /// List messages in a folder (inbox, archive, sent, drafts)
    List {
        /// Folder name
        #[arg(default_value = "inbox")]
        folder: String,
    },

    /// Show a message by ID
    Show {
        /// Message identifier (filename stem)
        message_id: String,
    },

    /// Reply to a message
    Reply {
        /// Message identifier to reply to
        message_id: String,
    },

    /// Compose a new message
    Compose {
        /// Recipient address
        #[arg(long)]
        to: String,

        /// Subject line
        #[arg(long)]
        subject: String,
    },

    /// Archive a message (move from inbox to archive)
    Archive {
        /// Message identifier
        message_id: String,
    },
}
