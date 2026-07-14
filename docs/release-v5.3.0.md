# Vivarium 5.3.0

Vivarium 5.3.0 is a project-local board performance release. It makes
`vivi board` fast enough for every agent cycle on large mailspaces by batching
board reads and making display-handle work identity-scoped instead of global.

## Highlights

### Fast `vivi board`

`vivi board` now loads task, need, and want rows in a batch, batches event
loading, and partitions the result by identity in memory. On the reference
8.5K-message faberlang mailspace, warm subprocess runs dropped from roughly
11 seconds to roughly 30 milliseconds.

```sh
vivi board --project "$ROOT"
vivi board --project "$ROOT" --json
vivi board --project "$ROOT" --for codex
```

### Scoped handles for identity-bound work

Identity-bound work commands now resolve `--for <identity> <handle>` within the
identity's current name plus aliases. This preserves the documented CLI syntax
while avoiding global handle scans for task, need, want, and memo lists and
moves.

Display handles now use a minimum 8-character prefix and only lengthen when the
identity scope requires it.

### Faster work lists

`task list`, `need list`, `want list`, and memo listing paths use account+role
SQL and skip blob/event kind checks when the folder role already determines the
kind. Work-list event display now uses a batch event load.

## Compatibility

No breaking CLI changes are intended. Existing `--for <identity> <handle>`
commands remain valid. Board text and JSON shapes are unchanged.

`vivi-pty` is unchanged at version 1.0.0 and continues to ship alongside
`vivi` in release artifacts.

## Installation

```sh
# Homebrew
brew upgrade ianzepp/tap/vivarium

# curl installer
curl -fsSL https://raw.githubusercontent.com/ianzepp/vivarium/main/install.sh | bash

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
cargo clippy --all-targets -- -D warnings
cargo build --release
cargo build --release -p vivi-pty
target/release/vivi --version
target/release/vivi-pty --version
```
