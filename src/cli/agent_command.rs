use std::path::PathBuf;

use clap::{ArgGroup, Subcommand};

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// Plan or execute an archive mutation
    Archive {
        handle: String,
        #[arg(long)]
        execute: bool,
    },

    /// Plan or execute a delete mutation
    Delete {
        handle: String,
        #[arg(long)]
        expunge: bool,
        #[arg(long)]
        confirm: bool,
        #[arg(long)]
        execute: bool,
    },

    /// Plan or execute a move mutation
    Move {
        handle: String,
        folder: String,
        #[arg(long)]
        execute: bool,
    },

    /// Plan or execute a flag mutation
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
        #[arg(long)]
        execute: bool,
    },

    /// Plan or execute sending an explicit .eml file
    Send {
        path: PathBuf,
        #[arg(long)]
        execute: bool,
    },

    /// Plan or execute local reply draft creation
    Reply {
        handle: String,
        #[arg(long)]
        body: String,
        #[arg(long)]
        execute: bool,
    },
}
