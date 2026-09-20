use vivarium::VivariumError;
use vivarium::mailspace::Mailspace;

pub(crate) fn handle_step_command(
    apply: Option<&str>,
    project: Option<&std::path::Path>,
    json: bool,
) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let manifest = match apply {
        Some(handle) => mailspace.step_apply(handle)?,
        None => mailspace.step_shadow()?,
    };
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&manifest)
                .map_err(|e| VivariumError::Other(format!("failed to encode JSON: {e}")))?
        );
        return Ok(());
    }
    print_manifest(&manifest);
    Ok(())
}

fn print_manifest(manifest: &vivarium::mailspace::StepManifest) {
    println!("graph {}", manifest.graph);
    println!();
    if manifest.dispatches.is_empty() {
        println!("  no dispatches");
    } else {
        println!("  node  item  kind  clauses  scope  subject");
        for dispatch in &manifest.dispatches {
            println!(
                "  {}  {}  {}  {}  {}  {}",
                dispatch.node,
                dispatch.item,
                dispatch.kind,
                dispatch.done_when_clauses,
                if dispatch.write_scope { "yes" } else { "no" },
                dispatch.subject
            );
        }
    }
    println!();
    if manifest.exceptions.is_empty() {
        println!("  no exceptions");
    } else {
        println!("  node  item  kind  reason  detail");
        for exception in &manifest.exceptions {
            println!(
                "  {}  {}  {}  {}  {}",
                exception.node, exception.item, exception.kind, exception.reason, exception.detail
            );
        }
    }
    println!();
    if manifest.decisions.is_empty() {
        println!("  no decisions");
    } else {
        println!("  decisions");
        for decision in &manifest.decisions {
            println!("  {decision}");
        }
    }
}
