use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{MAILSPACE_CONFIG, Mailspace, write_config};
use crate::error::VivariumError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalIdentity {
    pub name: String,
    /// Former names this identity was known by, kept so historical mail
    /// (stored under the old name) still resolves and shows up under the
    /// renamed identity. Renaming never rewrites stored message rows.
    #[serde(default)]
    pub aliases: Vec<String>,
}

impl Mailspace {
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
                aliases: Vec::new(),
            });
            self.config
                .identities
                .sort_by(|left, right| left.name.cmp(&right.name));
            write_config(&self.dir.join(MAILSPACE_CONFIG), &self.config)?;
        }
        Ok(self.address_for(&identity))
    }

    /// Renames a local identity in the roster. Historical mail already
    /// stored under the old name is left untouched: the old name is kept
    /// as an alias so it keeps resolving and its mail keeps counting
    /// toward this identity in status, list, and dump output.
    pub fn rename_identity(&mut self, old: &str, new: &str) -> Result<String, VivariumError> {
        let old = sanitize_identity(old)?;
        let new = sanitize_identity(new)?;
        let Some(index) = self
            .config
            .identities
            .iter()
            .position(|known| known.name == old || known.aliases.iter().any(|a| a == &old))
        else {
            return Err(VivariumError::Message(format!(
                "unknown local identity '{old}'"
            )));
        };
        if self.config.identities[index].name == new {
            return Err(VivariumError::Message(format!(
                "identity is already named '{new}'"
            )));
        }
        if self.config.identities.iter().enumerate().any(|(i, known)| {
            i != index && (known.name == new || known.aliases.iter().any(|a| a == &new))
        }) {
            return Err(VivariumError::Message(format!(
                "local identity '{new}' already exists"
            )));
        }
        let previous_name = self.config.identities[index].name.clone();
        self.config.identities[index].name = new.clone();
        if !self.config.identities[index]
            .aliases
            .iter()
            .any(|a| a == &previous_name)
        {
            self.config.identities[index].aliases.push(previous_name);
        }
        self.config
            .identities
            .sort_by(|left, right| left.name.cmp(&right.name));
        write_config(&self.dir.join(MAILSPACE_CONFIG), &self.config)?;
        Ok(self.address_for(&new))
    }

    pub fn address_for(&self, identity: &str) -> String {
        format!("{identity}@{}.local", self.config.name)
    }

    /// All names (current name plus former names) that historical mail for
    /// this identity may be stored under. `canonical` must already be a
    /// resolved identity name.
    pub fn identity_names(&self, canonical: &str) -> HashSet<String> {
        let mut names = HashSet::new();
        names.insert(canonical.to_string());
        if let Some(identity) = self
            .config
            .identities
            .iter()
            .find(|known| known.name == canonical)
        {
            names.extend(identity.aliases.iter().cloned());
        }
        names
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
        if let Some(known) =
            self.config.identities.iter().find(|known| {
                known.name == identity || known.aliases.iter().any(|a| a == &identity)
            })
        {
            Ok(known.name.clone())
        } else {
            Err(VivariumError::Message(format!(
                "unknown local identity '{identity}'; add it with `vivi mailspace identity add {identity}`"
            )))
        }
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
