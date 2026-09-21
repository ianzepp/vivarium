use std::path::PathBuf;

use clap::{ArgGroup, Subcommand};

#[derive(Debug, Subcommand)]
pub enum ExecCommand {
    /// Execute archive writes immediately
    Archive {
        #[arg(required = true)]
        handles: Vec<String>,

        /// Output JSON execution results
        #[arg(long)]
        json: bool,
    },

    /// Execute delete writes immediately
    #[command(group(
        ArgGroup::new("exec_delete_mode")
            .args(["trash", "expunge"])
            .multiple(false)
    ))]
    Delete {
        #[arg(required = true)]
        handles: Vec<String>,

        /// Move to Trash; this is the default delete behavior
        #[arg(long)]
        trash: bool,

        /// Permanently expunge the remote message
        #[arg(long)]
        expunge: bool,

        /// Required with --expunge for immediate hard delete
        #[arg(long)]
        confirm: bool,

        /// Output JSON execution results
        #[arg(long)]
        json: bool,
    },

    /// Execute one move immediately
    Move {
        handle: String,
        folder: String,

        /// Output JSON execution results
        #[arg(long)]
        json: bool,
    },

    /// Execute one read/star flag change immediately
    #[command(group(
        ArgGroup::new("exec_flag_mode")
            .args(["read", "unread", "star", "unstar"])
            .required(true)
            .multiple(false)
    ))]
    Flag {
        handle: String,
        #[arg(long)]
        read: bool,
        #[arg(long)]
        unread: bool,
        #[arg(long)]
        star: bool,
        #[arg(long)]
        unstar: bool,

        /// Output JSON execution results
        #[arg(long)]
        json: bool,
    },

    /// Send an explicit .eml file immediately
    Send {
        path: PathBuf,

        /// Sender address to set before sending
        #[arg(long)]
        from: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum EnqueueCommand {
    /// Queue archive writes for later review and execution
    Archive {
        #[arg(required = true)]
        handles: Vec<String>,
    },

    /// Queue delete writes for later review and execution
    #[command(group(
        ArgGroup::new("enqueue_delete_mode")
            .args(["trash", "expunge"])
            .multiple(false)
    ))]
    Delete {
        #[arg(required = true)]
        handles: Vec<String>,

        /// Move to Trash; this is the default delete behavior
        #[arg(long)]
        trash: bool,

        /// Queue a permanent expunge
        #[arg(long)]
        expunge: bool,

        /// Store approval for a queued hard delete
        #[arg(long)]
        confirm: bool,
    },

    /// Queue one move for later review and execution
    Move { handle: String, folder: String },

    /// Queue one read/star flag change for later review and execution
    #[command(group(
        ArgGroup::new("enqueue_flag_mode")
            .args(["read", "unread", "star", "unstar"])
            .required(true)
            .multiple(false)
    ))]
    Flag {
        handle: String,
        #[arg(long)]
        read: bool,
        #[arg(long)]
        unread: bool,
        #[arg(long)]
        star: bool,
        #[arg(long)]
        unstar: bool,
    },

    /// Queue sending an explicit .eml file
    Send {
        path: PathBuf,

        /// Sender address to set before sending
        #[arg(long)]
        from: Option<String>,
    },

    /// Queue local reply draft creation
    Reply {
        handle: String,
        #[arg(long)]
        body: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum QueueCommand {
    /// List queued work items
    List {
        /// Include executed, failed, and dropped items
        #[arg(long)]
        all: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show one queued work item
    Show {
        id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Mark one queued work item as dropped
    Drop { id: String },

    /// Execute queued work now
    Run {
        /// Queue item IDs to execute
        ids: Vec<String>,

        /// Run all pending items in FIFO order
        #[arg(long)]
        all: bool,
    },
}
