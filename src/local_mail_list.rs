use serde::Serialize;
use vivarium::VivariumError;
use vivarium::mailspace::Mailspace;
use vivarium::storage::StoredMessageView;

#[derive(Debug, Serialize)]
struct MailListItem {
    handle: String,
    date: String,
    from: String,
    to: String,
    subject: String,
    role: String,
}

pub(crate) fn print_mail_list(
    mailspace: &Mailspace,
    identity: &str,
    role: &str,
    json: bool,
) -> Result<(), VivariumError> {
    let items: Vec<MailListItem> = mailspace
        .list(identity, role)?
        .into_iter()
        .map(mail_list_item)
        .collect();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&items)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    if items.is_empty() {
        println!("  no messages in {role}");
        return Ok(());
    }
    for item in &items {
        println!(
            "  {}  {}  {}  {}",
            item.handle, item.date, item.from, item.subject
        );
    }
    Ok(())
}

fn mail_list_item(message: StoredMessageView) -> MailListItem {
    MailListItem {
        handle: message.handle,
        date: message.date,
        from: message.from_addr,
        to: message.to_addr,
        subject: message.subject,
        role: message.local_role,
    }
}
