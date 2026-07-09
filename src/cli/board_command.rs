use std::path::PathBuf;

use clap::Args;

#[derive(Debug, Args)]
pub struct BoardCommand {
    /// Identity whose board should be shown
    #[arg(long = "for")]
    pub for_identity: Option<String>,

    /// Maximum wants to show per identity
    #[arg(long, default_value_t = 5)]
    pub wants: usize,

    /// Show items created or moved on or after this time
    #[arg(long)]
    pub since: Option<String>,

    /// Read the since bound from this agent-owned file when --since is absent
    #[arg(long)]
    pub watermark_file: Option<PathBuf>,

    /// Write a fresh watermark to --watermark-file after a successful board run
    #[arg(long, requires = "watermark_file")]
    pub write_watermark: bool,

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Project root to use
    #[arg(long)]
    pub project: Option<PathBuf>,
}
