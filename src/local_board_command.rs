use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;
use vivarium::VivariumError;
use vivarium::cli::BoardCommand;
use vivarium::mailspace::Mailspace;
use vivarium::storage::{MailspaceEvent, Storage, StoredMessageView};

#[derive(Debug, Serialize)]
struct Board {
    name: String,
    root: PathBuf,
    totals: BoardTotals,
    identities: Vec<IdentityBoard>,
}

#[derive(Debug, Default, Serialize)]
struct BoardTotals {
    actionable_open: usize,
    tasks_open: usize,
    needs_open: usize,
    wants_open: usize,
    wants_shown: usize,
    wants_hidden: usize,
}

#[derive(Debug, Serialize)]
struct IdentityBoard {
    identity: String,
    address: String,
    actionable_open: usize,
    tasks: Vec<BoardItem>,
    needs: Vec<BoardItem>,
    wants: Vec<BoardItem>,
    wants_hidden: usize,
}

#[derive(Debug, Serialize)]
struct BoardItem {
    handle: String,
    date: String,
    from: String,
    subject: String,
    last_event: Option<BoardEvent>,
}

#[derive(Debug, Serialize)]
struct BoardEvent {
    occurred_at: String,
    command: String,
    event_type: String,
    note: Option<String>,
}

pub(crate) fn handle_board_command(command: &BoardCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let since = resolve_since(command)?;
    let board = build_board(
        &mailspace,
        command.for_identity.as_deref(),
        command.wants,
        since,
    )?;
    if command.json {
        print_json(&board)?;
    } else {
        print_board(&board);
    }
    write_watermark(command)?;
    Ok(())
}

fn build_board(
    mailspace: &Mailspace,
    for_identity: Option<&str>,
    wants_cap: usize,
    since: Option<DateTime<Utc>>,
) -> Result<Board, VivariumError> {
    let identities = board_identities(mailspace, for_identity)?;
    let storage = mailspace.storage()?;
    let mut boards = Vec::new();
    for identity in identities {
        boards.push(build_identity_board(
            mailspace, &storage, &identity, wants_cap, since,
        )?);
    }
    Ok(Board {
        name: mailspace.config.name.clone(),
        root: mailspace.root.clone(),
        totals: board_totals(&boards),
        identities: boards,
    })
}

fn board_identities(
    mailspace: &Mailspace,
    for_identity: Option<&str>,
) -> Result<Vec<String>, VivariumError> {
    if let Some(identity) = for_identity {
        return Ok(vec![mailspace.resolve_identity(identity)?]);
    }
    Ok(mailspace
        .config
        .identities
        .iter()
        .map(|identity| identity.name.clone())
        .collect())
}

fn build_identity_board(
    mailspace: &Mailspace,
    storage: &Storage,
    identity: &str,
    wants_cap: usize,
    since: Option<DateTime<Utc>>,
) -> Result<IdentityBoard, VivariumError> {
    let tasks = board_items(
        storage,
        mailspace.list_kind(identity, "tasks", "task")?,
        None,
        since,
    )?;
    let needs = board_items(
        storage,
        mailspace.list_kind(identity, "needs", "need")?,
        None,
        since,
    )?;
    let wants_open = mailspace.list_kind(identity, "wants", "want")?;
    let (wants, wants_count) = board_items_with_count(storage, wants_open, Some(wants_cap), since)?;
    Ok(IdentityBoard {
        identity: identity.into(),
        address: mailspace.address_for(identity),
        actionable_open: tasks.len() + needs.len(),
        wants_hidden: wants_count.saturating_sub(wants.len()),
        tasks,
        needs,
        wants,
    })
}

fn board_items(
    storage: &Storage,
    messages: Vec<StoredMessageView>,
    cap: Option<usize>,
    since: Option<DateTime<Utc>>,
) -> Result<Vec<BoardItem>, VivariumError> {
    board_items_with_count(storage, messages, cap, since).map(|(items, _)| items)
}

fn board_items_with_count(
    storage: &Storage,
    messages: Vec<StoredMessageView>,
    cap: Option<usize>,
    since: Option<DateTime<Utc>>,
) -> Result<(Vec<BoardItem>, usize), VivariumError> {
    let limit = cap.unwrap_or(usize::MAX);
    let mut items = Vec::new();
    let mut matching = 0;
    for message in messages {
        let events = storage.list_mailspace_events(&message.message_id)?;
        if !changed_since(&message, &events, since) {
            continue;
        }
        matching += 1;
        if items.len() < limit {
            items.push(board_item(message, &events));
        }
    }
    Ok((items, matching))
}

fn resolve_since(command: &BoardCommand) -> Result<Option<DateTime<Utc>>, VivariumError> {
    if let Some(since) = &command.since {
        return vivarium::mailspace::parse_time_bound(since).map(Some);
    }
    let Some(path) = &command.watermark_file else {
        return Ok(None);
    };
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let value = raw.trim();
    if value.is_empty() {
        Ok(None)
    } else {
        vivarium::mailspace::parse_time_bound(value).map(Some)
    }
}

fn changed_since(
    message: &StoredMessageView,
    events: &[MailspaceEvent],
    since: Option<DateTime<Utc>>,
) -> bool {
    let Some(since) = since else {
        return true;
    };
    timestamp_at_or_after(&message.date, since)
        || events
            .iter()
            .any(|event| timestamp_at_or_after(&event.occurred_at, since))
}

fn timestamp_at_or_after(raw: &str, since: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(raw)
        .map(|date| date.with_timezone(&Utc) >= since)
        .unwrap_or(false)
}

fn write_watermark(command: &BoardCommand) -> Result<(), VivariumError> {
    if !command.write_watermark {
        return Ok(());
    }
    let Some(path) = &command.watermark_file else {
        return Ok(());
    };
    fs::write(path, Utc::now().to_rfc3339()).map_err(Into::into)
}

fn board_item(message: StoredMessageView, events: &[MailspaceEvent]) -> BoardItem {
    BoardItem {
        handle: message.handle,
        date: message.date,
        from: message.from_addr,
        subject: message.subject,
        last_event: events.last().map(board_event),
    }
}

fn board_event(event: &MailspaceEvent) -> BoardEvent {
    BoardEvent {
        occurred_at: event.occurred_at.clone(),
        command: event.command.clone(),
        event_type: event.event_type.clone(),
        note: event.note.clone(),
    }
}

fn board_totals(identities: &[IdentityBoard]) -> BoardTotals {
    let mut totals = BoardTotals::default();
    for identity in identities {
        totals.actionable_open += identity.actionable_open;
        totals.tasks_open += identity.tasks.len();
        totals.needs_open += identity.needs.len();
        totals.wants_open += identity.wants.len() + identity.wants_hidden;
        totals.wants_shown += identity.wants.len();
        totals.wants_hidden += identity.wants_hidden;
    }
    totals
}

fn print_json(board: &Board) -> Result<(), VivariumError> {
    println!(
        "{}",
        serde_json::to_string_pretty(board)
            .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
    );
    Ok(())
}

fn print_board(board: &Board) {
    println!("mailspace {}", board.name);
    println!("root      {}", board.root.display());
    println!(
        "actionable open: {}  tasks: {}  needs: {}  wants: {}",
        board.totals.actionable_open,
        board.totals.tasks_open,
        board.totals.needs_open,
        board.totals.wants_open
    );
    for identity in &board.identities {
        print_identity(identity);
    }
}

fn print_identity(identity: &IdentityBoard) {
    println!();
    println!(
        "{} <{}>  actionable:{}  tasks:{}  needs:{}  wants:{}",
        identity.identity,
        identity.address,
        identity.actionable_open,
        identity.tasks.len(),
        identity.needs.len(),
        identity.wants.len() + identity.wants_hidden
    );
    print_items("tasks", &identity.tasks);
    print_items("needs", &identity.needs);
    print_items("wants", &identity.wants);
    if identity.wants_hidden > 0 {
        println!("  wants hidden by cap: {}", identity.wants_hidden);
    }
}

fn print_items(label: &str, items: &[BoardItem]) {
    if items.is_empty() {
        return;
    }
    println!("{label}:");
    for item in items {
        println!(
            "  {}  {}  {}  {}",
            item.handle, item.date, item.from, item.subject
        );
    }
}
