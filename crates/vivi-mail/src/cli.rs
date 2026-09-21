//! Mail-side CLI surface.
//!
//! Argument structs and subcommand enums for the email half of the CLI. The
//! root binary owns the unified `Command` enum and holds these types in its
//! mail variants, so the parsed shape stays one flat command list.

use std::path::PathBuf;

use clap::{ArgGroup, Args, Subcommand};

mod agent_command;
mod draft_command;
mod index_command;
mod proton_command;
mod render_command;
mod write_command;

pub use agent_command::AgentCommand;
pub use draft_command::{ComposeCommand, ReplyCommand};
pub use index_command::IndexCommand;
pub use proton_command::ProtonCommand;
pub use render_command::{RenderCommand, RenderFormat};
pub use write_command::{EnqueueCommand, ExecCommand, QueueCommand};

/// Authorize an OAuth account and store its refresh token.
#[cfg(feature = "outbox")]
#[derive(Debug, Args)]
pub struct AuthArgs {
    /// Account to authorize (overrides --account)
    pub account: Option<String>,

    /// OAuth client ID (overrides account config)
    #[arg(long)]
    pub client_id: Option<String>,

    /// OAuth client secret (overrides account config)
    #[arg(long)]
    pub client_secret: Option<String>,
}

/// Print a fresh OAuth access token for token_cmd.
#[cfg(feature = "outbox")]
#[derive(Debug, Args)]
pub struct TokenArgs {
    /// Account to mint a token for (overrides --account)
    pub account: Option<String>,
}

/// Sync mail from IMAP to local store.
#[derive(Debug, Args)]
pub struct SyncArgs {
    /// Account to sync (overrides --account)
    #[arg(long)]
    pub account: Option<String>,

    /// Maximum number of new messages to download in this run
    #[arg(long)]
    pub limit: Option<usize>,

    /// Sync messages on or after this date (YYYY-MM-DD, or relative like 30d, 3mo, 1y)
    #[arg(long)]
    pub since: Option<String>,

    /// Sync messages before this date (YYYY-MM-DD)
    #[arg(long)]
    pub before: Option<String>,

    /// Delete the local account cache before syncing
    #[arg(long)]
    pub reset: bool,

    /// Confirm reset for accounts with a custom `mail_dir`
    #[arg(long)]
    pub confirm_reset: bool,

    /// Rebuild the deterministic metadata index after sync succeeds
    #[arg(long)]
    pub index: bool,

    /// Build local embeddings after sync succeeds; implies --index
    #[arg(long)]
    pub embed: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Sync all IMAP folders (Inbox, Sent, All Mail)
    #[arg(long)]
    pub all: bool,
}

/// Poll direct Proton API events and sync changed mail.
#[derive(Debug, Args)]
pub struct SyncEventsArgs {
    /// Account to sync (overrides --account)
    #[arg(long)]
    pub account: Option<String>,

    /// Run a normal direct Proton sync before initializing or polling the event cursor
    #[arg(long)]
    pub bootstrap: bool,

    /// Continue polling for events
    #[arg(long)]
    pub watch: bool,

    /// Poll interval for --watch, such as 30s, 5m, or a bare number of seconds
    #[arg(long, default_value = "30s")]
    pub interval: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// List remote IMAP folders and capabilities.
#[derive(Debug, Args)]
pub struct FoldersArgs {
    /// Account to inspect (overrides --account)
    #[arg(long)]
    pub account: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Check account configuration, IMAP, and SMTP connectivity.
#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Account to inspect (overrides --account)
    #[arg(long)]
    pub account: Option<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Experimental direct Proton API probes.
#[derive(Debug, Args)]
pub struct ProtonArgs {
    #[command(subcommand)]
    pub command: ProtonCommand,
}

/// Watch inbound IMAP mail and emit JSON events after local sync.
#[derive(Debug, Args)]
pub struct WatchInboxArgs {
    /// Account to watch (overrides --account)
    #[arg(long)]
    pub account: Option<String>,

    /// Emit the stable structured event contract required by Ops
    #[arg(long)]
    pub json: bool,
}

/// List messages in a folder (inbox, archive, trash, sent, drafts).
#[derive(Debug, Args)]
pub struct ListArgs {
    /// Folder name
    #[arg(default_value = "inbox")]
    pub folder: String,

    /// Maximum messages to display per account
    #[arg(short = 'n', long)]
    pub limit: Option<usize>,

    /// Filter listed messages by handle, sender, or subject text
    #[arg(long)]
    pub filter: Option<String>,

    /// List messages on or after this date (YYYY-MM-DD, or relative like 30d, 3mo, 1y)
    #[arg(long)]
    pub since: Option<String>,

    /// List messages before this date (YYYY-MM-DD)
    #[arg(long)]
    pub before: Option<String>,

    /// List only unread messages
    #[arg(long, conflicts_with = "read")]
    pub unread: bool,

    /// List only read messages
    #[arg(long)]
    pub read: bool,

    /// List only starred/flagged messages
    #[arg(long, visible_alias = "flagged", conflicts_with = "unstarred")]
    pub starred: bool,

    /// List only unstarred/unflagged messages
    #[arg(long, visible_alias = "unflagged")]
    pub unstarred: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Show one or more messages by ID.
#[derive(Debug, Args)]
pub struct ShowArgs {
    /// Message identifiers (filename stems)
    #[arg(required = true)]
    pub message_ids: Vec<String>,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Show local thread context for a message.
#[derive(Debug, Args)]
pub struct ThreadArgs {
    /// Message identifier (filename stem)
    pub message_id: String,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Maximum messages to include
    #[arg(long, default_value = "50")]
    pub limit: usize,
}

/// Export one raw .eml message by ID.
#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Message identifier (filename stem)
    pub message_id: String,

    /// Export normalized local text instead of raw RFC 5322 bytes
    #[arg(long)]
    pub text: bool,
}

/// Search messages by keyword.
#[derive(Debug, Args)]
pub struct SearchArgs {
    /// Search query (space-separated keywords)
    pub query: String,

    /// Restrict results to one local folder role, such as inbox, archive, trash, sent, or drafts
    #[arg(long)]
    pub folder: Option<String>,

    /// Restrict results to messages from this sender address or From header text
    #[arg(long = "from")]
    pub from_addr: Option<String>,

    /// Restrict results to messages from this sender domain
    #[arg(long = "from-domain")]
    pub from_domain: Option<String>,

    /// Maximum results to return
    #[arg(long, default_value = "20")]
    pub limit: usize,

    /// Number of results to skip
    #[arg(long, default_value = "0")]
    pub offset: usize,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Print only the total matching message count
    #[arg(long)]
    pub count: bool,

    /// Use local email embeddings for semantic search
    #[arg(long)]
    pub semantic: bool,

    /// Combine lexical and semantic search
    #[arg(long)]
    pub hybrid: bool,
}

/// Build and inspect derived local indexes.
#[derive(Debug, Args)]
pub struct IndexArgs {
    #[command(subcommand)]
    pub command: IndexCommand,
}

/// Poll locally downloaded mail for trusted agent instructions.
#[derive(Debug, Args)]
pub struct AgentArgs {
    #[command(subcommand)]
    pub command: AgentCommand,
}

/// Execute external writes immediately.
#[derive(Debug, Args)]
pub struct ExecArgs {
    #[command(subcommand)]
    pub command: ExecCommand,
}

/// Add external writes to the durable review queue.
#[derive(Debug, Args)]
pub struct EnqueueArgs {
    #[command(subcommand)]
    pub command: EnqueueCommand,
}

/// Inspect, drop, or run queued writes.
#[derive(Debug, Args)]
pub struct QueueArgs {
    #[command(subcommand)]
    pub command: QueueCommand,
}

/// Show provider label support for the selected account.
#[derive(Debug, Args)]
pub struct LabelsArgs {
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// Plan or apply a provider label operation.
#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("label_mode")
        .args(["add", "remove"])
        .required(true)
        .multiple(false)
))]
pub struct LabelArgs {
    /// Message handle or local message identifier
    pub handle: String,

    /// Label to apply
    #[arg(long)]
    pub add: Option<String>,

    /// Label to remove
    #[arg(long)]
    pub remove: Option<String>,

    /// Preview without changing mailbox state
    #[arg(long)]
    pub dry_run: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

/// The email half of the CLI, as one command set.
///
/// The root binary converts its unified `Command` into this enum, so the
/// mail dispatch stays self-contained inside this crate.
#[derive(Debug, Subcommand)]
pub enum MailCommand {
    /// Initialize vivarium config directory and files
    Init,

    #[cfg(feature = "outbox")]
    /// Authorize an OAuth account and store its refresh token
    Auth(AuthArgs),

    #[cfg(feature = "outbox")]
    /// Print a fresh OAuth access token for token_cmd
    Token(TokenArgs),

    /// Sync mail from IMAP to local store
    Sync(SyncArgs),

    /// Poll direct Proton API events and sync changed mail
    SyncEvents(SyncEventsArgs),

    /// List remote IMAP folders and capabilities
    Folders(FoldersArgs),

    /// Check account configuration, IMAP, and SMTP connectivity
    Doctor(DoctorArgs),

    /// Experimental direct Proton API probes
    Proton(ProtonArgs),

    /// Render a local Markdown document to HTML or PDF
    Render(RenderCommand),

    /// Watch inbound IMAP mail and emit JSON events after local sync
    WatchInbox(WatchInboxArgs),

    /// List messages in a folder (inbox, archive, trash, sent, drafts)
    List(ListArgs),

    /// Show one or more messages by ID
    Show(ShowArgs),

    /// Show local thread context for a message
    Thread(ThreadArgs),

    /// Create a reply draft for a message
    Reply(ReplyCommand),

    /// Compose a new local draft
    Compose(ComposeCommand),

    /// Export one raw .eml message by ID
    Export(ExportArgs),

    /// Search messages by keyword
    Search(SearchArgs),

    /// Build and inspect derived local indexes
    Index(IndexArgs),

    /// Poll locally downloaded mail for trusted agent instructions
    Agent(AgentArgs),

    /// Execute external writes immediately
    Exec(ExecArgs),

    /// Add external writes to the durable review queue
    Enqueue(EnqueueArgs),

    /// Inspect, drop, or run queued writes
    Queue(QueueArgs),

    /// Show provider label support for the selected account
    Labels(LabelsArgs),

    /// Plan or apply a provider label operation
    Label(LabelArgs),
}

/// Kept so the moved subcommand modules can name their own path argument.
pub type ProjectPath = PathBuf;
