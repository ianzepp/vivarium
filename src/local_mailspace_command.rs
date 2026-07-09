use vivarium::VivariumError;
use vivarium::cli::{
    Command, LocalSendCommand, MailCommand, MailDumpCommand, MailspaceCommand,
    MailspaceIdentityCommand, TaskCommand,
};
use vivarium::mailspace::{DumpFilters, MailDumpRequest, Mailspace, SendRequest};
use vivarium::message;

pub(crate) fn run_mailspace_command(command: &Command) -> Result<bool, VivariumError> {
    match command {
        Command::Mailspace { command } => {
            handle_mailspace_command(command)?;
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
        _ => Ok(false),
    }
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
        MailspaceCommand::Identity { command } => match command {
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
                }
            }
        },
    }
    Ok(())
}

fn handle_mail_command(command: &MailCommand) -> Result<(), VivariumError> {
    match command {
        MailCommand::Send(command) => {
            let mailspace = Mailspace::discover(command.project.as_deref())?;
            let result = mailspace.send(SendRequest {
                from: command.from.clone(),
                to: command.to.clone(),
                cc: command.cc.clone(),
                bcc: command.bcc.clone(),
                subject: command.subject.clone(),
                body: vivarium::mailspace::read_body_arg(&command.body)?,
                role: "inbox".into(),
                kind: Some("mail".into()),
            })?;
            for delivered in result.delivered {
                println!("delivered {} {}", delivered.identity, delivered.handle);
            }
            println!("sent {}", result.sent);
        }
        MailCommand::Deliver {
            path,
            folder,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            let data = std::fs::read(path)?;
            for delivered in mailspace.deliver_raw(&data, folder)? {
                println!("delivered {} {}", delivered.identity, delivered.handle);
            }
        }
        MailCommand::List {
            for_identity,
            folder,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            print_local_list(&mailspace, for_identity, folder)?;
        }
        MailCommand::Show {
            handles,
            json,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            print_local_messages(&mailspace, handles, *json)?;
        }
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

fn handle_task_command(command: &TaskCommand) -> Result<(), VivariumError> {
    match command {
        TaskCommand::Send(command) => {
            send_task(command)?;
        }
        TaskCommand::List {
            for_identity,
            status,
            json,
            project,
        } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            crate::local_work_list::print_work_list(
                &mailspace,
                for_identity,
                match status {
                    vivarium::cli::TaskStatus::Open => "tasks",
                    vivarium::cli::TaskStatus::Done => "done",
                },
                "task",
                *json,
            )?;
        }
        TaskCommand::Show { handle, project } => {
            let mailspace = Mailspace::discover(project.as_deref())?;
            let storage = mailspace.storage()?;
            let data = storage.read_message(handle)?;
            println!("{}", message::render_message(&data)?);
        }
        TaskCommand::Dump(command) => {
            crate::local_work_command::dump_tasks(command)?;
        }
        TaskCommand::Done {
            handle,
            for_identity,
            note,
            project,
        } => {
            move_task(handle, for_identity, note, project.as_deref(), "done")?;
        }
        TaskCommand::Reopen {
            handle,
            for_identity,
            note,
            project,
        } => {
            move_task(handle, for_identity, note, project.as_deref(), "tasks")?;
        }
    }
    Ok(())
}

fn send_task(command: &LocalSendCommand) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(command.project.as_deref())?;
    let result = mailspace.send(SendRequest {
        from: command.from.clone(),
        to: command.to.clone(),
        cc: command.cc.clone(),
        bcc: command.bcc.clone(),
        subject: command.subject.clone(),
        body: vivarium::mailspace::read_body_arg(&command.body)?,
        role: "tasks".into(),
        kind: Some("task".into()),
    })?;
    for delivered in result.delivered {
        println!("created {} {}", delivered.identity, delivered.handle);
    }
    println!("sent {}", result.sent);
    Ok(())
}

fn move_task(
    handle: &str,
    for_identity: &str,
    note: &Option<String>,
    project: Option<&std::path::Path>,
    role: &str,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let handle = mailspace.move_task(for_identity, handle, role, note.as_deref())?;
    let verb = if role == "done" { "done" } else { "reopened" };
    println!("{verb} {handle}");
    Ok(())
}

fn print_local_list(
    mailspace: &Mailspace,
    identity: &str,
    role: &str,
) -> Result<(), VivariumError> {
    let messages = mailspace.list(identity, role)?;
    if messages.is_empty() {
        println!("  no messages in {role}");
    } else {
        for message in messages {
            println!(
                "  {}  {}  {}",
                message.handle, message.from_addr, message.subject
            );
        }
    }
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
