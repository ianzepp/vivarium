use std::ffi::OsString;

use clap::Subcommand;

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
