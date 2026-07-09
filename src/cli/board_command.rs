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

    /// Output as JSON
    #[arg(long)]
    pub json: bool,

    /// Project root to use
    #[arg(long)]
    pub project: Option<PathBuf>,
}
