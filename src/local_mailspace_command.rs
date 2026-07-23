use vivarium::VivariumError;
use vivarium::cli::{
    Command, CycleCommand, GraphCommand, GraphImportCommand, GraphShowCommand, LocalSendCommand,
    MailAbsorbStatus, MailCommand, MailDumpCommand, MailReplyCommand, MailspaceCommand,
    MailspaceIdentityCommand, MailspaceImportCommand, MailspaceWatchCommand, MemoCommand,
    TaskCommand, TaskSendCommand, TraceCommand,
};
use vivarium::mailspace::{
    DumpFilters, MailAbsorbFilter, MailDumpRequest, Mailspace, MailspaceWatchRequest, SendRequest,
    SourceTaskRequest,
};
use vivarium::message;
use vivarium::storage::StoredMessageView;

pub(crate) fn run_mailspace_command(command: &Command) -> Result<bool, VivariumError> {
    match command {
        Command::Mailspace { command } => {
            handle_mailspace_command(command)?;
            Ok(true)
        }
        Command::Board(command) => {
            crate::local_board_command::handle_board_command(command)?;
            Ok(true)
        }
        Command::Mail { command } => {
            handle_mail_command(command)?;
            Ok(true)
        }
        Command::Task { command } => {
            handle_task_command(command)?;
            Ok(true)
        }
        Command::Need { command } => {
            crate::local_work_command::handle_need_command(command)?;
            Ok(true)
        }
        Command::Want { command } => {
            crate::local_work_command::handle_want_command(command)?;
            Ok(true)
        }
        Command::Memo { command } => {
            handle_memo_command(command)?;
            Ok(true)
        }
        Command::Role { command } => {
            crate::local_role_command::handle_role_command(command)?;
            Ok(true)
        }
        Command::Cycle { command } => {
            handle_cycle_command(command)?;
            Ok(true)
        }
        Command::Trace(command) => {
            handle_trace_command(command)?;
            Ok(true)
        }
        Command::Graph { command } => {
            handle_graph_command(command)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn handle_graph_command(command: &GraphCommand) -> Result<(), VivariumError> {
    match command {
        GraphCommand::Import(command) => handle_graph_import(command),
        GraphCommand::Show(command) => handle_graph_show(command),
    }
}

fn handle_graph_import(command: &GraphImportCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let report = mailspace.graph_import_file(&command.code, &command.file, command.check)?;
    vivarium::mailspace::print_import_report(&report, command.json)
}

fn handle_graph_show(command: &GraphShowCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let show = mailspace.graph_show(&command.graph)?;
    vivarium::mailspace::print_graph_show(&show, command.json)
}

fn handle_mailspace_command(command: &MailspaceCommand) -> Result<(), VivariumError> {
    match command {
        MailspaceCommand::Init { project } => {
            let mailspace = Mailspace::init(project.as_deref())?;
            println!("mailspace {}", mailspace.config.name);
            println!("root      {}", mailspace.root.display());
            println!("store     {}", mailspace.store_path().display());
        }
        MailspaceCommand::Status { project, json } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            let status = mailspace.status()?;
            if *json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&status).map_err(|e| {
                        VivariumError::Other(format!("failed to encode JSON: {e}"))
                    })?
                );
            } else {
                vivarium::mailspace::print_status(&status);
            }
        }
        MailspaceCommand::Description { project, set } => {
            let mut mailspace = Mailspace::discover(project.as_deref())?;
            if let Some(description) = set {
                mailspace.set_description(Some(description.clone()))?;
                println!("description set");
            } else {
                let description = mailspace.config.description.as_deref().unwrap_or("(none)");
                println!("{description}");
            }
        }
        MailspaceCommand::Watch(command) => run_watch(command, None)?,
        MailspaceCommand::Import(command) | MailspaceCommand::Merge(command) => {
            import_mailspace(command)?;
        }
        MailspaceCommand::Identity { command } => handle_mailspace_identity_command(command)?,
    }
    Ok(())
}

fn handle_mailspace_identity_command(
    command: &MailspaceIdentityCommand,
) -> Result<(), VivariumError> {
    match command {
        MailspaceIdentityCommand::Add { identity, project } => {
            let mut mailspace = Mailspace::discover(project.as_deref())?;
            let address = mailspace.add_identity(identity)?;
            println!("added {address}");
        }
        MailspaceIdentityCommand::List { project } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            for identity in &mailspace.config.identities {
                println!(
                    "{} {}",
                    identity.name,
                    mailspace.address_for(&identity.name)
                );
                if !identity.aliases.is_empty() {
                    println!("  formerly: {}", identity.aliases.join(", "));
                }
            }
        }
        MailspaceIdentityCommand::Rename { old, new, project } => {
            let mut mailspace = Mailspace::discover(project.as_deref())?;
            let address = mailspace.rename_identity(old, new)?;
            println!("renamed {old} -> {new} ({address})");
            println!("historical mail sent as {old} still resolves under {new}");
        }
    }
    Ok(())
}

fn import_mailspace(command: &MailspaceImportCommand) -> Result<(), VivariumError> {
    let target = Mailspace::discover(command.project.as_deref())?;
    let report = vivarium::mailspace::import_mailspace(
        &target,
        &command.from,
        vivarium::mailspace::MailspaceImportOptions {
            dry_run: command.dry_run,
        },
    )?;
    if command.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    let mode = if report.dry_run { "dry run" } else { "applied" };
    println!("mailspace import {mode}");
    println!("source    {}", report.source.display());
    println!("target    {}", report.target.display());
    println!(
        "messages  scanned={} imported={} deduped={}",
        report.scanned_messages, report.imported_messages, report.deduped_messages
    );
    println!(
        "blobs     imported={} deduped={}",
        report.imported_blobs, report.deduped_blobs
    );
    println!(
        "events    imported={} deduped={}",
        report.imported_events, report.deduped_events
    );
    println!(
        "links     imported={} deduped={}",
        report.imported_links, report.deduped_links
    );
    if !report.conflicts.is_empty() {
        println!("conflicts {}", report.conflicts.len());
        for conflict in &report.conflicts {
            println!("  {conflict}");
        }
    }
    Ok(())
}

fn handle_cycle_command(command: &CycleCommand) -> Result<(), VivariumError> {
    match command {
        CycleCommand::Intake {
            for_identity,
            cursor_file,
            write_cursor,
            json,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            let intake =
                mailspace.cycle_intake(for_identity, cursor_file.as_deref(), *write_cursor)?;
            if *json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&intake)
                        .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
                );
            } else {
                print_cycle_intake(&intake);
            }
        }
    }
    Ok(())
}

fn print_cycle_intake(intake: &vivarium::mailspace::CycleIntake) {
    println!("cursor {} -> {}", intake.cursor, intake.next_cursor);
    println!("unabsorbed_mail {}", intake.unabsorbed_mail.len());
    println!("completed_tasks {}", intake.completed_tasks.len());
    println!("open_needs {}", intake.open_needs.len());
    println!("open_wants {}", intake.open_wants.len());
}

fn handle_mail_command(command: &MailCommand) -> Result<(), VivariumError> {
    match command {
        MailCommand::Send(command) => send_local_mail(command)?,
        MailCommand::Watch(command) => run_watch(command, Some("mail"))?,
        MailCommand::Reply(command) => reply_local_mail(command)?,
        MailCommand::Deliver {
            path,
            folder,
            project,
        } => deliver_local_mail(path, folder, project.as_deref())?,
        MailCommand::List {
            for_identity,
            folder,
            status,
            absorbed_by,
            json,
            project,
        } => list_local_mail(
            for_identity,
            folder,
            *status,
            absorbed_by.as_ref(),
            *json,
            project.as_deref(),
        )?,
        MailCommand::Show {
            handles,
            json,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            print_local_messages(&mailspace, handles, *json)?;
        }
        MailCommand::Thread(command) => {
            print_local_thread(command)?;
        }
        MailCommand::Absorb {
            handle,
            for_identity,
            note,
            project,
        } => absorb_local_mail(handle, for_identity, note.as_ref(), project.as_deref())?,
        MailCommand::Dump(command) => {
            let mailspace = Mailspace::discover(command.project.as_deref())?;
            let records = mailspace.dump_mail(mail_dump_request(command))?;
            crate::local_mailspace_dump::write_dump(
                "Vivi Mail Dump",
                &records,
                command.json,
                command.output.as_deref(),
            )?;
        }
    }
    Ok(())
}

fn print_local_thread(command: &vivarium::cli::MailThreadCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    vivarium::mailspace::print_thread(
        &mailspace,
        &command.handle,
        command.infer,
        command.limit,
        command.max_depth,
        command.json,
    )
}

fn handle_trace_command(command: &TraceCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    vivarium::mailspace::print_trace(
        &mailspace,
        &command.handle,
        command.max_depth,
        command.limit,
        command.json,
    )
}

fn deliver_local_mail(
    path: &std::path::Path,
    folder: &str,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let data = std::fs::read(path)?;
    for delivered in mailspace.deliver_raw(&data, folder)? {
        println!("delivered {} {}", delivered.identity, delivered.handle);
    }
    Ok(())
}

fn list_local_mail(
    for_identity: &str,
    folder: &str,
    status: MailAbsorbStatus,
    absorbed_by: Option<&String>,
    json: bool,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    crate::local_mail_list::print_mail_list(
        &mailspace,
        for_identity,
        folder,
        mail_absorb_filter(status),
        absorbed_by,
        json,
    )
}

fn absorb_local_mail(
    handle: &str,
    for_identity: &str,
    note: Option<&String>,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let handle = mailspace.absorb_mail(for_identity, handle, note.map(String::as_str))?;
    println!("absorbed {handle}");
    Ok(())
}

fn handle_memo_command(command: &MemoCommand) -> Result<(), VivariumError> {
    match command {
        MemoCommand::Save(command) => {
            let mailspace = Mailspace::discover(command.project.as_deref())?;
            let body = vivarium::mailspace::read_body_input(
                command.body.as_deref(),
                command.body_file.as_deref(),
            )?;
            let handle = mailspace.save_memo(&command.for_identity, &command.subject, &body)?;
            println!("saved {handle}");
        }
        MemoCommand::Delete {
            handle,
            for_identity,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            let handle =
                mailspace.move_item(for_identity, handle, "trash", None, "memo delete", None)?;
            println!("deleted {handle}");
        }
        MemoCommand::List {
            for_identity,
            json,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            print_memo_list(&mailspace, for_identity, *json)?;
        }
        MemoCommand::Search {
            query,
            for_identity,
            subject,
            json,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            let memos = mailspace.search_memos(for_identity, query, *subject)?;
            print_memo_list_items(&memos, *json)?;
        }
        MemoCommand::Show {
            handle,
            json,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            vivarium::mailspace::print_thread(&mailspace, handle, false, 50, 50, *json)?;
        }
        MemoCommand::Dump {
            for_identity,
            json,
            output,
            project,
        } => dump_memos(for_identity, *json, output.as_deref(), project.as_deref())?,
    }
    Ok(())
}

fn print_memo_list(mailspace: &Mailspace, identity: &str, json: bool) -> Result<(), VivariumError> {
    let memos = mailspace.list_kind(identity, "memos", "memo")?;
    print_memo_list_items(&memos, json)
}

fn print_memo_list_items(memos: &[StoredMessageView], json: bool) -> Result<(), VivariumError> {
    if json {
        let items: Vec<serde_json::Value> = memos
            .iter()
            .map(|m| {
                serde_json::json!({
                    "handle": m.handle,
                    "date": m.date,
                    "subject": m.subject,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&items)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    if memos.is_empty() {
        println!("  no memos");
        return Ok(());
    }
    println!("  handle  date  subject");
    for memo in memos {
        println!("  {}  {}  {}", memo.handle, memo.date, memo.subject);
    }
    Ok(())
}

fn dump_memos(
    for_identity: &str,
    json: bool,
    output: Option<&std::path::Path>,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let request = MailDumpRequest {
        folder: "memos".into(),
        kind: Some("memo".into()),
        filters: DumpFilters {
            for_identity: Some(for_identity.into()),
            ..Default::default()
        },
    };
    let records = mailspace.dump_mail(request)?;
    crate::local_mailspace_dump::write_dump("Vivi Memo Dump", &records, json, output)
}

fn send_local_mail(command: &LocalSendCommand) -> Result<(), VivariumError> {
    send_mail(command, "inbox", "mail", "delivered")
}

fn handle_task_command(command: &TaskCommand) -> Result<(), VivariumError> {
    match command {
        TaskCommand::Send(command) => send_task(command)?,
        TaskCommand::From(command) => task_from_source(command)?,
        TaskCommand::Watch(command) => run_watch(command, Some("task"))?,
        TaskCommand::List {
            for_identity,
            status,
            blocked,
            blocking,
            json,
            project,
        } => list_tasks_command(
            for_identity,
            status,
            *blocked,
            blocking.as_deref(),
            *json,
            project.as_deref(),
        )?,
        TaskCommand::Show {
            handle,
            json,
            project,
        } => show_task(handle, *json, project.as_deref())?,
        TaskCommand::Dump(command) => crate::local_work_command::dump_tasks(command)?,
        TaskCommand::Done {
            handle,
            for_identity,
            note,
            verdict,
            repo,
            tip,
            project,
        } => done_task(
            handle,
            for_identity,
            note.as_deref(),
            verdict.as_deref(),
            repo,
            tip,
            project.as_deref(),
        )?,
        TaskCommand::Reopen {
            handle,
            for_identity,
            note,
            project,
        } => reopen_task(handle, for_identity, note.as_deref(), project.as_deref())?,
    }
    Ok(())
}

fn done_task(
    handle: &str,
    for_identity: &str,
    note: Option<&str>,
    verdict: Option<&str>,
    repo: &[String],
    tip: &[String],
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    move_task(
        handle,
        for_identity,
        note,
        project,
        "done",
        verdict,
        repo,
        tip,
    )
}

fn reopen_task(
    handle: &str,
    for_identity: &str,
    note: Option<&str>,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    move_task(handle, for_identity, note, project, "tasks", None, &[], &[])
}

fn list_tasks_command(
    for_identity: &str,
    status: &vivarium::cli::TaskStatus,
    blocked: bool,
    blocking: Option<&str>,
    json: bool,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    if blocked || blocking.is_some() {
        list_tasks_with_deps(for_identity, blocked, blocking, json, project)
    } else {
        list_tasks(for_identity, status, json, project)
    }
}

fn list_tasks(
    for_identity: &str,
    status: &vivarium::cli::TaskStatus,
    json: bool,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    crate::local_work_list::print_work_list(
        &mailspace,
        for_identity,
        match status {
            vivarium::cli::TaskStatus::Open => "tasks",
            vivarium::cli::TaskStatus::Done => "done",
        },
        "task",
        json,
    )?;
    Ok(())
}

fn list_tasks_with_deps(
    for_identity: &str,
    blocked: bool,
    blocking: Option<&str>,
    json: bool,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let tasks = mailspace.list_tasks_with_deps(for_identity, blocked, blocking)?;
    crate::local_work_list::print_task_list(&mailspace, &tasks, json)
}

fn show_task(
    handle: &str,
    json: bool,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    vivarium::mailspace::print_thread(&mailspace, handle, false, 50, 50, json)?;
    Ok(())
}

fn task_from_source(command: &vivarium::cli::TaskFromCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let result = mailspace.task_from_source(SourceTaskRequest {
        source_handle: command.handle.clone(),
        actor: command.for_identity.clone(),
        to: command.to.clone(),
        cc: command.cc.clone(),
        subject: command.subject.clone(),
        body: vivarium::mailspace::read_body_input(
            command.body.as_deref(),
            command.body_file.as_deref(),
        )?,
    })?;
    for delivered in result.delivered {
        println!("created {} {}", delivered.identity, delivered.handle);
    }
    println!("source {} {}", result.source_kind, result.source_handle);
    println!("sent {}", result.sent);
    Ok(())
}

fn send_task(command: &TaskSendCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.send.project.as_deref())?;
    let result = mailspace.send(SendRequest {
        from: command.send.from.clone(),
        to: command.send.to.clone(),
        cc: command.send.cc.clone(),
        bcc: command.send.bcc.clone(),
        subject: command.send.subject.clone(),
        body: vivarium::mailspace::read_body_input(
            command.send.body.as_deref(),
            command.send.body_file.as_deref(),
        )?,
        role: "tasks".into(),
        kind: Some("task".into()),
        reply_to: command.send.reply_to.clone(),
        depends_on: command.depends_on.clone(),
    })?;
    for delivered in result.delivered {
        println!("created {} {}", delivered.identity, delivered.handle);
    }
    println!("sent {}", result.sent);
    Ok(())
}

fn send_mail(
    command: &LocalSendCommand,
    role: &str,
    kind: &str,
    delivered_label: &str,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let result = mailspace.send(SendRequest {
        from: command.from.clone(),
        to: command.to.clone(),
        cc: command.cc.clone(),
        bcc: command.bcc.clone(),
        subject: command.subject.clone(),
        body: vivarium::mailspace::read_body_input(
            command.body.as_deref(),
            command.body_file.as_deref(),
        )?,
        role: role.into(),
        kind: Some(kind.into()),
        reply_to: command.reply_to.clone(),
        depends_on: Vec::new(),
    })?;
    for delivered in result.delivered {
        println!(
            "{delivered_label} {} {}",
            delivered.identity, delivered.handle
        );
    }
    println!("sent {}", result.sent);
    Ok(())
}

fn reply_local_mail(command: &MailReplyCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let result = mailspace.reply(
        &command.handle,
        &command.from,
        command.to.clone(),
        command.cc.clone(),
        command.subject.clone(),
        vivarium::mailspace::read_body_input(
            command.body.as_deref(),
            command.body_file.as_deref(),
        )?,
    )?;
    for delivered in result.delivered {
        println!("replied {} {}", delivered.identity, delivered.handle);
    }
    println!("sent {}", result.sent);
    Ok(())
}

pub(crate) fn run_watch(
    command: &MailspaceWatchCommand,
    alias_kind: Option<&str>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let request = MailspaceWatchRequest {
        for_identity: command.for_identity.clone(),
        kinds: alias_kind.unwrap_or(&command.kinds).to_string(),
        events: command.events.clone(),
        statuses: command.statuses.clone(),
        match_from: command.match_from.clone(),
        match_subject_prefix: command.match_subject_prefix.clone(),
        handle: command.handle.clone(),
        until_count: command.until_count,
        timeout: command.timeout.clone(),
        once: command.once,
        since: command.since.clone(),
        cursor_file: command
            .cursor_file
            .clone()
            .or_else(|| command.watermark_file.clone()),
        write_cursor: command.write_cursor || command.write_watermark,
        poll_interval: command.poll_interval.clone(),
        json: command.json,
    };
    vivarium::mailspace::run_watch(&mailspace, request)
}

#[allow(clippy::too_many_arguments)]
fn move_task(
    handle: &str,
    for_identity: &str,
    note: Option<&str>,
    project: Option<&std::path::Path>,
    role: &str,
    verdict: Option<&str>,
    repo: &[String],
    tip: &[String],
) -> Result<(), VivariumError> {
    if repo.len() != tip.len() {
        return Err(VivariumError::Other(
            "--repo and --tip must be provided in matching pairs".into(),
        ));
    }
    let mailspace = Mailspace::discover(project)?;
    let handle = mailspace.move_task(for_identity, handle, role, note, verdict, repo, tip)?;
    let verb = if role == "done" { "done" } else { "reopened" };
    println!("{verb} {handle}");
    Ok(())
}

fn print_local_messages(
    mailspace: &Mailspace,
    handles: &[String],
    as_json: bool,
) -> Result<(), VivariumError> {
    let storage = mailspace.storage()?;
    if as_json {
        let mut messages = Vec::new();
        for handle in handles {
            let resolved = storage.resolve_message_token(handle)?;
            let data = storage.read_message(&resolved)?;
            let display_handle = storage.display_handle(&resolved)?;
            messages.push(message::to_json_message(&display_handle, &data)?);
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&messages)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    for (i, handle) in handles.iter().enumerate() {
        if i > 0 {
            println!("\n---\n");
        }
        let data = storage.read_message(handle)?;
        println!("{}", message::render_message(&data)?);
    }
    Ok(())
}

fn mail_dump_request(command: &MailDumpCommand) -> MailDumpRequest {
    MailDumpRequest {
        folder: command.folder.clone(),
        kind: Some("mail".into()),
        filters: dump_filters(command),
    }
}

fn mail_absorb_filter(status: MailAbsorbStatus) -> MailAbsorbFilter {
    match status {
        MailAbsorbStatus::All => MailAbsorbFilter::All,
        MailAbsorbStatus::Absorbed => MailAbsorbFilter::Absorbed,
        MailAbsorbStatus::Unabsorbed => MailAbsorbFilter::Unabsorbed,
    }
}

fn dump_filters(command: &MailDumpCommand) -> DumpFilters {
    DumpFilters {
        for_identity: command.for_identity.clone(),
        from: command.from.clone(),
        to: command.to.clone(),
        participant: command.participant.clone(),
        subject: command.subject.clone(),
        body: command.body.clone(),
        since: command.since.clone(),
        before: command.before.clone(),
        absorb_status: mail_absorb_filter(command.status),
        absorbed_by: command.absorbed_by.clone(),
    }
}
