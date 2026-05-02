use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum IndexCommand {
    /// Rebuild the deterministic metadata index
    Rebuild,

    /// Show deterministic index status
    Status,

    /// Show how many cataloged messages are not indexed
    Pending,

    /// Build provider/model-scoped email embeddings
    Embeddings {
        /// Embed only chunks missing from the selected embedding DB
        #[arg(long)]
        pending: bool,

        /// Clear and rebuild embeddings for the selected account
        #[arg(long)]
        rebuild: bool,

        /// Maximum indexed messages to process
        #[arg(long)]
        limit: Option<usize>,

        /// Embedding provider name
        #[arg(long, default_value = "ollama")]
        provider: String,

        /// Embedding model name
        #[arg(long, default_value = "cassio-embedding")]
        model: String,

        /// Embedding endpoint URL
        #[arg(long, default_value = "http://127.0.0.1:11434/api/embed")]
        endpoint: String,
    },
}
