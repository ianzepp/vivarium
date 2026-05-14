use super::{Runtime, VivariumError};
use vivarium::{cli::Command, message::MessageEntry, store::MailStore};

struct ListRequest<'a> {
    folder: &'a str,
    limit: Option<usize>,
    filter: Option<&'a str>,
    since: Option<String>,
    before: Option<String>,
    unread: bool,
    read: bool,
    json: bool,
}

#[derive(serde::Serialize)]
struct ListAccountOutput {
    account: String,
    folder: String,
    messages: Vec<MessageEntry>,
}

impl Runtime {
    pub(crate) fn run_list_command(&self, command: Command) -> Result<(), VivariumError> {
        let Command::List {
            folder,
            limit,
            filter,
            since,
            before,
            unread,
            read,
            json,
        } = command
        else {
            unreachable!();
        };
        self.list(ListRequest {
            folder: &folder,
            limit,
            filter: filter.as_deref(),
            since,
            before,
            unread,
            read,
            json,
        })
    }

    fn list(&self, request: ListRequest<'_>) -> Result<(), VivariumError> {
        let window =
            vivarium::sync::SyncWindow::parse(request.since.as_deref(), request.before.as_deref())?;
        let read_state = match (request.unread, request.read) {
            (true, false) => Some(false),
            (false, true) => Some(true),
            _ => None,
        };
        let accounts = match &self.account {
            Some(name) => vec![self.accounts.find_account(name)?.clone()],
            None => self.accounts.accounts.clone(),
        };
        let mut json_output = Vec::new();
        for acct in &accounts {
            let store = MailStore::new(&acct.mail_path(&self.config));
            let entries = store.list_messages(request.folder)?;
            let entries = vivarium::list::filter_entries(
                entries,
                window,
                request.limit,
                request.filter,
                read_state,
            );
            if request.json {
                json_output.push(ListAccountOutput {
                    account: acct.name.clone(),
                    folder: request.folder.to_string(),
                    messages: entries,
                });
            } else {
                println!("# {}", acct.name);
                vivarium::list::print_entries(request.folder, &entries);
            }
        }
        if request.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&json_output)
                    .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
            );
        }
        Ok(())
    }
}
