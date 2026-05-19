use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod work_command;
pub use work_command::{
    MailDumpCommand, NeedCommand, TaskDumpCommand, TaskDumpStatusArg, TaskStatus, WantCommand,
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
}

#[derive(Debug, Subcommand)]
pub enum MailCommand {
    /// Deliver local mail inside the current project mailspace only
    Send(LocalSendCommand),

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

    /// Dump local mailspace messages for audit and board review
    Dump(MailDumpCommand),
}

#[derive(Debug, Parser)]
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

    /// Message body, or @path to read body from a file
    #[arg(long)]
    pub body: String,

    /// Project root to use
    #[arg(long)]
    pub project: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum TaskCommand {
    /// Send a task message into the recipient's Tasks folder
    Send(LocalSendCommand),

    /// List tasks for an identity
    List {
        /// Identity whose tasks should be listed
        #[arg(long = "for")]
        for_identity: String,

        /// Task folder status
        #[arg(long, default_value = "open")]
        status: TaskStatus,

        /// Project root to use
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Show a task root message
    Show {
        /// Task handle or unambiguous prefix
        handle: String,

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
