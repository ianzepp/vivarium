use std::path::PathBuf;

use clap::{ArgGroup, Subcommand};

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// Plan an archive mutation
    Archive {
        #[arg(required = true)]
        handles: Vec<String>,
    },

    /// Plan a delete mutation
    Delete {
        #[arg(required = true)]
        handles: Vec<String>,
        #[arg(long)]
        expunge: bool,
    },

    /// Plan a move mutation
    Move { handle: String, folder: String },

    /// Plan a flag mutation
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
    },

    /// Plan sending an explicit .eml file
    Send { path: PathBuf },

    /// Plan local reply draft creation
    Reply {
        handle: String,
        #[arg(long)]
        body: String,
    },
}
