use std::fmt::Write;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::VivariumError;
use crate::storage::Storage;
use crate::store::secure_create_dir_all;

mod body;
mod delivery;
mod dump;
mod event_log;
mod graph;
mod graph_mutate;
mod identity;
mod kind;
mod lifecycle;
mod merge;
mod mermaid;
mod reply;
#[cfg(test)]
mod tests;
mod thread;
mod trace;
mod watch;

pub use body::{read_body_arg, read_body_input};
pub use dump::{
    DumpFilters, DumpRecord, MailDumpRequest, TaskDumpRequest, TaskDumpStatus, parse_time_bound,
};
pub use graph::{GraphEdgeView, GraphNodeView, GraphShow, print_graph_show, print_import_report};
pub use graph_mutate::print_apply_report;
pub use identity::{
    LocalIdentity, ROLE_HARNESS_SUBAGENT, ROLE_STATUS_ACTIVE, ROLE_STATUS_PARKED,
    ROLE_STATUS_RETIRED, RoleUpdate, RoleView,
};
pub use lifecycle::{
    CycleIntake, MailAbsorbFilter, SourceTaskRequest, SourceTaskResult, WantListOptions,
    WantListRecord, WantMetadataUpdate,
};
pub use merge::{MailspaceImportOptions, MailspaceImportReport, import_mailspace};
pub use thread::{MailspaceThreadMessage, print_thread};
pub use trace::{print_trace, print_trace_json};
pub use watch::{MailspaceWatchRequest, run_watch};

const MAILSPACE_DIR: &str = ".vivi";
const MAILSPACE_CONFIG: &str = "mailspace.toml";

#[derive(Debug, Clone)]
pub struct Mailspace {
    pub root: PathBuf,
    pub dir: PathBuf,
    pub config: MailspaceConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MailspaceConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub identities: Vec<LocalIdentity>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MailspaceStatus {
    pub found: bool,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub root: PathBuf,
    pub store: PathBuf,
    pub identities: Vec<IdentityStatus>,
    pub totals: StatusTotals,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityStatus {
    pub identity: String,
    pub address: String,
    pub aliases: Vec<String>,
    pub actionable_open: usize,
    pub inbox_unread: usize,
    pub tasks_open: usize,
    pub needs_open: usize,
    pub wants_open: usize,
    pub memos_open: usize,
    pub done: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StatusTotals {
    pub actionable_open: usize,
    pub inbox_unread: usize,
    pub tasks_open: usize,
    pub needs_open: usize,
    pub wants_open: usize,
    pub memos_open: usize,
}

#[derive(Debug, Clone)]
pub struct SendRequest {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: String,
    pub body: String,
    pub role: String,
    pub kind: Option<String>,
    pub reply_to: Option<String>,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct DeliveryResult {
    pub delivered: Vec<DeliveredMessage>,
    pub sent: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeliveredMessage {
    pub identity: String,
    pub handle: String,
}

impl Mailspace {
    /// Initialise a new mailspace at the given project path, or the current
    /// directory. Creates the `.vivi` directory, config, and store.
    ///
    /// # Errors
    /// Returns an error if directory creation, config writing, or store
    /// initialisation fails.
    pub fn init(project: Option<&Path>) -> Result<Self, VivariumError> {
        let root = project_root(project)?;
        let dir = root.join(MAILSPACE_DIR);
        secure_create_dir_all(&dir)?;
        secure_create_dir_all(&dir.join("blobs"))?;
        let path = dir.join(MAILSPACE_CONFIG);
        if path.exists() {
            return Self::open_root(root);
        }
        let config = MailspaceConfig {
            name: default_name(&root),
            description: None,
            identities: Vec::new(),
        };
        write_config(&path, &config)?;
        Storage::open_mailspace(&dir)?;
        Self::open_root(root)
    }

    /// Discover a mailspace by searching ancestors from the given project
    /// path, or the current directory.
    ///
    /// # Errors
    /// Returns an error if no mailspace config is found in the ancestor chain,
    /// or if reading the config file fails.
    pub fn discover(project: Option<&Path>) -> Result<Self, VivariumError> {
        if let Some(project) = project {
            return Self::open_root(project.to_path_buf());
        }
        let cwd = std::env::current_dir()?;
        let Some(root) = find_root(&cwd) else {
            return Err(missing_mailspace_error(&cwd));
        };
        Self::open_root(root)
    }

    /// Open a mailspace at an explicit root path.
    ///
    /// # Errors
    /// Returns an error if the config file cannot be read or parsed.
    pub fn open_root(root: PathBuf) -> Result<Self, VivariumError> {
        let dir = root.join(MAILSPACE_DIR);
        let path = dir.join(MAILSPACE_CONFIG);
        let raw = fs::read_to_string(&path).map_err(|e| {
            VivariumError::Message(format!(
                "failed to read mailspace config at {}: {e}",
                path.display()
            ))
        })?;
        let config: MailspaceConfig = toml::from_str(&raw).map_err(|e| {
            VivariumError::Message(format!(
                "failed to parse mailspace config at {}: {e}",
                path.display()
            ))
        })?;
        Ok(Self { root, dir, config })
    }

    #[must_use]
    pub fn store_path(&self) -> PathBuf {
        self.dir.join("mail.sqlite")
    }

    /// Open the `SQLite` storage for this mailspace.
    ///
    /// # Errors
    /// Returns an error if the database cannot be opened or migrations fail.
    pub fn storage(&self) -> Result<Storage, VivariumError> {
        Storage::open_mailspace(&self.dir)
    }

    /// Set the mailspace description.
    ///
    /// # Errors
    /// Returns an error if the config cannot be written.
    pub fn set_description(&mut self, description: Option<String>) -> Result<(), VivariumError> {
        self.config.description = description;
        let path = self.dir.join(MAILSPACE_CONFIG);
        write_config(&path, &self.config)
    }

    /// Return a status summary for the mailspace and all identities.
    ///
    /// # Errors
    /// Returns an error if the storage cannot be opened or queries fail.
    pub fn status(&self) -> Result<MailspaceStatus, VivariumError> {
        let storage = self.storage()?;
        let mut identities = Vec::new();
        let mut totals = StatusTotals::default();
        for identity in &self.config.identities {
            let names = self.identity_names(&identity.name);
            let mut inbox_unread = 0;
            let mut tasks_open = 0;
            let mut needs_open = 0;
            let mut wants_open = 0;
            let mut memos_open = 0;
            let mut done = 0;
            for name in &names {
                inbox_unread +=
                    storage.count_messages_for_account_role(name, "inbox", Some(false))?;
                tasks_open += storage.count_messages_for_account_role(name, "tasks", None)?;
                needs_open += storage.count_messages_for_account_role(name, "needs", None)?;
                wants_open += storage.count_messages_for_account_role(name, "wants", None)?;
                memos_open += storage.count_messages_for_account_role(name, "memos", None)?;
                done += storage.count_messages_for_account_role(name, "done", None)?;
            }
            let actionable_open = tasks_open + needs_open;
            totals.actionable_open += actionable_open;
            totals.inbox_unread += inbox_unread;
            totals.tasks_open += tasks_open;
            totals.needs_open += needs_open;
            totals.wants_open += wants_open;
            totals.memos_open += memos_open;
            identities.push(IdentityStatus {
                identity: identity.name.clone(),
                address: self.address_for(&identity.name),
                aliases: identity.aliases.clone(),
                actionable_open,
                inbox_unread,
                tasks_open,
                needs_open,
                wants_open,
                memos_open,
                done,
            });
        }
        Ok(MailspaceStatus {
            found: true,
            name: self.config.name.clone(),
            description: self.config.description.clone(),
            root: self.root.clone(),
            store: self.store_path(),
            identities,
            totals,
        })
    }
}

pub fn print_status(status: &MailspaceStatus) {
    println!("mailspace {}", status.name);
    if let Some(description) = &status.description {
        println!("descr     {description}");
    }
    println!("root      {}", status.root.display());
    println!("store     {}", status.store.display());
    println!();
    println!(
        "identity  actionable  tasks open  needs open  wants open  memos open  inbox unread  done"
    );
    for identity in &status.identities {
        println!(
            "{:<9} {:<11} {:<11} {:<11} {:<11} {:<11} {:<13} {}",
            identity.identity,
            identity.actionable_open,
            identity.tasks_open,
            identity.needs_open,
            identity.wants_open,
            identity.memos_open,
            identity.inbox_unread,
            identity.done
        );
        if !identity.aliases.is_empty() {
            println!("  formerly: {}", identity.aliases.join(", "));
        }
    }
    println!();
    println!("total actionable open: {}", status.totals.actionable_open);
    println!("total unread mail: {}", status.totals.inbox_unread);
    println!("total open tasks: {}", status.totals.tasks_open);
    println!("total open needs: {}", status.totals.needs_open);
    println!("total open wants: {}", status.totals.wants_open);
    println!("total open memos: {}", status.totals.memos_open);
}

/// Canonicalise a local folder/role name (e.g. `inbox`, `tasks`, `done`).
///
/// # Errors
/// Returns an error if the role name is not recognised.
pub fn canonical_local_role(role: &str) -> Result<String, VivariumError> {
    match role.to_ascii_lowercase().as_str() {
        "inbox" => Ok("inbox".into()),
        "archive" | "all" => Ok("archive".into()),
        "trash" | "deleted" => Ok("trash".into()),
        "sent" => Ok("sent".into()),
        "draft" | "drafts" => Ok("drafts".into()),
        "task" | "tasks" | "open" => Ok("tasks".into()),
        "need" | "needs" => Ok("needs".into()),
        "want" | "wants" => Ok("wants".into()),
        "memo" | "memos" => Ok("memos".into()),
        "done" | "closed" => Ok("done".into()),
        _ => Err(VivariumError::Message(format!(
            "unsupported local folder '{role}'; expected inbox, archive, trash, sent, drafts, tasks, needs, wants, memos, or done"
        ))),
    }
}

fn project_root(project: Option<&Path>) -> Result<PathBuf, VivariumError> {
    match project {
        Some(path) => Ok(path.to_path_buf()),
        None => std::env::current_dir().map_err(Into::into),
    }
}

fn find_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|path| path.join(MAILSPACE_DIR).join(MAILSPACE_CONFIG).is_file())
        .map(Path::to_path_buf)
}

fn missing_mailspace_error(cwd: &Path) -> VivariumError {
    let mut message = format!("No Vivi mailspace found.\ncwd: {}", cwd.display());
    if let Some(git) = nearest_git_root(cwd) {
        let _ = write!(
            message,
            "\nnearest git root: {}\ninit: vivi mailspace init --project {}",
            git.display(),
            git.display()
        );
    } else {
        message.push_str("\ninit: vivi mailspace init");
    }
    VivariumError::Message(message)
}

fn nearest_git_root(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|path| path.join(".git").exists())
        .map(Path::to_path_buf)
}

fn default_name(root: &Path) -> String {
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");
    sanitize_project_name(name)
}

fn sanitize_project_name(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if matches!(ch, '-' | '_' | '.') && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "project".into()
    } else {
        out
    }
}

pub(super) fn write_config(path: &Path, config: &MailspaceConfig) -> Result<(), VivariumError> {
    let raw = toml::to_string_pretty(config)
        .map_err(|e| VivariumError::Other(format!("failed to encode mailspace config: {e}")))?;
    fs::write(path, raw)?;
    Ok(())
}
