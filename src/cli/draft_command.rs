use std::path::PathBuf;

use clap::{ArgGroup, Args};

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("reply_html")
        .args(["html_body", "html_body_auto"])
        .multiple(false)
))]
pub struct ReplyCommand {
    /// Message handle or local message identifier to reply to
    pub handle: String,

    /// Sender address to use for this draft
    #[arg(long)]
    pub from: Option<String>,

    /// Reply body text
    #[arg(long)]
    pub body: Option<String>,

    /// HTML body for multipart/alternative replies
    #[arg(long)]
    pub html_body: Option<String>,

    /// Generate a simple styled HTML body from --body
    #[arg(long, requires = "body")]
    pub html_body_auto: bool,

    /// Append the created draft to the remote Drafts folder
    #[arg(long)]
    pub append_remote: bool,
}

#[derive(Debug, Args)]
#[command(group(
    ArgGroup::new("compose_html")
        .args(["html_body", "html_body_auto"])
        .multiple(false)
))]
pub struct ComposeCommand {
    /// Sender address to use for this draft
    #[arg(long)]
    pub from: Option<String>,

    /// Recipient address
    #[arg(long)]
    pub to: Vec<String>,

    /// Cc recipient address
    #[arg(long)]
    pub cc: Vec<String>,

    /// Bcc recipient address
    #[arg(long)]
    pub bcc: Vec<String>,

    /// Subject line
    #[arg(long)]
    pub subject: String,

    /// Plain-text body
    #[arg(long)]
    pub body: Option<String>,

    /// HTML body for multipart/alternative messages
    #[arg(long)]
    pub html_body: Option<String>,

    /// Generate a simple styled HTML body from --body
    #[arg(long, requires = "body")]
    pub html_body_auto: bool,

    /// Append the created draft to the remote Drafts folder
    #[arg(long)]
    pub append_remote: bool,

    /// Attach an existing local file
    #[arg(long = "attach")]
    pub attachments: Vec<PathBuf>,

    /// Attach this Markdown source and a locally generated PDF
    #[arg(long)]
    pub attach_document: Option<PathBuf>,
}
