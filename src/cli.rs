//! Unified CLI surface for the `vivi` binary.
//!
//! Project-mailspace commands are declared here. Email commands are declared in
//! `vivi_mail::cli` and held in the mail variants below, so the parsed command
//! list stays flat while each half owns its own argument structs.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use vivi_mail::cli as mail;

mod board_command;
mod mailspace_command;
mod role_command;

pub use board_command::BoardCommand;
pub use mailspace_command::{
    AbsorbCommand, CycleCommand, GoalCommand, GraphActivateCommand, GraphApplyCommand,
    GraphAuditCommand, GraphCommand, GraphCompleteCommand, GraphConnectCommand,
    GraphEdgeAddCommand, GraphEdgeCommand, GraphExportCommand, GraphImportCommand,
    GraphNodeAddCommand, GraphNodeCommand, GraphReadyCommand, GraphShowCommand, KindWatchCommand,
    LocalSendCommand, MailAbsorbStatus, MailCommand, MailDumpCommand, MailListCommand,
    MailReplyCommand, MailThreadCommand, MailspaceArchiveCommand, MailspaceCommand,
    MailspaceIdentityCommand, MailspaceImportCommand, MailspaceWatchCommand, MemoCommand,
    NeedCommand, NeedSendCommand, TaskCommand, TaskDumpCommand, TaskDumpStatusArg, TaskFromCommand,
    TaskSendCommand, TaskStatus, TraceCommand, WantCommand, WantSendCommand, WantStatus,
    WatchCommon,
};
pub use role_command::{RoleCharterCommand, RoleCommand};

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
    /// Project root for mailspace commands (board, task, need, want, mail, mailspace)
    #[arg(long, global = true)]
    pub project: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Initialize vivarium config directory and files
    Init,

    #[cfg(feature = "outbox")]
    /// Authorize an OAuth account and store its refresh token
    Auth(mail::AuthArgs),

    #[cfg(feature = "outbox")]
    /// Print a fresh OAuth access token for token_cmd
    Token(mail::TokenArgs),

    /// Sync mail from IMAP to local store
    Sync(mail::SyncArgs),

    /// Poll direct Proton API events and sync changed mail
    SyncEvents(mail::SyncEventsArgs),

    /// List remote IMAP folders and capabilities
    Folders(mail::FoldersArgs),

    /// Check account configuration, IMAP, and SMTP connectivity
    Doctor(mail::DoctorArgs),

    /// Experimental direct Proton API probes
    Proton(mail::ProtonArgs),

    /// Render a local Markdown document to HTML or PDF
    Render(mail::RenderCommand),

    /// Watch inbound IMAP mail and emit JSON events after local sync
    WatchInbox(mail::WatchInboxArgs),

    /// List messages in a folder (inbox, archive, trash, sent, drafts)
    List(mail::ListArgs),

    /// Show project-local actionable work across tasks, needs, and wants
    Board(BoardCommand),

    /// Show the whole project frame in one read: seats, loops, unabsorbed mail,
    /// handles with verdicts, goals with register tallies, and the backlog
    /// sliced for dispatch. Read-only and stateless, so it serves a cold boot
    /// and a post-compaction warm boot alike.
    Boot {
        /// Project root that owns .vivi/ (also accepted globally: vivi --project <ROOT> boot)
        #[arg(long)]
        project: Option<PathBuf>,
    },

    /// Manage a project-local Vivi mailspace
    Mailspace {
        #[command(subcommand)]
        command: MailspaceCommand,
    },

    /// Send and inspect project-local mail with no external side effects
    Mail {
        #[command(subcommand)]
        command: MailCommand,
    },

    /// Send and complete project-local tasks as folder-based mail
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },

    /// Send and complete project-local needs as prioritized mail
    Need {
        #[command(subcommand)]
        command: NeedCommand,
    },

    /// Send and promote project-local wants for later prioritization
    Want {
        #[command(subcommand)]
        command: WantCommand,
    },

    /// Save and inspect project-local memos as durable role memory
    Memo {
        #[command(subcommand)]
        command: MemoCommand,
    },

    /// Register goal document paths the Mind should keep monitoring
    Goal {
        #[command(subcommand)]
        command: GoalCommand,
    },

    /// Manage first-class mailspace agent seats (roles)
    Role {
        #[command(subcommand)]
        command: RoleCommand,
    },

    /// Inspect project-local agent cycle intake
    Cycle {
        #[command(subcommand)]
        command: CycleCommand,
    },

    /// Show one or more messages by ID
    Show(mail::ShowArgs),

    /// Show local thread context for a message
    Thread(mail::ThreadArgs),

    /// Trace the cross-role communication tree around a handle
    Trace(TraceCommand),

    /// Import and inspect executable work graphs
    Graph {
        #[command(subcommand)]
        command: GraphCommand,
    },

    /// Adjudicate the backlog graph into a dispatch/exception manifest
    Step {
        /// Settled item handle to adjudicate and complete (apply mode)
        #[arg(long)]
        apply: Option<String>,

        /// Project root to use
        #[arg(long)]
        project: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Create a reply draft for a message
    Reply(mail::ReplyCommand),

    /// Compose a new local draft
    Compose(mail::ComposeCommand),

    /// Export one raw .eml message by ID
    Export(mail::ExportArgs),

    /// Search messages by keyword
    Search(mail::SearchArgs),

    /// Build and inspect derived local indexes
    Index(mail::IndexArgs),

    /// Poll locally downloaded mail for trusted agent instructions
    Agent(mail::AgentArgs),

    /// Execute external writes immediately
    Exec(mail::ExecArgs),

    /// Add external writes to the durable review queue
    Enqueue(mail::EnqueueArgs),

    /// Inspect, drop, or run queued writes
    Queue(mail::QueueArgs),

    /// Show provider label support for the selected account
    Labels(mail::LabelsArgs),

    /// Plan or apply a provider label operation
    Label(mail::LabelArgs),
}
