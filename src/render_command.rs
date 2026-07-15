use vivarium::VivariumError;
use vivarium::cli::{RenderCommand, RenderFormat};

use super::Runtime;

impl Runtime {
    pub(crate) fn render(&self, command: RenderCommand) -> Result<(), VivariumError> {
        let format = command
            .format
            .or_else(|| command.output.as_deref().and_then(RenderFormat::from_path))
            .unwrap_or(RenderFormat::Pdf);
        if command.explain {
            println!(
                "{}",
                vivarium::render::explain(&self.config, format, command.engine)?
            );
            return Ok(());
        }
        let input = command
            .input
            .ok_or_else(|| VivariumError::Config("render requires a Markdown input path".into()))?;
        let output = command.output.ok_or_else(|| {
            VivariumError::Config("render requires --output unless --explain is used".into())
        })?;
        let receipt = vivarium::render::render_document(
            &input,
            &output,
            format,
            &self.config,
            command.engine,
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&receipt).map_err(|error| {
                VivariumError::Other(format!("failed to encode render receipt: {error}"))
            })?
        );
        Ok(())
    }
}

trait RenderFormatPath {
    fn from_path(path: &std::path::Path) -> Option<RenderFormat>;
}

impl RenderFormatPath for RenderFormat {
    fn from_path(path: &std::path::Path) -> Option<RenderFormat> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "html" | "htm" => Some(RenderFormat::Html),
            "pdf" => Some(RenderFormat::Pdf),
            _ => None,
        }
    }
}
