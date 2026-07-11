use std::path::PathBuf;

use clap::{ArgGroup, Parser, Subcommand};

mod work_command;
pub use work_command::{
    MailDumpCommand, NeedCommand, TaskDumpCommand, TaskDumpStatusArg, TaskStatus, WantCommand,
    WantStatus,
};

#[derive(Debug, Subcommand)]
pub enum MailspaceCommand {
    /// Explicitly initialize .vivi/ in a project root
    Init {
        /// Project root to initialize
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Show mailspace root, store, identities, and waiting work
    Status {
        /// Project root to inspect
        #[arg(long)]
        project: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Wait for project-local mailspace events; this is not IMAP watch
    Watch(Box<MailspaceWatchCommand>),

    /// Manage local identities in the explicit roster
    Identity {
        #[command(subcommand)]
        command: MailspaceIdentityCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum MailspaceIdentityCommand {
    /// Add a local identity, such as ceo or cto
    Add {
        /// Identity name
        identity: String,

        /// Project root to update
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// List local identities
    List {
        /// Project root to inspect
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Rename a local identity, keeping the old name as an alias so
    /// historical mail still resolves
    Rename {
        /// Current identity name
        old: String,

        /// New identity name
        new: String,

        /// Project root to update
        #[arg(long)]
        project: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum MailCommand {
    /// Deliver local mail inside the current project mailspace only
    Send(LocalSendCommand),

    /// Wait for mail events in the project-local mailspace
    Watch(Box<MailspaceWatchCommand>),

    /// Deliver an explicit .eml into local identities in the current project mailspace
    Deliver {
        /// Path to RFC 5322 .eml file
        path: PathBuf,

        /// Target local folder role
        #[arg(long, default_value = "inbox")]
        folder: String,

        /// Project root to use
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// List local mail for an identity
    List {
        /// Identity whose mailbox should be listed
        #[arg(long = "for")]
        for_identity: String,

        /// Folder role to list
        #[arg(long, default_value = "inbox")]
        folder: String,

        /// Project root to use
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Show one or more local mail messages by handle
    Show {
        /// Local mail handle or unambiguous prefix
        #[arg(required = true)]
        handles: Vec<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Project root to use
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Show the project-local conversation containing a handle
    Thread(MailThreadCommand),

    /// Send a project-local reply to any mailspace kind
    Reply(MailReplyCommand),

    /// Dump local mailspace messages for audit and board review
    Dump(MailDumpCommand),
}

#[derive(Debug, Parser)]
#[command(group(
    ArgGroup::new("body_input")
        .required(true)
        .args(["body", "body_file"])
))]
pub struct LocalSendCommand {
    /// Local sender identity or address
    #[arg(long)]
    pub from: String,

    /// Local To recipient; may be passed multiple times
    #[arg(long)]
    pub to: Vec<String>,

    /// Local Cc recipient; may be passed multiple times
    #[arg(long)]
    pub cc: Vec<String>,

    /// Rejected for local delivery in v1
    #[arg(long)]
    pub bcc: Vec<String>,

    /// Message subject
    #[arg(long)]
    pub subject: String,

    /// Existing mailspace handle to make this message a captured reply
    #[arg(long)]
    pub reply_to: Option<String>,

    /// Message body, or @path to read body from a file
    #[arg(long)]
    pub body: Option<String>,

    /// Read message body from a file
    #[arg(long)]
    pub body_file: Option<PathBuf>,

    /// Project root to use
    #[arg(long)]
    pub project: Option<PathBuf>,
}

#[derive(Debug, Clone, Parser)]
#[command(group(
    ArgGroup::new("reply_body")
        .required(true)
        .args(["body", "body_file"])
))]
pub struct MailReplyCommand {
    /// Existing mailspace handle to reply to
    pub handle: String,

    /// Local sender identity
    #[arg(long)]
    pub from: String,

    /// Optional explicit To recipient; defaults to the parent sender
    #[arg(long)]
    pub to: Vec<String>,

    /// Optional Cc recipient
    #[arg(long)]
    pub cc: Vec<String>,

    /// Reply subject; defaults to a single Re: prefix
    #[arg(long)]
    pub subject: Option<String>,

    /// Reply body, or @path to read body from a file
    #[arg(long)]
    pub body: Option<String>,

    /// Read reply body from a file
    #[arg(long)]
    pub body_file: Option<PathBuf>,

    /// Project root to use
    #[arg(long)]
    pub project: Option<PathBuf>,
}

#[derive(Debug, Clone, Parser)]
pub struct MailThreadCommand {
    /// Any local mailspace handle or unambiguous prefix
    pub handle: String,

    /// Include best-effort historical links in this read view
    #[arg(long)]
    pub infer: bool,

    /// Maximum number of thread messages to include
    #[arg(long, default_value = "50")]
    pub limit: usize,

    /// Maximum ancestor/descendant depth to walk
    #[arg(long, default_value = "50")]
    pub max_depth: usize,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Project root to use
    #[arg(long)]
    pub project: Option<PathBuf>,
}

#[derive(Debug, Clone, Parser)]
pub struct MailspaceWatchCommand {
    /// Identity whose local events should wake the watcher
    #[arg(long = "for")]
    pub for_identity: String,

    /// Comma-separated kinds; defaults to mail,task,need
    #[arg(long, default_value = "mail,task,need")]
    pub kinds: String,

    /// Comma-separated raw event types
    #[arg(long, default_value = "delivered,moved")]
    pub events: String,

    /// Optional comma-separated derived destination statuses
    #[arg(long)]
    pub statuses: Option<String>,

    /// Optional sender identity filter
    #[arg(long)]
    pub match_from: Option<String>,

    /// Optional case-sensitive subject prefix filter
    #[arg(long)]
    pub match_subject_prefix: Option<String>,

    /// Optional handle whose message must change
    #[arg(long)]
    pub handle: Option<String>,

    /// Number of matches before exit; zero follows until interrupted
    #[arg(long, default_value_t = 1)]
    pub until_count: usize,

    /// Exit nonzero if no match arrives in this duration
    #[arg(long)]
    pub timeout: Option<String>,

    /// Scan once and exit without blocking
    #[arg(long)]
    pub once: bool,

    /// Initial time lower bound, used only without a cursor file
    #[arg(long)]
    pub since: Option<String>,

    /// Caller-owned event-id cursor file
    #[arg(long, conflicts_with = "watermark_file")]
    pub cursor_file: Option<PathBuf>,

    /// Alias for the caller-owned event-id cursor file
    #[arg(long, conflicts_with = "cursor_file")]
    pub watermark_file: Option<PathBuf>,

    /// Write the cursor after a successful scan/match
    #[arg(long)]
    pub write_cursor: bool,

    /// Alias for --write-cursor
    #[arg(long)]
    pub write_watermark: bool,

    /// Poll interval, such as 250ms, 2s, or a bare number of seconds
    #[arg(long, default_value = "250ms")]
    pub poll_interval: String,

    /// Output one JSON object per matching event
    #[arg(long)]
    pub json: bool,

    /// Project root to use
    #[arg(long)]
    pub project: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// Send a task message into the recipient's Tasks folder
    Send(LocalSendCommand),

    /// Wait for task events in the project-local mailspace
    Watch(Box<MailspaceWatchCommand>),

    /// List tasks for an identity
    List {
        /// Identity whose tasks should be listed
        #[arg(long = "for")]
        for_identity: String,

        /// Task folder status
        #[arg(long, default_value = "open")]
        status: TaskStatus,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Project root to use
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Show a task root message
    Show {
        /// Task handle or unambiguous prefix
        handle: String,

        /// Include thread context as JSON
        #[arg(long)]
        json: bool,

        /// Project root to use
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Dump local task messages for audit and board review
    Dump(TaskDumpCommand),

    /// Move a task from Tasks to Done
    Done {
        /// Task handle or unambiguous prefix
        handle: String,

        /// Identity completing the task
        #[arg(long = "for")]
        for_identity: String,

        /// Optional completion note recorded in the mailspace event ledger
        #[arg(long)]
        note: Option<String>,

        /// Project root to use
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Move a task from Done back to Tasks
    Reopen {
        /// Task handle or unambiguous prefix
        handle: String,

        /// Identity reopening the task
        #[arg(long = "for")]
        for_identity: String,

        /// Optional reopen note recorded in the mailspace event ledger
        #[arg(long)]
        note: Option<String>,

        /// Project root to use
        #[arg(long)]
        project: Option<PathBuf>,
    },
}
