use std::io;

#[derive(Debug, thiserror::Error)]
pub enum VivariumError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("IMAP error: {0}")]
    Imap(String),

    #[error("SMTP error: {0}")]
    Smtp(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("parse error: {0}")]
    Parse(String),

    #[error("message error: {0}")]
    Message(String),

    #[error("{0}")]
    Other(String),
}
