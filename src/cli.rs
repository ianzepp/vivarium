use std::path::PathBuf;

use clap::{ArgGroup, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "vivi", version, about = "Local-first IMAP email sync for LLMs")]
pub struct Cli {
    /// Path to config file
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,

    /// Account name to operate on
    #[arg(long, global = true)]
    pub account: Option<String>,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Accept invalid TLS certificates for this run
    #[arg(long, global = true)]
    pub insecure: bool,

    /// Allow accounts.toml to be group/world readable
    #[arg(long, global = true)]
    pub ignore_permissions: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;

    #[test]
    fn parses_archive_dry_run_json() {
        let cli =
            Cli::try_parse_from(["vivi", "archive", "abc123", "--dry-run", "--json"]).unwrap();

        match cli.command {
            Command::Archive {
                handles,
                dry_run,
                json,
            } => {
                assert_eq!(handles, vec!["abc123"]);
                assert!(dry_run);
                assert!(json);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_delete_expunge_confirm() {
        let cli =
            Cli::try_parse_from(["vivi", "delete", "abc123", "--expunge", "--confirm"]).unwrap();

        match cli.command {
            Command::Delete {
                handle,
                expunge,
                confirm,
                ..
            } => {
                assert_eq!(handle, "abc123");
                assert!(expunge);
                assert!(confirm);
            }
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn rejects_multiple_flag_modes() {
        let err =
            Cli::try_parse_from(["vivi", "flag", "abc123", "--read", "--unread"]).unwrap_err();

        assert_eq!(err.kind(), clap::error::ErrorKind::ArgumentConflict);
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize vivarium config directory and files
    Init,

    #[cfg(feature = "outbox")]
    /// Authorize an OAuth account and store its refresh token
    Auth {
        /// Account to authorize (overrides --account)
        account: Option<String>,

        /// OAuth client ID (overrides account config)
        #[arg(long)]
        client_id: Option<String>,

        /// OAuth client secret (overrides account config)
        #[arg(long)]
        client_secret: Option<String>,
    },

    #[cfg(feature = "outbox")]
    /// Print a fresh OAuth access token for token_cmd
    Token {
        /// Account to mint a token for (overrides --account)
        account: Option<String>,
    },

    /// Sync mail from IMAP to local store
    Sync {
        /// Account to sync (overrides --account)
        #[arg(long)]
        account: Option<String>,

        /// Maximum number of new messages to download in this run
        #[arg(long)]
        limit: Option<usize>,

        /// Sync messages on or after this date (YYYY-MM-DD, or relative like 30d, 3mo, 1y)
        #[arg(long)]
        since: Option<String>,

        /// Sync messages before this date (YYYY-MM-DD)
        #[arg(long)]
        before: Option<String>,

        /// Delete the local account cache before syncing
        #[arg(long)]
        reset: bool,
    },

    /// List remote IMAP folders and capabilities
    Folders {
        /// Account to inspect (overrides --account)
        #[arg(long)]
        account: Option<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Watch for new mail via IMAP IDLE and outbox changes
    #[cfg(feature = "outbox")]
    Watch {
        /// Account to watch (overrides --account)
        #[arg(long)]
        account: Option<String>,
    },

    /// Send a message from a file
    #[cfg(feature = "outbox")]
    Send {
        /// Path to the .eml file
        path: PathBuf,
    },

    /// List messages in a folder (inbox, archive, trash, sent, drafts)
    List {
        /// Folder name
        #[arg(default_value = "inbox")]
        folder: String,

        /// Maximum messages to display per account
        #[arg(short = 'n', long)]
        limit: Option<usize>,

        /// List messages on or after this date (YYYY-MM-DD, or relative like 30d, 3mo, 1y)
        #[arg(long)]
        since: Option<String>,

        /// List messages before this date (YYYY-MM-DD)
        #[arg(long)]
        before: Option<String>,
    },

    /// Show one or more messages by ID
    Show {
        /// Message identifiers (filename stems)
        #[arg(required = true)]
        message_ids: Vec<String>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show local thread context for a message
    Thread {
        /// Message identifier (filename stem)
        message_id: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Maximum messages to include
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Reply to a message
    #[cfg(feature = "outbox")]
    Reply {
        /// Message identifier to reply to
        message_id: String,

        /// Reply body text
        #[arg(long)]
        body: Option<String>,
    },

    /// Compose a new message
    #[cfg(feature = "outbox")]
    Compose {
        /// Recipient address
        #[arg(long)]
        to: String,

        /// Subject line
        #[arg(long)]
        subject: String,
    },

    /// Archive one or more messages remotely, then update the local mirror
    Archive {
        /// Message handles or local message identifiers
        #[arg(required = true)]
        handles: Vec<String>,

        /// Preview the remote mutation without changing mailbox state
        #[arg(long)]
        dry_run: bool,

        /// Output an agent-readable JSON plan/result
        #[arg(long)]
        json: bool,
    },

    /// Delete one message remotely, trashing by default
    #[command(group(
        ArgGroup::new("delete_mode")
            .args(["trash", "expunge"])
            .multiple(false)
    ))]
    Delete {
        /// Message handle or local message identifier
        handle: String,

        /// Move to Trash; this is the default delete behavior
        #[arg(long)]
        trash: bool,

        /// Permanently expunge the remote message
        #[arg(long)]
        expunge: bool,

        /// Required with --expunge for non-dry-run hard delete
        #[arg(long)]
        confirm: bool,

        /// Preview the remote mutation without changing mailbox state
        #[arg(long)]
        dry_run: bool,

        /// Output an agent-readable JSON plan/result
        #[arg(long)]
        json: bool,
    },

    /// Move one message to a supported folder role
    Move {
        /// Message handle or local message identifier
        handle: String,

        /// Destination folder role: inbox, archive, trash, sent, or drafts
        folder: String,

        /// Preview the remote mutation without changing mailbox state
        #[arg(long)]
        dry_run: bool,

        /// Output an agent-readable JSON plan/result
        #[arg(long)]
        json: bool,
    },

    /// Mutate read/star flags on one message
    #[command(group(
        ArgGroup::new("flag_mode")
            .args(["read", "unread", "star", "unstar"])
            .required(true)
            .multiple(false)
    ))]
    Flag {
        /// Message handle or local message identifier
        handle: String,

        #[arg(long)]
        read: bool,

        #[arg(long)]
        unread: bool,

        #[arg(long)]
        star: bool,

        #[arg(long)]
        unstar: bool,

        /// Preview the remote mutation without changing mailbox state
        #[arg(long)]
        dry_run: bool,

        /// Output an agent-readable JSON plan/result
        #[arg(long)]
        json: bool,
    },

    /// Export one raw .eml message by ID
    Export {
        /// Message identifier (filename stem)
        message_id: String,

        /// Export normalized local text instead of raw RFC 5322 bytes
        #[arg(long)]
        text: bool,
    },

    /// Search messages by keyword
    Search {
        /// Search query (space-separated keywords)
        query: String,

        /// Maximum results to return
        #[arg(long, default_value = "20")]
        limit: usize,

        /// Number of results to skip
        #[arg(long, default_value = "0")]
        offset: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}
