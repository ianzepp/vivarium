use vivarium::VivariumError;
use vivarium::cli::{
    Command, CycleCommand, GraphActivateCommand, GraphApplyCommand, GraphCommand,
    GraphCompleteCommand, GraphEdgeCommand, GraphExportCommand, GraphImportCommand,
    GraphNodeCommand, GraphShowCommand, LocalSendCommand, MailAbsorbStatus, MailCommand,
    MailDumpCommand, MailListCommand, MailReplyCommand, MailspaceCommand, MailspaceIdentityCommand,
    MailspaceImportCommand, MemoCommand, TaskCommand, TaskSendCommand, TraceCommand,
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
        Command::Goal { command } => {
            crate::local_goal_command::handle_goal_command(command)?;
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
        Command::Step {
            apply,
            project,
            json,
        } => {
            crate::local_step_command::handle_step_command(
                apply.as_deref(),
                project.as_deref(),
                *json,
            )?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn handle_graph_command(command: &GraphCommand) -> Result<(), VivariumError> {
    match command {
        GraphCommand::Import(command) => handle_graph_import(command),
        GraphCommand::Apply(command) => handle_graph_apply(command),
        GraphCommand::Show(command) => handle_graph_show(command),
        GraphCommand::Export(command) => handle_graph_export(command),
        GraphCommand::Ready(command) => handle_graph_ready(command),
        GraphCommand::Complete(command) => handle_graph_complete(command),
        GraphCommand::Activate(command) => handle_graph_activate(command),
        GraphCommand::Node { command } => handle_graph_node_command(command),
        GraphCommand::Edge { command } => handle_graph_edge_command(command),
    }
}

fn handle_graph_import(command: &GraphImportCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let report = mailspace.graph_import_file(&command.code, &command.file, command.check)?;
    vivarium::mailspace::print_import_report(&report, command.json, command.confirm_large)
}

fn handle_graph_apply(command: &GraphApplyCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let report = mailspace.graph_apply_file(&command.graph, &command.file, command.check)?;
    vivarium::mailspace::print_apply_report(&report, command.json, command.confirm_large)
}

fn handle_graph_show(command: &GraphShowCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let mermaid = mailspace.graph_export_mermaid(&command.graph, command.include_state)?;
    print!("{mermaid}");
    Ok(())
}

fn handle_graph_export(command: &GraphExportCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let mermaid = mailspace.graph_export_mermaid(&command.graph, command.include_state)?;
    print!("{mermaid}");
    Ok(())
}

fn handle_graph_ready(command: &vivarium::cli::GraphReadyCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    if let Some(graph) = command.graph.as_deref() {
        let show = mailspace.graph_show(graph)?;
        let frontier = vivarium::mailspace::frontier_from_show(&show);
        return vivarium::mailspace::print_frontier(&frontier, command.json, command.confirm_large);
    }
    let shows = mailspace.graph_board_summaries()?;
    let frontiers: Vec<_> = shows
        .iter()
        .map(vivarium::mailspace::frontier_from_show)
        .collect();
    vivarium::mailspace::print_frontiers(&frontiers, command.json, command.confirm_large)
}

fn handle_graph_complete(command: &GraphCompleteCommand) -> Result<(), VivariumError> {
    if command.task.is_some() {
        eprintln!("note: --task is ignored by complete; task binding happens at graph activate");
    }
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let (graph, source_id) = split_graph_node(&command.node, command.graph.as_deref())?;
    let show = mailspace.graph_complete(&graph, &source_id, command.note.as_deref())?;
    let receipt =
        vivarium::mailspace::action_receipt_from_show("complete", &show, Some(&source_id), None);
    vivarium::mailspace::print_action_receipt(&receipt, command.json, command.confirm_large)
}

fn handle_graph_activate(command: &GraphActivateCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let (graph, source_id) = split_graph_node(&command.node, command.graph.as_deref())?;
    let show =
        mailspace.graph_activate(&graph, &source_id, &command.task, command.note.as_deref())?;
    let mut receipt = vivarium::mailspace::action_receipt_from_show(
        "activate",
        &show,
        Some(&source_id),
        Some(&command.task),
    );
    receipt.content = mailspace.content_hash_of(&command.task).ok();
    vivarium::mailspace::print_action_receipt(&receipt, command.json, command.confirm_large)
}

fn handle_graph_node_command(command: &GraphNodeCommand) -> Result<(), VivariumError> {
    match command {
        GraphNodeCommand::Add(command) => {
            let mailspace = Mailspace::discover(command.project.as_deref())?;
            let show = mailspace.graph_node_add(
                &command.graph,
                &command.id,
                command.label.as_deref(),
                command.kind.as_deref(),
            )?;
            let receipt = vivarium::mailspace::action_receipt_from_show(
                "node_add",
                &show,
                Some(&command.id),
                None,
            );
            vivarium::mailspace::print_action_receipt(&receipt, command.json, command.confirm_large)
        }
    }
}

fn handle_graph_edge_command(command: &GraphEdgeCommand) -> Result<(), VivariumError> {
    match command {
        GraphEdgeCommand::Add(command) => {
            let mailspace = Mailspace::discover(command.project.as_deref())?;
            let show = mailspace.graph_edge_add(
                &command.graph,
                &command.from,
                &command.to,
                command.label.as_deref(),
            )?;
            let node = format!("{}->{}", command.from, command.to);
            let receipt =
                vivarium::mailspace::action_receipt_from_show("edge_add", &show, Some(&node), None);
            vivarium::mailspace::print_action_receipt(&receipt, command.json, command.confirm_large)
        }
    }
}

fn split_graph_node(
    node: &str,
    graph_flag: Option<&str>,
) -> Result<(String, String), VivariumError> {
    if let Some((graph, source_id)) = node.split_once(':')
        && !graph.is_empty()
        && !source_id.is_empty()
    {
        return Ok((graph.to_string(), source_id.to_string()));
    }
    // Bare source ids address the backlog graph: its ids are mailspace
    // handles, so the common dispatch path needs no prefix.
    let graph = graph_flag
        .map(str::to_string)
        .unwrap_or_else(|| "backlog".to_string());
    Ok((graph, node.to_string()))
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
        MailspaceCommand::Archive {
            project,
            set,
            clear,
            command,
        } => handle_archive_command(project.as_deref(), set.as_deref(), *clear, command.as_ref())?,
        MailspaceCommand::Watch(command) => run_watch(&command.common, &command.kinds)?,
        MailspaceCommand::Import(command) | MailspaceCommand::Merge(command) => {
            import_mailspace(command)?;
        }
        MailspaceCommand::Identity { command } => handle_mailspace_identity_command(command)?,
    }
    Ok(())
}

pub(crate) fn handle_archive_command(
    project: Option<&std::path::Path>,
    set: Option<&str>,
    clear: bool,
    command: Option<&vivarium::cli::MailspaceArchiveCommand>,
) -> Result<(), VivariumError> {
    if let Some(vivarium::cli::MailspaceArchiveCommand::Export {
        project: export_project,
        json,
    }) = command
    {
        return export_archive(export_project.as_deref().or(project), *json);
    }
    let mut mailspace = Mailspace::discover(project)?;
    if clear {
        mailspace.set_archive(None)?;
        println!("archive cleared");
        return Ok(());
    }
    if let Some(path) = set {
        mailspace.set_archive(Some(path.to_string()))?;
        println!("archive set");
        return Ok(());
    }
    println!(
        "{}",
        mailspace.config.archive.as_deref().unwrap_or("(none)")
    );
    Ok(())
}

pub(crate) fn export_archive(
    project: Option<&std::path::Path>,
    json: bool,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let report = mailspace.export_archive()?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    println!(
        "scanned {}  written {}  unchanged {}",
        report.scanned, report.written, report.unchanged
    );
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
        MailCommand::Watch(command) => run_watch(&command.common, "mail")?,
        MailCommand::Reply(command) => reply_local_mail(command)?,
        MailCommand::Deliver {
            path,
            folder,
            project,
        } => deliver_local_mail(path, folder, project.as_deref())?,
        MailCommand::List(command) => list_local_mail(command)?,
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
        MailCommand::Absorb(command) => absorb_record("mail", command)?,
        MailCommand::Dump(command) => {
            let mailspace = Mailspace::discover(command.project.as_deref())?;
            let records = mailspace.dump_mail(mail_dump_request(command))?;
            crate::local_mailspace_dump::write_dump(
                "Vivi Mail Dump",
                &records,
                command.json,
                command.output.as_deref(),
                command.confirm_large,
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

fn list_local_mail(command: &MailListCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    crate::local_mail_list::print_mail_list(&mailspace, command, mail_absorb_filter(command.status))
}

pub(crate) fn absorb_record(
    kind: &str,
    command: &vivarium::cli::AbsorbCommand,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let handle = mailspace.absorb(
        &command.for_identity,
        &command.handle,
        command.note.as_deref(),
        kind,
    )?;
    println!("absorbed {handle}");
    Ok(())
}

#[rustfmt::skip]
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
        MemoCommand::Absorb(command) => absorb_record("memo", command)?,
        MemoCommand::Delete { handle, for_identity, project } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            let handle = mailspace.move_item(for_identity, handle, "trash", None, "memo delete", None)?;
            println!("deleted {handle}");
        }
        MemoCommand::List { for_identity, json, project } => {
            print_memo_list(&Mailspace::discover(project.as_deref())?, for_identity, *json)?;
        }
        MemoCommand::Search { query, for_identity, subject, json, project } => {
            let memos = Mailspace::discover(project.as_deref())?.search_memos(for_identity, query, *subject)?;
            print_memo_list_items(&memos, *json)?;
        }
        MemoCommand::Show { handle, json, project } => {
            vivarium::mailspace::print_thread(
                &Mailspace::discover(project.as_deref())?,
                handle,
                false,
                50,
                50,
                *json,
            )?;
        }
        MemoCommand::Dump { for_identity, json, output, confirm_large, project } => {
            dump_memos(for_identity, *json, output.as_deref(), *confirm_large, project.as_deref())?;
        }
    }
    Ok(())
}

fn print_memo_list(mailspace: &Mailspace, identity: &str, json: bool) -> Result<(), VivariumError> {
    let memos = mailspace.list_kind(Some(identity), "memos", "memo")?;
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
    confirm_large: bool,
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
    crate::local_mailspace_dump::write_dump("Vivi Memo Dump", &records, json, output, confirm_large)
}

fn send_local_mail(command: &LocalSendCommand) -> Result<(), VivariumError> {
    send_mail(command, "inbox", "mail", "delivered")
}

fn handle_task_command(command: &TaskCommand) -> Result<(), VivariumError> {
    match command {
        TaskCommand::Send(command) => send_task(command)?,
        TaskCommand::From(command) => task_from_source(command)?,
        TaskCommand::Watch(command) => run_watch(&command.common, "task")?,
        TaskCommand::List { .. } => run_task_list(command)?,
        TaskCommand::Show {
            handle,
            json,
            project,
        } => show_task(handle, *json, project.as_deref())?,
        TaskCommand::Dump(command) => crate::local_work_command::dump_tasks(command)?,
        TaskCommand::Absorb(command) => absorb_record("task", command)?,
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

fn run_task_list(command: &TaskCommand) -> Result<(), VivariumError> {
    let TaskCommand::List {
        for_identity,
        from,
        to,
        status,
        blocked,
        blocking,
        json,
        project,
    } = command
    else {
        return Err(VivariumError::Message(
            "internal: run_task_list requires task list".into(),
        ));
    };
    if *blocked || blocking.is_some() {
        let Some(for_identity) = for_identity.as_deref() else {
            return Err(VivariumError::Message(
                "task list --blocked/--blocking requires --for".into(),
            ));
        };
        return list_tasks_with_deps(
            for_identity,
            *blocked,
            blocking.as_deref(),
            *json,
            project.as_deref(),
        );
    }
    list_tasks(
        for_identity.as_deref(),
        from.as_deref(),
        to.as_deref(),
        status,
        *json,
        project.as_deref(),
    )
}

fn list_tasks(
    for_identity: Option<&str>,
    from: Option<&str>,
    to: Option<&str>,
    status: &vivarium::cli::TaskStatus,
    json: bool,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let roles = match status {
        vivarium::cli::TaskStatus::Open => vec!["tasks"],
        vivarium::cli::TaskStatus::Done => vec!["done"],
        vivarium::cli::TaskStatus::All => vec!["tasks", "done"],
    };
    crate::local_work_list::print_work_lists(
        &mailspace,
        for_identity,
        &roles,
        "task",
        from,
        to,
        json,
    )
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
    let body = vivarium::mailspace::read_body_input(
        command.send.body.as_deref(),
        command.send.body_file.as_deref(),
    )?;
    let lacks_clause = !vivarium::mailspace::body_has_labeled_clause(&body, "done_when");
    let result = mailspace.send(SendRequest {
        from: command.send.from.clone(),
        to: command.send.to.clone(),
        cc: command.send.cc.clone(),
        bcc: command.send.bcc.clone(),
        subject: command.send.subject.clone(),
        body,
        role: "tasks".into(),
        kind: Some("task".into()),
        reply_to: command.send.reply_to.clone(),
        depends_on: command.depends_on.clone(),
    })?;
    for delivered in result.delivered {
        println!("created {} {}", delivered.identity, delivered.handle);
    }
    println!("sent {}", result.sent);
    if lacks_clause {
        eprintln!(
            "note: body declares no 'done_when:' clause; vivi step will report this task as \
             no_done_when"
        );
    }
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
    command: &vivarium::cli::WatchCommon,
    kinds: &str,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let request = MailspaceWatchRequest {
        for_identity: command.for_identity.clone(),
        kinds: kinds.to_string(),
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
