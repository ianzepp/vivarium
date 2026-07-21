use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use crate::cli::RenderFormat;
use crate::config::{Config, RenderDefaults};
use crate::error::VivariumError;
use crate::message::FileAttachment;

const MAX_SOURCE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_RESOURCE_BYTES: u64 = 20 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct RenderReceipt {
    pub format: String,
    pub pipeline: String,
    pub tool_versions: Vec<String>,
    pub source_sha256: String,
    pub output_sha256: String,
    pub output_filename: String,
    pub reproducibility: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderExplanation {
    pub format: String,
    pub selected_pipeline: Option<String>,
    pub fallback_enabled: bool,
    pub pinned_engine: Option<String>,
    pub denied_engines: Vec<String>,
    pub candidates: Vec<PipelineExplanation>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PipelineExplanation {
    pub name: String,
    pub available: bool,
    pub prerequisites: Vec<Prerequisite>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Prerequisite {
    pub executable: String,
    pub available: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
struct Pipeline {
    name: &'static str,
    tools: &'static [&'static str],
    pdf_engine: Option<&'static str>,
    pdf_options: &'static [&'static str],
}

const PANDOC_HTML: Pipeline = Pipeline {
    name: "pandoc-html",
    tools: &["pandoc"],
    pdf_engine: None,
    pdf_options: &[],
};
const PANDOC_TECTONIC: Pipeline = Pipeline {
    name: "pandoc-tectonic",
    tools: &["pandoc", "tectonic"],
    pdf_engine: Some("tectonic"),
    pdf_options: &["--only-cached", "--untrusted"],
};
const PANDOC_WEASYPRINT: Pipeline = Pipeline {
    name: "pandoc-weasyprint",
    tools: &["pandoc", "weasyprint"],
    pdf_engine: Some("weasyprint"),
    pdf_options: &[],
};
const PANDOC_TYPST: Pipeline = Pipeline {
    name: "pandoc-typst",
    tools: &["pandoc", "typst"],
    pdf_engine: Some("typst"),
    pdf_options: &["--ignore-system-fonts"],
};

/// Produce a JSON explanation of available render pipelines.
///
/// # Errors
/// Returns an error if serializing the explanation to JSON fails.
pub fn explain(
    config: &Config,
    format: RenderFormat,
    requested_engine: Option<String>,
) -> Result<String, VivariumError> {
    let policy = &config.defaults.render;
    let requested = requested_engine.or_else(|| policy.engine.clone());
    let candidates = candidate_pipelines(format);
    let selected = select_pipeline(&candidates, requested.as_deref(), policy)
        .ok()
        .map(|pipeline| pipeline.name.to_string());
    let explanation = RenderExplanation {
        format: format_name(format).into(),
        selected_pipeline: selected,
        fallback_enabled: policy.allow_fallback,
        pinned_engine: requested,
        denied_engines: policy.deny_engines.clone(),
        candidates: candidates.iter().copied().map(explain_pipeline).collect(),
    };
    serde_json::to_string_pretty(&explanation).map_err(|error| {
        VivariumError::Other(format!("failed to encode render explanation: {error}"))
    })
}

/// Render a markdown document to the specified format using a selectable
/// pipeline.
///
/// The output path must not already exist (atomic install).
///
/// # Errors
/// Returns an error if the source is invalid, the pipeline selection fails,
/// or the render command fails.
pub fn render_document(
    input: &Path,
    output: &Path,
    format: RenderFormat,
    config: &Config,
    requested_engine: Option<String>,
) -> Result<RenderReceipt, VivariumError> {
    let source = validate_source(input)?;
    let source_data = fs::read(&source)?;
    validate_markdown(&source_data, source.parent().unwrap_or(Path::new(".")))?;
    let pipeline = select_pipeline(
        &candidate_pipelines(format),
        requested_engine
            .or_else(|| config.defaults.render.engine.clone())
            .as_deref(),
        &config.defaults.render,
    )?;
    let (output, temp) = prepare_output(output)?;
    let temp_output = temp.path().join(output.file_name().unwrap_or_default());
    run_pipeline(
        pipeline,
        &source,
        source.parent().unwrap_or(Path::new(".")),
        &temp_output,
        format,
    )?;
    let output_data = install_output(&temp_output, &output)?;
    Ok(receipt(
        format,
        pipeline,
        &source_data,
        &output_data,
        &output,
    ))
}

fn prepare_output(output: &Path) -> Result<(PathBuf, TempDir), VivariumError> {
    let output_parent = output.parent().ok_or_else(|| {
        VivariumError::Config("render output must have a parent directory".into())
    })?;
    let output_parent = output_parent.canonicalize().map_err(|error| {
        VivariumError::Config(format!("render output parent is unavailable: {error}"))
    })?;
    let output = output_parent.join(
        output
            .file_name()
            .ok_or_else(|| VivariumError::Config("render output needs a filename".into()))?,
    );
    reject_existing_output(&output)?;
    let temp = tempfile::Builder::new()
        .prefix("vivi-render-")
        .tempdir_in(&output_parent)
        .map_err(|error| {
            VivariumError::Other(format!("failed to create private render temp: {error}"))
        })?;
    Ok((output, temp))
}

fn install_output(temp_output: &Path, output: &Path) -> Result<Vec<u8>, VivariumError> {
    let output_data = fs::read(temp_output)?;
    if output_data.is_empty() {
        return Err(VivariumError::Other(
            "render engine produced an empty output".into(),
        ));
    }
    fs::rename(temp_output, output).map_err(|error| {
        VivariumError::Other(format!(
            "failed to atomically install render output: {error}"
        ))
    })?;
    Ok(output_data)
}

fn receipt(
    format: RenderFormat,
    pipeline: &Pipeline,
    source_data: &[u8],
    output_data: &[u8],
    output: &Path,
) -> RenderReceipt {
    RenderReceipt {
        format: format_name(format).into(),
        pipeline: pipeline.name.into(),
        tool_versions: pipeline
            .tools
            .iter()
            .filter_map(|tool| executable_version(tool))
            .collect(),
        source_sha256: sha256(source_data),
        output_sha256: sha256(output_data),
        output_filename: output
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("output")
            .into(),
        reproducibility: "semantic output is deterministic; engine PDF metadata may vary",
    }
}

/// Compose file attachments with an optional rendered PDF document.
///
/// # Errors
/// Returns an error if reading attachments, validating the document source,
/// or rendering the PDF fails.
pub fn compose_attachments(
    files: &[PathBuf],
    document: Option<&Path>,
    config: &Config,
    requested_engine: Option<String>,
) -> Result<Vec<FileAttachment>, VivariumError> {
    let mut attachments = files
        .iter()
        .map(|path| read_attachment(path))
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(document) = document {
        let source = validate_source(document)?;
        let source_data = fs::read(&source)?;
        validate_markdown(&source_data, source.parent().unwrap_or(Path::new(".")))?;
        let source_name = safe_filename(&source)?;
        attachments.push(FileAttachment {
            filename: source_name,
            content_type: "text/markdown".into(),
            data: source_data,
        });
        let temp = tempfile::Builder::new()
            .prefix("vivi-attachment-")
            .tempdir()
            .map_err(|error| {
                VivariumError::Other(format!("failed to create private attachment temp: {error}"))
            })?;
        let pdf_name = format!(
            "{}.pdf",
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("document")
        );
        let pdf_path = temp.path().join(&pdf_name);
        render_document(
            document,
            &pdf_path,
            RenderFormat::Pdf,
            config,
            requested_engine,
        )?;
        attachments.push(FileAttachment {
            filename: pdf_name,
            content_type: "application/pdf".into(),
            data: fs::read(pdf_path)?,
        });
    }
    Ok(attachments)
}

fn candidate_pipelines(format: RenderFormat) -> Vec<&'static Pipeline> {
    match format {
        RenderFormat::Html => vec![&PANDOC_HTML],
        RenderFormat::Pdf => vec![&PANDOC_TECTONIC, &PANDOC_TYPST, &PANDOC_WEASYPRINT],
    }
}

fn select_pipeline<'a>(
    candidates: &[&'a Pipeline],
    requested: Option<&str>,
    policy: &RenderDefaults,
) -> Result<&'a Pipeline, VivariumError> {
    if let Some(name) = requested.filter(|name| *name != "auto") {
        let pipeline = candidates
            .iter()
            .copied()
            .find(|pipeline| pipeline.name == name)
            .ok_or_else(|| VivariumError::Config(format!("unknown render engine '{name}'")))?;
        if policy
            .deny_engines
            .iter()
            .any(|denied| denied == pipeline.name)
        {
            return Err(VivariumError::Config(format!(
                "render engine '{name}' is denied by config"
            )));
        }
        return require_available(pipeline);
    }
    let mut available = candidates.iter().copied().filter(|pipeline| {
        !policy
            .deny_engines
            .iter()
            .any(|denied| denied == pipeline.name)
    });
    let Some(first) = available.next() else {
        return Err(VivariumError::Config(
            "no render pipeline is configured".into(),
        ));
    };
    if pipeline_available(first) {
        return Ok(first);
    }
    if !policy.allow_fallback {
        return Err(missing_prerequisites(first));
    }
    available
        .find(|pipeline| pipeline_available(pipeline))
        .ok_or_else(|| {
            VivariumError::Config("no installed safe render pipeline is available".into())
        })
}

fn require_available(pipeline: &Pipeline) -> Result<&Pipeline, VivariumError> {
    if pipeline_available(pipeline) {
        Ok(pipeline)
    } else {
        Err(missing_prerequisites(pipeline))
    }
}

fn pipeline_available(pipeline: &Pipeline) -> bool {
    pipeline
        .tools
        .iter()
        .all(|tool| find_executable(tool).is_some())
}

fn missing_prerequisites(pipeline: &Pipeline) -> VivariumError {
    let missing = pipeline
        .tools
        .iter()
        .filter(|tool| find_executable(tool).is_none())
        .copied()
        .collect::<Vec<_>>();
    VivariumError::Config(format!(
        "render pipeline '{}' is unavailable; missing prerequisites: {}",
        pipeline.name,
        missing.join(", ")
    ))
}

fn explain_pipeline(pipeline: &'static Pipeline) -> PipelineExplanation {
    PipelineExplanation {
        name: pipeline.name.into(),
        available: pipeline_available(pipeline),
        prerequisites: pipeline
            .tools
            .iter()
            .map(|tool| Prerequisite {
                executable: (*tool).into(),
                available: find_executable(tool).is_some(),
                version: executable_version(tool),
            })
            .collect(),
    }
}

fn run_pipeline(
    pipeline: &Pipeline,
    source: &Path,
    resource_root: &Path,
    output: &Path,
    format: RenderFormat,
) -> Result<(), VivariumError> {
    let pandoc = find_executable("pandoc")
        .ok_or_else(|| VivariumError::Config("pandoc is not installed".into()))?;
    let title = source
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("document")
        .replace([':', '\n', '\r'], "-");
    let mut args = vec![
        "--from=markdown-raw_html-raw_tex".into(),
        "--standalone".into(),
        "--embed-resources".into(),
        format!("--resource-path={}", resource_root.display()),
        format!("--metadata=title:{title}"),
        "--metadata=author:".into(),
        "--metadata=date:".into(),
        format!("--output={}", output.display()),
    ];
    if format == RenderFormat::Pdf {
        args.push(format!(
            "--pdf-engine={}",
            pipeline.pdf_engine.unwrap_or("")
        ));
        for option in pipeline.pdf_options {
            args.push(format!("--pdf-engine-opt={option}"));
        }
    }
    args.push(source.display().to_string());
    let result = Command::new(pandoc).args(args).output().map_err(|error| {
        VivariumError::Other(format!(
            "failed to start render pipeline '{}': {error}",
            pipeline.name
        ))
    })?;
    if !result.status.success() {
        return Err(VivariumError::Other(format!(
            "render pipeline '{}' failed: {}",
            pipeline.name,
            scrub_tool_output(&String::from_utf8_lossy(&result.stderr))
        )));
    }
    Ok(())
}

fn validate_source(path: &Path) -> Result<PathBuf, VivariumError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(VivariumError::Config(
            "render source must be a regular non-symlink file".into(),
        ));
    }
    if metadata.len() > MAX_SOURCE_BYTES {
        return Err(VivariumError::Config(
            "render source exceeds the 5 MiB limit".into(),
        ));
    }
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default();
    if !matches!(extension.to_ascii_lowercase().as_str(), "md" | "markdown") {
        return Err(VivariumError::Config(
            "render source must be Markdown (.md or .markdown)".into(),
        ));
    }
    path.canonicalize()
        .map_err(|error| VivariumError::Config(format!("cannot resolve render source: {error}")))
}

fn validate_markdown(data: &[u8], resource_root: &Path) -> Result<(), VivariumError> {
    let text = std::str::from_utf8(data)
        .map_err(|error| VivariumError::Message(format!("Markdown is not UTF-8: {error}")))?;
    if text.starts_with("---\n") || text.starts_with("---\r\n") {
        return Err(VivariumError::Message(
            "YAML front matter is rejected for safe rendering".into(),
        ));
    }
    if text.contains("!include") || text.contains("@import") || text.contains("file:") {
        return Err(VivariumError::Message(
            "file includes are rejected for safe rendering".into(),
        ));
    }
    if text.char_indices().any(|(index, character)| {
        character == '<'
            && text[index + character.len_utf8()..]
                .chars()
                .next()
                .is_some_and(|next| next.is_ascii_alphabetic() || matches!(next, '/' | '!' | '?'))
    }) {
        return Err(VivariumError::Message(
            "raw HTML is rejected for safe rendering".into(),
        ));
    }
    if text.contains("$$") || text.contains('\\') {
        for (index, _) in text.match_indices('\\') {
            if text[index + 1..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_alphabetic())
            {
                return Err(VivariumError::Message(
                    "raw TeX is rejected for safe rendering".into(),
                ));
            }
        }
    }
    validate_images(text, resource_root)
}

fn validate_images(text: &str, resource_root: &Path) -> Result<(), VivariumError> {
    let resource_root = resource_root.canonicalize()?;
    let mut rest = text;
    let mut total = 0u64;
    while let Some(start) = rest.find("![") {
        rest = &rest[start + 2..];
        let Some(open) = rest.find("](") else {
            continue;
        };
        let target = rest[open + 2..]
            .split(')')
            .next()
            .unwrap_or_default()
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .trim_matches('<');
        total = total.saturating_add(validate_image_target(target, &resource_root)?);
        if total > MAX_RESOURCE_BYTES {
            return Err(VivariumError::Config(
                "local render resources exceed the 20 MiB limit".into(),
            ));
        }
    }
    Ok(())
}

fn validate_image_target(target: &str, resource_root: &Path) -> Result<u64, VivariumError> {
    if target.is_empty()
        || target.starts_with("http:")
        || target.starts_with("https:")
        || target.starts_with("data:")
        || target.starts_with("file:")
    {
        return Err(VivariumError::Message(
            "remote or empty image targets are rejected".into(),
        ));
    }
    let path = Path::new(target);
    if path.is_absolute()
        || path
            .components()
            .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(VivariumError::Message(
            "image path escapes the Markdown resource root".into(),
        ));
    }
    let full = resource_root.join(path);
    let metadata = fs::symlink_metadata(&full)
        .map_err(|_| VivariumError::Message(format!("local image is missing: {target}")))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(VivariumError::Message(format!(
            "local image is not a regular file: {target}"
        )));
    }
    if !full.canonicalize()?.starts_with(resource_root) {
        return Err(VivariumError::Message(
            "image path escapes the Markdown resource root".into(),
        ));
    }
    Ok(metadata.len())
}

fn reject_existing_output(output: &Path) -> Result<(), VivariumError> {
    if output.exists() || fs::symlink_metadata(output).is_ok() {
        return Err(VivariumError::Config(format!(
            "render output already exists: {}",
            output.display()
        )));
    }
    Ok(())
}

fn read_attachment(path: &Path) -> Result<FileAttachment, VivariumError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(VivariumError::Config(
            "attachment must be a regular non-symlink file".into(),
        ));
    }
    if metadata.len() > MAX_RESOURCE_BYTES {
        return Err(VivariumError::Config(
            "attachment exceeds the 20 MiB limit".into(),
        ));
    }
    let filename = safe_filename(path)?;
    let content_type = mime_for_filename(&filename);
    Ok(FileAttachment {
        filename,
        content_type: content_type.into(),
        data: fs::read(path)?,
    })
}

fn safe_filename(path: &Path) -> Result<String, VivariumError> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| VivariumError::Config("attachment needs a UTF-8 filename".into()))?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', '\0', '\n', '\r'])
    {
        return Err(VivariumError::Config("unsafe attachment filename".into()));
    }
    Ok(name.into())
}

fn mime_for_filename(filename: &str) -> &'static str {
    match Path::new(filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "md" | "markdown" => "text/markdown",
        "pdf" => "application/pdf",
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => "application/octet-stream",
    }
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|path| path.join(name))
        .find(|path| path.is_file())
}

fn executable_version(name: &str) -> Option<String> {
    let executable = find_executable(name)?;
    let output = Command::new(executable).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(scrub_tool_output)
}

fn scrub_tool_output(value: &str) -> String {
    let home = dirs::home_dir().map(|path| path.to_string_lossy().into_owned());
    let mut value = value.replace(['\n', '\r'], " ");
    if let Some(home) = home {
        value = value.replace(&home, "~");
    }
    value
}

fn sha256(data: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(data);
    hex::encode(digest.finalize())
}

fn format_name(format: RenderFormat) -> &'static str {
    match format {
        RenderFormat::Html => "html",
        RenderFormat::Pdf => "pdf",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn explain_reports_installed_pipeline_without_secrets() {
        let explanation = explain(&Config::default(), RenderFormat::Pdf, None).unwrap();
        assert!(explanation.contains("selected_pipeline"));
        assert!(!explanation.contains("PASSWORD"));
    }

    #[test]
    fn hostile_markdown_is_rejected() {
        let root = tempfile::tempdir().unwrap();
        for source in [
            "<script>x</script>",
            "\\input{secret}",
            "![x](https://example/x.png)",
        ] {
            let error = validate_markdown(source.as_bytes(), root.path()).unwrap_err();
            assert!(error.to_string().contains("rejected"));
        }
    }

    #[test]
    fn image_paths_are_bounded_and_symlinks_rejected() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("ok.png"), b"png").unwrap();
        validate_markdown(b"![x](ok.png)", root.path()).unwrap();
        let error = validate_markdown(b"![x](../outside.png)", root.path()).unwrap_err();
        assert!(error.to_string().contains("escapes"));
    }

    #[test]
    fn operational_markdown_features_are_accepted() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("diagram.png"), b"png").unwrap();
        let source = "# Unicode — report\n\n| A | B |\n|---|---|\n| 1 | 2 |\n\n```text\nlong code block\n```\n\n[reference](https://example.invalid)\n\n![diagram](diagram.png)";
        validate_markdown(source.as_bytes(), root.path()).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn image_symlinks_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("secret.png"), b"secret").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.png"),
            root.path().join("link.png"),
        )
        .unwrap();
        let error = validate_markdown(b"![x](link.png)", root.path()).unwrap_err();
        assert!(error.to_string().contains("regular file"));
    }

    #[test]
    fn missing_engine_fails_visible_and_pinned_engine_never_falls_back() {
        let candidates = [&PANDOC_TECTONIC];
        let policy = RenderDefaults {
            engine: Some("pandoc-tectonic".into()),
            deny_engines: Vec::new(),
            allow_fallback: true,
        };
        let result = select_pipeline(&candidates, Some("missing"), &policy);
        assert!(result.is_err());
    }

    #[test]
    fn auto_fallback_and_pinned_engine_are_deterministic() {
        const MISSING: Pipeline = Pipeline {
            name: "missing",
            tools: &["__vivi_missing_engine__"],
            pdf_engine: None,
            pdf_options: &[],
        };
        const AVAILABLE: Pipeline = Pipeline {
            name: "available",
            tools: &[],
            pdf_engine: None,
            pdf_options: &[],
        };
        let candidates = [&MISSING, &AVAILABLE];
        let fallback = RenderDefaults {
            engine: None,
            deny_engines: Vec::new(),
            allow_fallback: true,
        };
        assert_eq!(
            select_pipeline(&candidates, None, &fallback).unwrap().name,
            "available"
        );
        let pinned = RenderDefaults {
            engine: Some("missing".into()),
            deny_engines: Vec::new(),
            allow_fallback: true,
        };
        assert!(select_pipeline(&candidates, Some("missing"), &pinned).is_err());
    }

    #[test]
    fn attachment_mime_defaults_are_safe_and_filenames_are_preserved() {
        assert_eq!(mime_for_filename("report.pdf"), "application/pdf");
        assert_eq!(mime_for_filename("data.bin"), "application/octet-stream");
        assert_eq!(safe_filename(Path::new("report.md")).unwrap(), "report.md");
        assert!(safe_filename(Path::new("../report.md")).is_ok());
    }

    #[test]
    fn atomic_output_rejects_existing_paths() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("output.pdf");
        fs::write(&path, b"existing").unwrap();
        assert!(reject_existing_output(&path).is_err());
    }
}
