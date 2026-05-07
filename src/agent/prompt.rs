use std::fs;
use std::path::PathBuf;

use crate::config::Config;
use crate::error::VivariumError;

pub(super) fn agent_instructions(account: &str) -> Result<String, VivariumError> {
    for path in prompt_paths(account) {
        if path.exists() {
            return fs::read_to_string(&path).map_err(|e| {
                VivariumError::Config(format!(
                    "failed to read agent prompt {}: {e}",
                    path.display()
                ))
            });
        }
    }
    Ok(DEFAULT_AGENT_PROMPT.to_string())
}

pub(super) fn prompt_paths(account: &str) -> [PathBuf; 2] {
    let dir = Config::default_dir().join("agent");
    [
        dir.join("prompts").join(format!("{account}.md")),
        dir.join("prompt.md"),
    ]
}

const DEFAULT_AGENT_PROMPT: &str = "You are processing instructions delivered through Vivi's trusted agent mailbox.
Use your judgment about what action, if any, is appropriate. After processing, send a reply in this same thread from the receiving Vivi account summarizing what action you took, or explaining that no action was taken.
Use Vivi's draft/send surfaces for the reply and send from the receiving account. Do not send from another account unless the instruction explicitly asks you to.
";
