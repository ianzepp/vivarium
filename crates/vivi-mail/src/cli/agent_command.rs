use std::ffi::OsString;

use clap::{ArgGroup, Subcommand};

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

    /// Plan archive writes; use --execute to perform them immediately
    Archive {
        #[arg(required = true)]
        handles: Vec<String>,

        /// Execute the planned archives instead of only previewing
        #[arg(long)]
        execute: bool,

        /// Output JSON plan or execution results
        #[arg(long)]
        json: bool,
    },

    /// Plan delete writes; use --execute to perform them immediately
    #[command(group(
        ArgGroup::new("agent_delete_mode")
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

        /// Execute the planned deletes instead of only previewing
        #[arg(long)]
        execute: bool,

        /// Output JSON plan or execution results
        #[arg(long)]
        json: bool,
    },

    /// Plan one move; use --execute to perform it immediately
    Move {
        handle: String,
        folder: String,

        /// Execute the planned move instead of only previewing
        #[arg(long)]
        execute: bool,

        /// Output JSON plan or execution results
        #[arg(long)]
        json: bool,
    },

    /// Plan one read/star flag change; use --execute to perform it immediately
    #[command(group(
        ArgGroup::new("agent_flag_mode")
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

        /// Execute the planned flag change instead of only previewing
        #[arg(long)]
        execute: bool,

        /// Output JSON plan or execution results
        #[arg(long)]
        json: bool,
    },
}
