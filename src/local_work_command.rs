use vivarium::VivariumError;
use vivarium::cli::{
    LocalSendCommand, MailDumpCommand, NeedCommand, TaskDumpCommand, TaskDumpStatusArg, TaskStatus,
    WantCommand, WantStatus,
};
use vivarium::mailspace::{DumpFilters, MailDumpRequest, Mailspace, SendRequest, TaskDumpRequest};
use vivarium::message;

pub(crate) fn handle_need_command(command: &NeedCommand) -> Result<(), VivariumError> {
    match command {
        NeedCommand::Send(command) => send_local_item(command, "needs", "need", "created")?,
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
        NeedCommand::Show { handle, project } => show_local_message(handle, project.as_deref())?,
        NeedCommand::Dump(command) => dump_work_items(command, "needs", "need", "Vivi Need Dump")?,
        NeedCommand::Done {
            handle,
            for_identity,
            note,
            project,
        } => move_item(
            handle,
            for_identity,
            note,
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
            note,
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
        WantCommand::List {
            for_identity,
            status,
            json,
            project,
        } => list_wants(for_identity, status, *json, project.as_deref())?,
        WantCommand::Show { handle, project } => show_local_message(handle, project.as_deref())?,
        WantCommand::Dump(command) => dump_wants(command)?,
        WantCommand::Promote {
            handle,
            for_identity,
            note,
            project,
        } => move_item(
            handle,
            for_identity,
            note,
            project.as_deref(),
            "needs",
            "want promote",
            "promoted",
        )?,
        WantCommand::Done {
            handle,
            for_identity,
            note,
            project,
        } => close_want(
            handle,
            for_identity,
            note,
            project.as_deref(),
            "want done",
            "done",
        )?,
        WantCommand::Drop {
            handle,
            for_identity,
            note,
            project,
        } => close_want(
            handle,
            for_identity,
            note,
            project.as_deref(),
            "want drop",
            "dropped",
        )?,
    }
    Ok(())
}

fn list_wants(
    for_identity: &str,
    status: &WantStatus,
    json: bool,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    crate::local_work_list::print_work_lists(
        &mailspace,
        for_identity,
        want_status_roles(status),
        "want",
        json,
    )
}

fn close_want(
    handle: &str,
    for_identity: &str,
    note: &Option<String>,
    project: Option<&std::path::Path>,
    command: &str,
    verb: &str,
) -> Result<(), VivariumError> {
    move_item(handle, for_identity, note, project, "done", command, verb)
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
    note: &Option<String>,
    project: Option<&std::path::Path>,
    role: &str,
    command: &str,
    verb: &str,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let handle = mailspace.move_item(for_identity, handle, role, note.as_deref(), command)?;
    println!("{verb} {handle}");
    Ok(())
}

fn show_local_message(
    handle: &str,
    project: Option<&std::path::Path>,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let storage = mailspace.storage()?;
    let data = storage.read_message(handle)?;
    println!("{}", message::render_message(&data)?);
    Ok(())
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
        filters: dump_filters(
            &command.for_identity,
            &command.from,
            &command.to,
            &command.participant,
            &command.subject,
            &command.body,
            &command.since,
            &command.before,
        ),
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
        filters: dump_filters(
            &command.for_identity,
            &command.from,
            &command.to,
            &command.participant,
            &command.subject,
            &command.body,
            &command.since,
            &command.before,
        ),
    }
}

fn dump_filters(
    for_identity: &Option<String>,
    from: &Option<String>,
    to: &Option<String>,
    participant: &Option<String>,
    subject: &Option<String>,
    body: &Option<String>,
    since: &Option<String>,
    before: &Option<String>,
) -> DumpFilters {
    DumpFilters {
        for_identity: for_identity.clone(),
        from: from.clone(),
        to: to.clone(),
        participant: participant.clone(),
        subject: subject.clone(),
        body: body.clone(),
        since: since.clone(),
        before: before.clone(),
    }
}
