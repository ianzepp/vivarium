use std::path::Path;

use vivarium::VivariumError;
use vivarium::mailspace::DumpRecord;

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
    if let Some(path) = output {
        std::fs::write(path, rendered)?;
    } else {
        println!("{rendered}");
    }
    Ok(())
}

fn render_markdown(title: &str, records: &[DumpRecord]) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(title);
    out.push_str("\n\n");
    out.push_str(&format!("count: {}\n\n", records.len()));
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
    out.push_str(&format!(
        "## {} - {}{}\n\n",
        record.date, record.handle, status
    ));
    out.push_str(&format!("Role: {}\n", record.role));
    out.push_str(&format!("Account: {}\n", record.account));
    out.push_str(&format!("From: {}\n", empty_marker(&record.from)));
    out.push_str(&format!("To: {}\n", empty_marker(&record.to)));
    if !record.cc.is_empty() {
        out.push_str(&format!("Cc: {}\n", record.cc));
    }
    out.push_str(&format!("Subject: {}\n\n", empty_marker(&record.subject)));
    out.push_str(record.body.trim());
    out.push_str("\n\n---\n\n");
}

fn empty_marker(value: &str) -> &str {
    if value.is_empty() { "(none)" } else { value }
}
