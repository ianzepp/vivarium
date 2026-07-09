use std::path::{Path, PathBuf};

use serde::Serialize;
use vivarium::VivariumError;
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

pub(crate) fn handle_board_command(
    for_identity: &Option<String>,
    project: Option<&Path>,
    wants_cap: usize,
    json: bool,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let board = build_board(&mailspace, for_identity.as_deref(), wants_cap)?;
    if json {
        print_json(&board)
    } else {
        print_board(&board);
        Ok(())
    }
}

fn build_board(
    mailspace: &Mailspace,
    for_identity: Option<&str>,
    wants_cap: usize,
) -> Result<Board, VivariumError> {
    let identities = board_identities(mailspace, for_identity)?;
    let storage = mailspace.storage()?;
    let mut boards = Vec::new();
    for identity in identities {
        boards.push(build_identity_board(
            mailspace, &storage, &identity, wants_cap,
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
) -> Result<IdentityBoard, VivariumError> {
    let tasks = board_items(
        storage,
        mailspace.list_kind(identity, "tasks", "task")?,
        None,
    )?;
    let needs = board_items(
        storage,
        mailspace.list_kind(identity, "needs", "need")?,
        None,
    )?;
    let wants_open = mailspace.list_kind(identity, "wants", "want")?;
    let wants_count = wants_open.len();
    let wants = board_items(storage, wants_open, Some(wants_cap))?;
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
) -> Result<Vec<BoardItem>, VivariumError> {
    let limit = cap.unwrap_or(usize::MAX);
    let mut items = Vec::new();
    for message in messages.into_iter().take(limit) {
        let events = storage.list_mailspace_events(&message.message_id)?;
        items.push(board_item(message, &events));
    }
    Ok(items)
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
