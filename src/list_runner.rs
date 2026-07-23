use super::{Runtime, VivariumError};
use std::io::{self, Write};
use vivarium::{cli::Command, message::MessageEntry, store::MailStore};

#[allow(clippy::struct_excessive_bools)]
struct ListRequest<'a> {
    folder: &'a str,
    limit: Option<usize>,
    filter: Option<&'a str>,
    since: Option<String>,
    before: Option<String>,
    unread: bool,
    read: bool,
    starred: bool,
    unstarred: bool,
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
            starred,
            unstarred,
            json,
        } = command
        else {
            unreachable!();
        };
        self.list(&ListRequest {
            folder: &folder,
            limit,
            filter: filter.as_deref(),
            since,
            before,
            unread,
            read,
            starred,
            unstarred,
            json,
        })
    }

    fn list(&self, request: &ListRequest<'_>) -> Result<(), VivariumError> {
        let window =
            vivarium::sync::SyncWindow::parse(request.since.as_deref(), request.before.as_deref())?;
        let read_state = match (request.unread, request.read) {
            (true, false) => Some(false),
            (false, true) => Some(true),
            _ => None,
        };
        let starred = match (request.starred, request.unstarred) {
            (true, false) => Some(true),
            (false, true) => Some(false),
            _ => None,
        };
        let accounts = match &self.account {
            Some(name) => vec![self.accounts.find_account(name)?.clone()],
            None => self.accounts.accounts.clone(),
        };
        let mut json_output = Vec::new();
        let mut stdout = io::stdout().lock();
        for acct in &accounts {
            let store = MailStore::new(&acct.mail_path(&self.config));
            let entries = store.list_messages(request.folder)?;
            let entries = vivarium::list::filter_entries(
                entries,
                window,
                request.limit,
                request.filter,
                read_state,
                starred,
            );
            if request.json {
                json_output.push(ListAccountOutput {
                    account: acct.name.clone(),
                    folder: request.folder.to_string(),
                    messages: entries,
                });
            } else {
                handle_output_result(print_account_entries(
                    &mut stdout,
                    &acct.name,
                    request.folder,
                    &entries,
                ))?;
            }
        }
        if request.json {
            let raw = serde_json::to_string_pretty(&json_output)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?;
            handle_output_result(writeln!(stdout, "{raw}"))?;
        }
        Ok(())
    }
}

fn handle_output_result(result: io::Result<()>) -> Result<(), VivariumError> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn print_account_entries(
    writer: &mut impl Write,
    account: &str,
    folder: &str,
    entries: &[MessageEntry],
) -> io::Result<()> {
    writeln!(writer, "# {account}")?;
    vivarium::list::write_entries(writer, folder, entries)
}

#[cfg(test)]
#[path = "list_runner_test.rs"]
mod tests;
