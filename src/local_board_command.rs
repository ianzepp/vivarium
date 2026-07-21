use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Serialize;
use vivarium::VivariumError;
use vivarium::cli::BoardCommand;
use vivarium::mailspace::Mailspace;
use vivarium::role_schedule::{ScheduleReport, ScheduleState, state_label as schedule_state_label};
use vivarium::role_status::ProcessReport;
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
    /// Live process status. Present only when the board is run with `--process`.
    #[serde(skip_serializing_if = "Option::is_none")]
    process: Option<ProcessReport>,
    /// Schedule health from role cadence and latest outbound signal.
    schedule: ScheduleReport,
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
        command.process,
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
    with_process: bool,
) -> Result<Board, VivariumError> {
    let identities = board_identities(mailspace, for_identity)?;
    let storage = mailspace.storage()?;
    let scopes = board_identity_scopes(mailspace, &identities);
    let account_identity = account_identity_map(&scopes);
    let accounts = unique_scope_accounts(&scopes);
    let roles = board_roles();
    let mut messages = storage.list_messages_by_account_roles_raw(&accounts, &roles)?;
    apply_scoped_handles(&storage, &scopes, &account_identity, &mut messages)?;
    let message_ids = messages
        .iter()
        .map(|message| message.message_id.clone())
        .collect::<Vec<_>>();
    let events_by_message = storage.list_mailspace_events_for_messages(&message_ids)?;
    let mut messages_by_identity = group_board_messages(messages, &account_identity);
    let mut boards = Vec::new();
    for identity in identities {
        boards.push(build_identity_board(
            mailspace,
            &identity,
            messages_by_identity.remove(&identity).unwrap_or_default(),
            &events_by_message,
            wants_cap,
            since,
            with_process,
        ));
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

fn board_identity_scopes(
    mailspace: &Mailspace,
    identities: &[String],
) -> Vec<(String, Vec<String>)> {
    identities
        .iter()
        .map(|identity| {
            let mut names = mailspace
                .identity_names(identity)
                .into_iter()
                .collect::<Vec<_>>();
            names.sort();
            (identity.clone(), names)
        })
        .collect()
}

fn account_identity_map(scopes: &[(String, Vec<String>)]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for (identity, accounts) in scopes {
        for account in accounts {
            map.insert(account.clone(), identity.clone());
        }
    }
    map
}

fn unique_scope_accounts(scopes: &[(String, Vec<String>)]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut accounts = Vec::new();
    for (_, scope_accounts) in scopes {
        for account in scope_accounts {
            if seen.insert(account.clone()) {
                accounts.push(account.clone());
            }
        }
    }
    accounts
}

fn board_roles() -> Vec<String> {
    ["tasks", "needs", "wants"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn apply_scoped_handles(
    storage: &Storage,
    scopes: &[(String, Vec<String>)],
    account_identity: &HashMap<String, String>,
    messages: &mut [StoredMessageView],
) -> Result<(), VivariumError> {
    let handles_by_identity = storage.display_handles_for_account_scopes(scopes)?;
    for message in messages {
        let Some(identity) = account_identity.get(&message.account) else {
            continue;
        };
        let Some(handles) = handles_by_identity.get(identity) else {
            continue;
        };
        if let Some(handle) = handles.get(&message.message_id) {
            message.handle = handle.clone();
        }
    }
    Ok(())
}

fn group_board_messages(
    messages: Vec<StoredMessageView>,
    account_identity: &HashMap<String, String>,
) -> HashMap<String, Vec<StoredMessageView>> {
    let mut grouped: HashMap<String, Vec<StoredMessageView>> = HashMap::new();
    for message in messages {
        if let Some(identity) = account_identity.get(&message.account) {
            grouped.entry(identity.clone()).or_default().push(message);
        }
    }
    grouped
}

fn build_identity_board(
    mailspace: &Mailspace,
    identity: &str,
    messages: Vec<StoredMessageView>,
    events_by_message: &HashMap<String, Vec<MailspaceEvent>>,
    wants_cap: usize,
    since: Option<DateTime<Utc>>,
    with_process: bool,
) -> IdentityBoard {
    let (tasks_open, needs_open, wants_open) = partition_board_messages(messages);
    let tasks = board_items(tasks_open, events_by_message, None, since);
    let needs = board_items(needs_open, events_by_message, None, since);
    let (wants, wants_count) =
        board_items_with_count(wants_open, events_by_message, Some(wants_cap), since);
    IdentityBoard {
        identity: identity.into(),
        address: mailspace.address_for(identity),
        actionable_open: tasks.len() + needs.len(),
        wants_hidden: wants_count.saturating_sub(wants.len()),
        process: role_process_status(mailspace, identity, with_process),
        schedule: role_schedule_status(mailspace, identity),
        tasks,
        needs,
        wants,
    }
}

fn role_schedule_status(mailspace: &Mailspace, identity: &str) -> ScheduleReport {
    mailspace
        .schedule_report(identity)
        .unwrap_or_else(|_| vivarium::role_schedule::evaluate(None, None, Utc::now()))
}

/// Look up a role's pid/host binding and probe it when `with_process` is set.
/// Reads the config record directly so the board does not pay charter file reads.
fn role_process_status(
    mailspace: &Mailspace,
    identity: &str,
    with_process: bool,
) -> Option<ProcessReport> {
    if !with_process {
        return None;
    }
    let role = mailspace
        .config
        .identities
        .iter()
        .find(|known| known.name == identity);
    let pid = role.and_then(|role| role.pid);
    let host = role.and_then(|role| role.host.as_deref());
    Some(vivarium::role_status::probe_quick(pid, host))
}

fn partition_board_messages(
    messages: Vec<StoredMessageView>,
) -> (
    Vec<StoredMessageView>,
    Vec<StoredMessageView>,
    Vec<StoredMessageView>,
) {
    let mut tasks = Vec::new();
    let mut needs = Vec::new();
    let mut wants = Vec::new();
    for message in messages {
        match message.local_role.as_str() {
            "tasks" => tasks.push(message),
            "needs" => needs.push(message),
            "wants" => wants.push(message),
            _ => {}
        }
    }
    (tasks, needs, wants)
}

fn board_items(
    messages: Vec<StoredMessageView>,
    events_by_message: &HashMap<String, Vec<MailspaceEvent>>,
    cap: Option<usize>,
    since: Option<DateTime<Utc>>,
) -> Vec<BoardItem> {
    board_items_with_count(messages, events_by_message, cap, since).0
}

fn board_items_with_count(
    messages: Vec<StoredMessageView>,
    events_by_message: &HashMap<String, Vec<MailspaceEvent>>,
    cap: Option<usize>,
    since: Option<DateTime<Utc>>,
) -> (Vec<BoardItem>, usize) {
    let limit = cap.unwrap_or(usize::MAX);
    let mut items = Vec::new();
    let mut matching = 0;
    for message in messages {
        let events = events_by_message
            .get(&message.message_id)
            .map_or([].as_slice(), Vec::as_slice);
        if !changed_since(&message, events, since) {
            continue;
        }
        matching += 1;
        if items.len() < limit {
            items.push(board_item(message, events));
        }
    }
    (items, matching)
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
    DateTime::parse_from_rfc3339(raw).is_ok_and(|date| date.with_timezone(&Utc) >= since)
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
    if let Some(process) = &identity.process {
        let running = match process.running {
            Some(true) => "yes",
            Some(false) => "no",
            None => "unknown",
        };
        let name = process
            .name
            .as_deref()
            .map(|name| format!("  ({name})"))
            .unwrap_or_default();
        println!(
            "  process: {} running:{}{name}",
            role_status_state_label(&process.state),
            running
        );
    }
    print_schedule_line(&identity.schedule);
    print_items("tasks", &identity.tasks);
    print_items("needs", &identity.needs);
    print_items("wants", &identity.wants);
    if identity.wants_hidden > 0 {
        println!("  wants hidden by cap: {}", identity.wants_hidden);
    }
}

fn role_status_state_label(state: &vivarium::role_status::ProcessState) -> &'static str {
    use vivarium::role_status::ProcessState;
    match state {
        ProcessState::NotSet => "not_set",
        ProcessState::Remote => "remote",
        ProcessState::Alive => "alive",
        ProcessState::Zombie => "zombie",
        ProcessState::Dead => "dead",
        ProcessState::Unknown => "unknown",
    }
}

fn print_schedule_line(schedule: &ScheduleReport) {
    if matches!(schedule.state, ScheduleState::None) {
        return;
    }
    print!("  schedule: {}", schedule_state_label(schedule.state));
    if let Some(cadence) = &schedule.cadence {
        print!("  cadence {cadence}");
    }
    match schedule.state {
        ScheduleState::Never => print!("  (no outbound signal)"),
        _ => {
            if let Some(age) = schedule.age_seconds {
                print!("  last signal {age}s ago");
            }
        }
    }
    if let Some(handle) = &schedule.last_signal_handle {
        print!("  {handle}");
    }
    println!();
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
