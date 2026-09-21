use std::path::PathBuf;

use clap::{Args, ValueEnum};

#[derive(Debug, Args)]
pub struct RenderCommand {
    /// Markdown source document; omit only with --explain
    pub input: Option<PathBuf>,

    /// Atomically written output path
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Output format; otherwise inferred from --output, defaulting to pdf
    #[arg(long, value_enum)]
    pub format: Option<RenderFormat>,

    /// Explain installed pipelines and prerequisites without rendering
    #[arg(long)]
    pub explain: bool,

    /// Pin a renderer for this invocation, overriding config
    #[arg(long)]
    pub engine: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lower")]
pub enum RenderFormat {
    Html,
    Pdf,
}
