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

    /// Accept invalid TLS certificates for this run
    #[arg(long, global = true)]
    pub insecure: bool,

    /// Allow accounts.toml to be group/world readable
    #[arg(long, global = true)]
    pub ignore_permissions: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize vivarium config directory and files
    Init,

    #[cfg(feature = "outbox")]
    /// Authorize an OAuth account and store its refresh token
    Auth {
        /// Account to authorize (overrides --account)
        account: Option<String>,

        /// OAuth client ID (overrides account config)
        #[arg(long)]
        client_id: Option<String>,

        /// OAuth client secret (overrides account config)
        #[arg(long)]
        client_secret: Option<String>,
    },

    #[cfg(feature = "outbox")]
    /// Print a fresh OAuth access token for token_cmd
    Token {
        /// Account to mint a token for (overrides --account)
        account: Option<String>,
    },

    /// Sync mail from IMAP to local store
    Sync {
        /// Account to sync (overrides --account)
        #[arg(long)]
        account: Option<String>,

        /// Maximum number of new messages to download in this run
        #[arg(long)]
        limit: Option<usize>,

        /// Sync messages on or after this date (YYYY-MM-DD, or relative like 30d, 3mo, 1y)
        #[arg(long)]
        since: Option<String>,

        /// Sync messages before this date (YYYY-MM-DD)
        #[arg(long)]
        before: Option<String>,
    },

    /// Watch for new mail via IMAP IDLE and outbox changes
    #[cfg(feature = "outbox")]
    Watch {
        /// Account to watch (overrides --account)
        #[arg(long)]
        account: Option<String>,
    },

    /// Send a message from a file
    #[cfg(feature = "outbox")]
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

    /// Show one or more messages by ID
    Show {
        /// Message identifiers (filename stems)
        message_ids: Vec<String>,
    },

    /// Reply to a message
    #[cfg(feature = "outbox")]
    Reply {
        /// Message identifier to reply to
        message_id: String,

        /// Reply body text
        #[arg(long)]
        body: Option<String>,
    },

    /// Compose a new message
    #[cfg(feature = "outbox")]
    Compose {
        /// Recipient address
        #[arg(long)]
        to: String,

        /// Subject line
        #[arg(long)]
        subject: String,
    },

    /// Archive one or more messages (move from inbox to archive)
    Archive {
        /// Message identifiers
        message_ids: Vec<String>,
    },

    /// Search messages by keyword
    Search {
        /// Search query (space-separated keywords)
        query: String,

        /// Maximum results to return
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Number of results to skip
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}
