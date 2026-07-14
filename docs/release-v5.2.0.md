# Vivarium 5.2.0

Vivarium 5.2.0 improves project-local agent cycle operations. The release
focuses on making common Mind workflows typed and queryable so agents spend
less effort re-reading stale mail, hand-maintaining want/task lineage, or
piping JSON dumps into follow-on filters.

## Highlights

### Mail absorb

Advisory mail can now be marked as absorbed without moving it into the
task/need/want lifecycle:

```sh
vivi mail absorb --project "$ROOT" --for mind <handle> \
  --note "Converted to priority request"
```

Absorb is bookkeeping when a signal has been dispositioned. It is not an
integration acceptance gate and does not mean work was reviewed, accepted, or
merged. `mail list` and `mail dump` can filter absorbed and unabsorbed mail:

```sh
vivi mail list --project "$ROOT" --for mind --status unabsorbed --json
vivi mail dump --project "$ROOT" --for mind --status absorbed \
  --absorbed-by mind --json
```

### Task creation from source handles

`vivi task from <handle>` creates executable tasking from an existing source
handle while preserving lineage. The initial supported source kind is `want`.

```sh
vivi task from <want-handle> --project "$ROOT" \
  --for mind --to hand-2 \
  --subject "Fix prioritized issue" --body-file task.md
```

The source want records a `task from` lifecycle event, and created tasks store
source metadata so later tooling can answer what backlog item produced the
task.

### Structured want metadata

Wants can carry queryable priority and routing metadata:

```sh
vivi want set-priority <handle> --project "$ROOT" --for mind \
  --priority P1 --rank 20 --repo faber-runtime --lane correctness \
  --blocks-claim "tensor runtime does not panic" \
  --reason "Public Tensor API panic risk"
```

`want list` supports repo/lane filters and priority-aware sorting:

```sh
vivi want list --project "$ROOT" --for mind \
  --repo faber-runtime --lane correctness \
  --sort priority,rank,created --json
```

### Cycle intake

`vivi cycle intake` collects the core Mind cycle surface in one command:
unabsorbed mail, completed tasks since the cursor, open needs, and sorted open
wants.

```sh
vivi cycle intake --project "$ROOT" --for mind \
  --cursor-file .vivi/mind-cycle.cursor --write-cursor --json
```

### Mailspace import

Recovered project mailspaces can be imported into an active mailspace with a
dry-run report before writing:

```sh
vivi mailspace import --project /path/to/current/project \
  --from /path/to/recovered/project --dry-run --json
```

Imports preserve messages, blobs, events, and explicit thread links while
deduping repeated runs.

## Compatibility

No breaking CLI changes are intended. Existing mail, task, need, want, memo,
board, and watch commands remain available. The mailspace SQLite schema gains
an additive `mailspace_item_metadata` table and an additional explicit link
source value for source lineage.

`vivi-pty` is unchanged at version 1.0.0 and continues to ship alongside
`vivi` in release artifacts.

## Installation

```sh
# Homebrew
brew upgrade ianzepp/tap/vivarium

# From source
cargo install --path .
cargo install --path crates/vivi-pty
```

## Release Checks

Before publishing the tag, run:

```sh
cargo fmt --check
cargo test --test hygiene
cargo test
cargo build --release
target/release/vivi --version
target/release/vivi-pty --version
```
