# Local document rendering

Vivi renders local Markdown only. It never installs tools, fetches assets,
activates providers, executes shell/LaTeX commands, or sends mail as part of a
render.

## Diagnostics and rendering

```sh
vivi render --explain --format pdf
vivi render report.md --output report.pdf --format pdf
vivi render report.md --output report.html --format html
```

`--explain` reports the selected installed pipeline, alternatives, versions,
and missing prerequisites without secrets. The current safe PDF candidates are
`pandoc-tectonic`, `pandoc-typst`, and `pandoc-weasyprint`; HTML uses
`pandoc-html`. PDF Tectonic runs with `--only-cached` and `--untrusted`.
Auto-selection honors the configured deny list and fallback setting. A pinned
engine must be installed and is never silently replaced.

Configuration:

```toml
[defaults.render]
# engine = "pandoc-tectonic"
# deny_engines = ["pandoc-weasyprint"]
allow_fallback = true
```

Source, local image, and attachment limits are enforced. Raw HTML/TeX, remote
assets, file includes, absolute/parent-escaping paths, symlinks, oversized
inputs, and existing output paths fail closed. Output is installed atomically
from a private temporary directory. Receipts contain the selected pipeline,
tool versions, and source/output SHA-256 hashes; PDF engine metadata can still
vary, so reproducibility is semantic rather than byte-identical.

## Compose attachments

`vivi compose` keeps its existing behavior when no attachment option is used.
Use `--attach path` for a general local attachment, or
`--attach-document report.md` to include both the source Markdown and generated
PDF in the draft. Sending remains the existing explicit `vivi exec send`
operation under account policy; rendering has no SMTP authority.
