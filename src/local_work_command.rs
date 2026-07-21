use vivarium::VivariumError;
use vivarium::cli::{
    LocalSendCommand, MailDumpCommand, NeedCommand, TaskDumpCommand, TaskDumpStatusArg, TaskStatus,
    WantCommand, WantStatus,
};
use vivarium::mailspace::{
    DumpFilters, MailDumpRequest, Mailspace, SendRequest, TaskDumpRequest, WantListOptions,
    WantMetadataUpdate,
};

pub(crate) fn handle_need_command(command: &NeedCommand) -> Result<(), VivariumError> {
    match command {
        NeedCommand::Send(command) => send_local_item(command, "needs", "need", "created")?,
        NeedCommand::Watch(command) => {
            crate::local_mailspace_command::run_watch(command, Some("need"))?;
        }
        NeedCommand::List {
            for_identity,
            status,
            json,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            crate::local_work_list::print_work_list(
                &mailspace,
                for_identity,
                status_role(status, "needs"),
                "need",
                *json,
            )?;
        }
        NeedCommand::Show {
            handle,
            json,
            project,
        } => show_local_message(handle, *json, project.as_deref())?,
        NeedCommand::Dump(command) => dump_work_items(command, "needs", "need", "Vivi Need Dump")?,
        NeedCommand::Done {
            handle,
            for_identity,
            note,
            project,
        } => move_item(
            handle,
            for_identity,
            note.as_ref(),
            project.as_deref(),
            "done",
            "need done",
            "done",
        )?,
        NeedCommand::Reopen {
            handle,
            for_identity,
            note,
            project,
        } => move_item(
            handle,
            for_identity,
            note.as_ref(),
            project.as_deref(),
            "needs",
            "need reopen",
            "reopened",
        )?,
    }
    Ok(())
}

pub(crate) fn handle_want_command(command: &WantCommand) -> Result<(), VivariumError> {
    match command {
        WantCommand::Send(command) => send_local_item(command, "wants", "want", "created")?,
        WantCommand::Watch(command) => {
            crate::local_mailspace_command::run_watch(command, Some("want"))?;
        }
        WantCommand::List {
            for_identity,
            status,
            repo,
            lane,
            sort,
            json,
            project,
        } => list_wants(
            for_identity,
            status,
            WantListOptions {
                repo: repo.clone(),
                lane: lane.clone(),
                sort: sort.clone(),
            },
            *json,
            project.as_deref(),
        )?,
        WantCommand::Show {
            handle,
            json,
            project,
        } => show_local_message(handle, *json, project.as_deref())?,
        WantCommand::Dump(command) => dump_wants(command)?,
        WantCommand::SetPriority { .. } => set_want_priority(command)?,
        WantCommand::Promote {
            handle,
            for_identity,
            note,
            project,
        } => promote_want(handle, for_identity, note.as_ref(), project.as_deref())?,
        WantCommand::Done {
            handle,
            for_identity,
            note,
            project,
        } => close_want_done(handle, for_identity, note.as_ref(), project.as_deref())?,
        WantCommand::Drop {
            handle,
            for_identity,
            note,
            project,
        } => close_want_drop(handle, for_identity, note.as_ref(), project.as_deref())?,
    }
    Ok(())
}

fn promote_want(
    handle: &str,
    for_identity: &str,
    note: Option<&String>,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    move_item(
        handle,
        for_identity,
        note,
        project,
        "needs",
        "want promote",
        "promoted",
    )
}

fn set_want_priority(command: &WantCommand) -> Result<(), VivariumError> {
    let WantCommand::SetPriority {
        handle,
        for_identity,
        priority,
        rank,
        repo,
        lane,
        blocks_claim,
        reason,
        project,
    } = command
    else {
        unreachable!();
    };
    let mailspace = Mailspace::discover(project.as_deref())?;
    let handle = mailspace.set_want_metadata(
        for_identity,
        handle,
        WantMetadataUpdate {
            priority: priority.clone(),
            rank: *rank,
            repo: repo.clone(),
            lane: lane.clone(),
            blocks_claim: blocks_claim.clone(),
            reason: reason.clone(),
        },
    )?;
    println!("updated {handle}");
    Ok(())
}

fn list_wants(
    for_identity: &str,
    status: &WantStatus,
    options: WantListOptions,
    json: bool,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let records =
        mailspace.list_wants_with_metadata(for_identity, want_status_roles(status), options)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&records)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    if records.is_empty() {
        println!("  no wants");
        return Ok(());
    }
    println!("  handle  status  priority  rank  repo  lane  subject  active_tasks");
    for item in &records {
        println!(
            "  {}  {}  {}  {}  {}  {}  {}  {}",
            item.handle,
            item.status,
            item.metadata.get("priority").map_or("-", String::as_str),
            item.metadata.get("rank").map_or("-", String::as_str),
            item.metadata.get("repo").map_or("-", String::as_str),
            item.metadata.get("lane").map_or("-", String::as_str),
            item.subject,
            item.active_tasks.join(",")
        );
    }
    Ok(())
}

fn close_want(
    handle: &str,
    for_identity: &str,
    note: Option<&String>,
    project: Option<&std::path::Path>,
    command: &str,
    verb: &str,
) -> Result<(), VivariumError> {
    move_item(handle, for_identity, note, project, "done", command, verb)
}

fn close_want_done(
    handle: &str,
    for_identity: &str,
    note: Option<&String>,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    close_want(handle, for_identity, note, project, "want done", "done")
}

fn close_want_drop(
    handle: &str,
    for_identity: &str,
    note: Option<&String>,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    close_want(handle, for_identity, note, project, "want drop", "dropped")
}

pub(crate) fn dump_tasks(command: &TaskDumpCommand) -> Result<(), VivariumError> {
    dump_work_items(command, "tasks", "task", "Vivi Task Dump")
}

fn dump_work_items(
    command: &TaskDumpCommand,
    open_role: &str,
    kind: &str,
    title: &str,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let records = mailspace.dump_tasks(work_dump_request(command, open_role, kind))?;
    crate::local_mailspace_dump::write_dump(
        title,
        &records,
        command.json,
        command.output.as_deref(),
    )
}

fn dump_wants(command: &MailDumpCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let mut request = mail_dump_request(command);
    request.folder = "wants".into();
    request.kind = Some("want".into());
    let records = mailspace.dump_mail(request)?;
    crate::local_mailspace_dump::write_dump(
        "Vivi Want Dump",
        &records,
        command.json,
        command.output.as_deref(),
    )
}

fn send_local_item(
    command: &LocalSendCommand,
    role: &str,
    kind: &str,
    verb: &str,
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
        println!("{verb} {} {}", delivered.identity, delivered.handle);
    }
    println!("sent {}", result.sent);
    Ok(())
}

fn move_item(
    handle: &str,
    for_identity: &str,
    note: Option<&String>,
    project: Option<&std::path::Path>,
    role: &str,
    command: &str,
    verb: &str,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let handle = mailspace.move_item(
        for_identity,
        handle,
        role,
        note.map(String::as_str),
        command,
    )?;
    println!("{verb} {handle}");
    Ok(())
}

fn show_local_message(
    handle: &str,
    json: bool,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    vivarium::mailspace::print_thread(&mailspace, handle, false, 50, 50, json)
}

fn status_role(status: &TaskStatus, open_role: &'static str) -> &'static str {
    match status {
        TaskStatus::Open => open_role,
        TaskStatus::Done => "done",
    }
}

fn want_status_roles(status: &WantStatus) -> &'static [&'static str] {
    match status {
        WantStatus::Open => &["wants"],
        WantStatus::Done => &["done"],
        WantStatus::All => &["wants", "done"],
    }
}

fn mail_dump_request(command: &MailDumpCommand) -> MailDumpRequest {
    MailDumpRequest {
        folder: command.folder.clone(),
        kind: Some("mail".into()),
        filters: mail_dump_filters(command),
    }
}

fn work_dump_request(command: &TaskDumpCommand, open_role: &str, kind: &str) -> TaskDumpRequest {
    TaskDumpRequest {
        status: match command.status {
            TaskDumpStatusArg::Open => vivarium::mailspace::TaskDumpStatus::Open,
            TaskDumpStatusArg::Done => vivarium::mailspace::TaskDumpStatus::Done,
            TaskDumpStatusArg::All => vivarium::mailspace::TaskDumpStatus::All,
        },
        open_role: open_role.into(),
        kind: kind.into(),
        filters: task_dump_filters(command),
    }
}

fn mail_dump_filters(command: &MailDumpCommand) -> DumpFilters {
    DumpFilters {
        for_identity: command.for_identity.clone(),
        from: command.from.clone(),
        to: command.to.clone(),
        participant: command.participant.clone(),
        subject: command.subject.clone(),
        body: command.body.clone(),
        since: command.since.clone(),
        before: command.before.clone(),
        ..Default::default()
    }
}

fn task_dump_filters(command: &TaskDumpCommand) -> DumpFilters {
    DumpFilters {
        for_identity: command.for_identity.clone(),
        from: command.from.clone(),
        to: command.to.clone(),
        participant: command.participant.clone(),
        subject: command.subject.clone(),
        body: command.body.clone(),
        since: command.since.clone(),
        before: command.before.clone(),
        ..Default::default()
    }
}
