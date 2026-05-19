use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::VivariumError;
use crate::storage::Storage;
use crate::store::secure_create_dir_all;

mod delivery;
mod dump;
#[cfg(test)]
mod tests;

pub use dump::{DumpFilters, DumpRecord, MailDumpRequest, TaskDumpRequest, TaskDumpStatus};

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
    pub identities: Vec<LocalIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalIdentity {
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MailspaceStatus {
    pub found: bool,
    pub name: String,
    pub root: PathBuf,
    pub store: PathBuf,
    pub identities: Vec<IdentityStatus>,
    pub totals: StatusTotals,
}

#[derive(Debug, Clone, Serialize)]
pub struct IdentityStatus {
    pub identity: String,
    pub address: String,
    pub inbox_unread: usize,
    pub tasks_open: usize,
    pub done: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct StatusTotals {
    pub inbox_unread: usize,
    pub tasks_open: usize,
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
}

#[derive(Debug, Clone)]
pub struct DeliveryResult {
    pub delivered: Vec<DeliveredMessage>,
    pub sent: String,
}

#[derive(Debug, Clone)]
pub struct DeliveredMessage {
    pub identity: String,
    pub handle: String,
}

impl Mailspace {
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
            identities: Vec::new(),
        };
        write_config(&path, &config)?;
        Storage::open_mailspace(&dir)?;
        Self::open_root(root)
    }

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

    pub fn store_path(&self) -> PathBuf {
        self.dir.join("mail.sqlite")
    }

    pub fn storage(&self) -> Result<Storage, VivariumError> {
        Storage::open_mailspace(&self.dir)
    }

    pub fn add_identity(&mut self, identity: &str) -> Result<String, VivariumError> {
        let identity = sanitize_identity(identity)?;
        if !self
            .config
            .identities
            .iter()
            .any(|known| known.name == identity)
        {
            self.config.identities.push(LocalIdentity {
                name: identity.clone(),
            });
            self.config
                .identities
                .sort_by(|left, right| left.name.cmp(&right.name));
            write_config(&self.dir.join(MAILSPACE_CONFIG), &self.config)?;
        }
        Ok(self.address_for(&identity))
    }

    pub fn address_for(&self, identity: &str) -> String {
        format!("{identity}@{}.local", self.config.name)
    }

    pub fn resolve_identity(&self, value: &str) -> Result<String, VivariumError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(VivariumError::Message(
                "local identity cannot be empty".into(),
            ));
        }
        let identity = if let Some((local, domain)) = trimmed.rsplit_once('@') {
            let expected = format!("{}.local", self.config.name);
            if domain == "local" || domain == expected {
                local
            } else {
                return Err(VivariumError::Message(format!(
                    "external recipient '{trimmed}' is not allowed for local mailspace delivery"
                )));
            }
        } else {
            trimmed
        };
        let identity = sanitize_identity(identity)?;
        if self
            .config
            .identities
            .iter()
            .any(|known| known.name == identity)
        {
            Ok(identity)
        } else {
            Err(VivariumError::Message(format!(
                "unknown local identity '{identity}'; add it with `vivi mailspace identity add {identity}`"
            )))
        }
    }

    pub fn status(&self) -> Result<MailspaceStatus, VivariumError> {
        let storage = self.storage()?;
        let mut identities = Vec::new();
        let mut totals = StatusTotals::default();
        for identity in &self.config.identities {
            let inbox_unread =
                storage.count_messages_for_account_role(&identity.name, "inbox", Some(false))?;
            let tasks_open =
                storage.count_messages_for_account_role(&identity.name, "tasks", None)?;
            let done = storage.count_messages_for_account_role(&identity.name, "done", None)?;
            totals.inbox_unread += inbox_unread;
            totals.tasks_open += tasks_open;
            identities.push(IdentityStatus {
                identity: identity.name.clone(),
                address: self.address_for(&identity.name),
                inbox_unread,
                tasks_open,
                done,
            });
        }
        Ok(MailspaceStatus {
            found: true,
            name: self.config.name.clone(),
            root: self.root.clone(),
            store: self.store_path(),
            identities,
            totals,
        })
    }
}

pub fn print_status(status: &MailspaceStatus) {
    println!("mailspace {}", status.name);
    println!("root      {}", status.root.display());
    println!("store     {}", status.store.display());
    println!();
    println!("identity  inbox unread  tasks open  done");
    for identity in &status.identities {
        println!(
            "{:<9} {:<13} {:<11} {}",
            identity.identity, identity.inbox_unread, identity.tasks_open, identity.done
        );
    }
    println!();
    println!("total unread mail: {}", status.totals.inbox_unread);
    println!("total open tasks: {}", status.totals.tasks_open);
}

pub fn canonical_local_role(role: &str) -> Result<String, VivariumError> {
    match role.to_ascii_lowercase().as_str() {
        "inbox" => Ok("inbox".into()),
        "archive" | "all" => Ok("archive".into()),
        "trash" | "deleted" => Ok("trash".into()),
        "sent" => Ok("sent".into()),
        "draft" | "drafts" => Ok("drafts".into()),
        "task" | "tasks" | "open" => Ok("tasks".into()),
        "done" | "closed" => Ok("done".into()),
        _ => Err(VivariumError::Message(format!(
            "unsupported local folder '{role}'; expected inbox, archive, trash, sent, drafts, tasks, or done"
        ))),
    }
}

pub fn read_body_arg(value: &str) -> Result<String, VivariumError> {
    if let Some(path) = value.strip_prefix('@') {
        fs::read_to_string(path)
            .map_err(|e| VivariumError::Message(format!("failed to read body file {path}: {e}")))
    } else {
        Ok(value.to_string())
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
        message.push_str(&format!(
            "\nnearest git root: {}\ninit: vivi mailspace init --project {}",
            git.display(),
            git.display()
        ));
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

fn sanitize_identity(value: &str) -> Result<String, VivariumError> {
    let value = value.trim().to_ascii_lowercase();
    let valid = !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if valid {
        Ok(value)
    } else {
        Err(VivariumError::Message(format!(
            "invalid local identity '{value}'; use letters, numbers, dot, dash, or underscore"
        )))
    }
}

fn write_config(path: &Path, config: &MailspaceConfig) -> Result<(), VivariumError> {
    let raw = toml::to_string_pretty(config)
        .map_err(|e| VivariumError::Other(format!("failed to encode mailspace config: {e}")))?;
    fs::write(path, raw)?;
    Ok(())
}
