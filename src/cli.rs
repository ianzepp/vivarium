use std::path::PathBuf;

use clap::{ArgGroup, Parser, Subcommand};

mod draft_command;
mod index_command;
mod write_command;
pub use draft_command::{ComposeCommand, ReplyCommand};
pub use index_command::IndexCommand;
use std::ffi::OsString;
pub use write_command::{EnqueueCommand, ExecCommand, QueueCommand};

#[derive(Debug, Parser)]
#[command(name = "vivi", version, about = "Local-first IMAP email sync for LLMs")]
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

        /// Delete the local account cache before syncing
        #[arg(long)]
        reset: bool,

        /// Rebuild the deterministic metadata index after sync succeeds
        #[arg(long)]
        index: bool,

        /// Build local embeddings after sync succeeds; implies --index
        #[arg(long)]
        embed: bool,

        /// Sync all IMAP folders (Inbox, Sent, All Mail)
        #[arg(long)]
        all: bool,
    },

    /// List remote IMAP folders and capabilities
    Folders {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Check account configuration, IMAP, and SMTP connectivity
    Doctor {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Experimental direct Proton API probes
    Proton {
        #[command(subcommand)]
        command: ProtonCommand,
    },

    /// Watch for new mail via IMAP IDLE and outbox changes
    #[cfg(feature = "outbox")]
    Watch {
        /// Account to watch (overrides --account)
        #[arg(long)]
        account: Option<String>,
    },

    /// List messages in a folder (inbox, archive, trash, sent, drafts)
    List {
        /// Folder name
        #[arg(default_value = "inbox")]
        folder: String,

        /// Maximum messages to display per account
        #[arg(short = 'n', long)]
        limit: Option<usize>,

        /// Filter listed messages by handle, sender, or subject text
        #[arg(long)]
        filter: Option<String>,

        /// List messages on or after this date (YYYY-MM-DD, or relative like 30d, 3mo, 1y)
        #[arg(long)]
        since: Option<String>,

        /// List messages before this date (YYYY-MM-DD)
        #[arg(long)]
        before: Option<String>,
    },

    /// Show one or more messages by ID
    Show {
        /// Message identifiers (filename stems)
        #[arg(required = true)]
        message_ids: Vec<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show local thread context for a message
    Thread {
        /// Message identifier (filename stem)
        message_id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Maximum messages to include
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Create a reply draft for a message
    Reply(ReplyCommand),

    /// Compose a new local draft
    Compose(ComposeCommand),

    /// Export one raw .eml message by ID
    Export {
        /// Message identifier (filename stem)
        message_id: String,

        /// Export normalized local text instead of raw RFC 5322 bytes
        #[arg(long)]
        text: bool,
    },

    /// Search messages by keyword
    Search {
        /// Search query (space-separated keywords)
        query: String,

        /// Restrict results to one local folder role, such as inbox, archive, trash, sent, or drafts
        #[arg(long)]
        folder: Option<String>,

        /// Restrict results to messages from this sender address or From header text
        #[arg(long = "from")]
        from_addr: Option<String>,

        /// Restrict results to messages from this sender domain
        #[arg(long = "from-domain")]
        from_domain: Option<String>,

        /// Maximum results to return
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Number of results to skip
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Print only the total matching message count
        #[arg(long)]
        count: bool,

        /// Use local email embeddings for semantic search
        #[arg(long)]
        semantic: bool,

        /// Combine lexical and semantic search
        #[arg(long)]
        hybrid: bool,
    },

    /// Build and inspect derived local indexes
    Index {
        #[command(subcommand)]
        command: IndexCommand,
    },

    /// Poll locally downloaded mail for trusted agent instructions
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },

    /// Execute external writes immediately
    Exec {
        #[command(subcommand)]
        command: ExecCommand,
    },

    /// Add external writes to the durable review queue
    Enqueue {
        #[command(subcommand)]
        command: EnqueueCommand,
    },

    /// Inspect, drop, or run queued writes
    Queue {
        #[command(subcommand)]
        command: QueueCommand,
    },

    /// Show provider label support for the selected account
    Labels {
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Plan or apply a provider label operation
    #[command(group(
        ArgGroup::new("label_mode")
            .args(["add", "remove"])
            .required(true)
            .multiple(false)
    ))]
    Label {
        /// Message handle or local message identifier
        handle: String,

        /// Label to apply
        #[arg(long)]
        add: Option<String>,

        /// Label to remove
        #[arg(long)]
        remove: Option<String>,

        /// Preview without changing mailbox state
        #[arg(long)]
        dry_run: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProtonCommand {
    /// Fetch non-secret SRP auth bootstrap metadata
    AuthInfo {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Log in and store a reusable direct Proton API session
    Login {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,
        /// TOTP code for accounts that require one
        #[arg(long)]
        totp_code: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Fetch non-secret authenticated user and address metadata
    Identity {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Verify username/password login without storing returned tokens
    LoginCheck {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,
        /// TOTP code for accounts that require one
        #[arg(long)]
        totp_code: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Refresh and validate a stored direct Proton API session
    SessionCheck {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// Claim one trusted inbox thread and process it with Codex
    Poll {
        /// Exact sender email address allowed to issue agent instructions
        #[arg(long = "from")]
        from_addr: String,

        /// Folder role to scan
        #[arg(long, default_value = "inbox")]
        folder: String,

        /// Preview the next thread without claiming or invoking Codex
        #[arg(long)]
        dry_run: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Codex executable to run
        #[arg(long, default_value = "codex")]
        codex_command: OsString,

        /// Argument passed to the Codex command; defaults to `exec -`
        #[arg(long = "codex-arg", default_values = ["exec", "-"])]
        codex_args: Vec<OsString>,
    },
}
