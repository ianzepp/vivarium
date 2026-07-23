use std::fs;

use super::*;

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
