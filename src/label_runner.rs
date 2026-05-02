use vivarium::VivariumError;
use vivarium::cli::Command;
use vivarium::labels::{self, LabelOperation};

use super::Runtime;

pub(super) enum LabelDispatch {
    Handled,
    Unhandled(Command),
}

impl Runtime {
    pub(super) async fn run_label_command(
        &self,
        command: Command,
    ) -> Result<LabelDispatch, VivariumError> {
        match command {
            Command::Labels { json } => self.labels(json)?,
            Command::Label {
                handle,
                add,
                remove,
                dry_run,
                json,
            } => self.label(&handle, add, remove, dry_run, json)?,
            other => return Ok(LabelDispatch::Unhandled(other)),
        }
        Ok(LabelDispatch::Handled)
    }

    fn labels(&self, as_json: bool) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let support = labels::support(&acct);
        if as_json {
            println!("{}", serde_json::to_string_pretty(&support).unwrap());
            return Ok(());
        }
        println!("provider: {}", support.provider);
        println!("mode: {}", support.mode);
        println!("label mutation supported: no");
        println!("reason: {}", support.reason);
        println!("safe alternative: {}", support.safe_alternative);
        Ok(())
    }

    fn label(
        &self,
        handle: &str,
        add: Option<String>,
        remove: Option<String>,
        dry_run: bool,
        as_json: bool,
    ) -> Result<(), VivariumError> {
        let acct = self.resolve_account(self.account.clone())?;
        let (operation, label) = label_request(add, remove)?;
        let plan = labels::plan_json(&acct, handle, operation, &label, dry_run);
        if as_json || dry_run {
            println!("{}", serde_json::to_string_pretty(&plan).unwrap());
            return Ok(());
        }
        Err(labels::unsupported_error(&acct, &label))
    }
}

fn label_request(
    add: Option<String>,
    remove: Option<String>,
) -> Result<(LabelOperation, String), VivariumError> {
    match (add, remove) {
        (Some(label), None) => Ok((LabelOperation::Add, label)),
        (None, Some(label)) => Ok((LabelOperation::Remove, label)),
        _ => Err(VivariumError::Message(
            "choose exactly one of --add or --remove".into(),
        )),
    }
}
