//! `vivi boot` dispatch: collect the frame and print it.

use std::path::Path;

use vivi::VivariumError;
use vivi::boot::BootReport;
use vivi::mailspace::Mailspace;

pub(crate) fn handle_boot_command(project: Option<&Path>) -> Result<(), VivariumError> {
    let mailspace = Mailspace::discover(project)?;
    let report = BootReport::collect(&mailspace)?;
    print!("{}", report.render());
    Ok(())
}
