use std::fmt::Write;
use std::path::Path;

use vivarium::VivariumError;
use vivarium::mailspace::DumpRecord;

const MAX_STDOUT_DUMP_RECORDS: usize = 25;
const MAX_STDOUT_DUMP_BYTES: usize = 64 * 1024;

pub(crate) fn write_dump(
    title: &str,
    records: &[DumpRecord],
    json: bool,
    output: Option<&Path>,
) -> Result<(), VivariumError> {
    let rendered = if json {
        serde_json::to_string_pretty(records)
            .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
    } else {
        render_markdown(title, records)
    };
    if !json && output.is_none() {
        enforce_stdout_limit(title, records, rendered.len())?;
    }
    if let Some(path) = output {
        std::fs::write(path, rendered)?;
    } else {
        println!("{rendered}");
    }
    Ok(())
}

fn enforce_stdout_limit(
    title: &str,
    records: &[DumpRecord],
    rendered_len: usize,
) -> Result<(), VivariumError> {
    if records.len() <= MAX_STDOUT_DUMP_RECORDS && rendered_len <= MAX_STDOUT_DUMP_BYTES {
        return Ok(());
    }
    Err(VivariumError::Message(format!(
        "{title} matched {} records and {} bytes; refusing large human stdout dump. \
         Narrow it with --status open, --since, --before, --subject, or --body, \
         or export the full result with --json or --output <path>.",
        records.len(),
        rendered_len
    )))
}

fn render_markdown(title: &str, records: &[DumpRecord]) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(title);
    out.push_str("\n\n");
    let _ = write!(out, "count: {}\n\n", records.len());
    if records.is_empty() {
        out.push_str("No matching messages.\n");
        return out;
    }
    for record in records {
        push_record(&mut out, record);
    }
    out
}

fn push_record(out: &mut String, record: &DumpRecord) {
    let status = record
        .status
        .as_deref()
        .map(|status| format!(" - {status}"))
        .unwrap_or_default();
    let _ = write!(out, "## {} - {}{}\n\n", record.date, record.handle, status);
    let _ = writeln!(out, "Role: {}", record.role);
    if let Some(kind) = &record.kind {
        let _ = writeln!(out, "Kind: {kind}");
    }
    let _ = writeln!(out, "Account: {}", record.account);
    let _ = writeln!(out, "From: {}", empty_marker(&record.from));
    let _ = writeln!(out, "To: {}", empty_marker(&record.to));
    if !record.cc.is_empty() {
        let _ = writeln!(out, "Cc: {}", record.cc);
    }
    let _ = write!(out, "Subject: {}\n\n", empty_marker(&record.subject));
    if let Some(parent) = &record.parent_content_id {
        let _ = writeln!(out, "Parent content: {parent}");
        if let Some(source) = &record.link_source {
            let _ = writeln!(out, "Link source: {source}");
        }
        out.push('\n');
    }
    push_events(out, record);
    out.push_str(record.body.trim());
    out.push_str("\n\n---\n\n");
}

fn push_events(out: &mut String, record: &DumpRecord) {
    if record.events.is_empty() {
        return;
    }
    out.push_str("Events:\n");
    for event in &record.events {
        let _ = write!(out, "- {} {} {}", event.occurred_at, event.command, event.event_type);
        if let Some(actor) = &event.actor_identity {
            let _ = write!(out, " by {actor}");
        }
        if event.from_role.is_some() || event.to_role.is_some() {
            let _ = write!(
                out,
                " ({} -> {})",
                event.from_role.as_deref().unwrap_or("(none)"),
                event.to_role.as_deref().unwrap_or("(none)")
            );
        }
        out.push('\n');
    }
    out.push('\n');
}

fn empty_marker(value: &str) -> &str {
    if value.is_empty() { "(none)" } else { value }
}
