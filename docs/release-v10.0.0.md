# Vivi 10.0.0

Vivi 10.0.0 is the release in which this project becomes three repositories.
This one keeps the project mailspace. The email half moves to
[`vivi-mail`](https://github.com/ianzepp/vivi-mail) and the terminal runtime to
[`vivi-pty`](https://github.com/ianzepp/vivi-pty), each with its own binary and
its own release.

**This is a breaking release.** The email commands that used to live in the
`vivi` binary are now the `vivi-mail` binary. `vivi` is the project mailspace
and nothing else. Every mailspace command behaves as it did in 9.4.0, and the
on-disk mailspace format is unchanged.

## Why

Vivi was one binary holding two halves that shared no data and no runtime
state. The mailspace — tasks, needs, wants, memos, roles, goals, and work
graphs — is the surface used day to day. The email stack is built for isolated
agent containers and archive work, and it brought IMAP, SMTP, Proton,
embeddings, and a queue with it.

Splitting the repositories lets each half move at its own pace, keeps the
mailspace free of transport dependencies, and means an installer for one does
not carry the other. The split happened in two steps: 9.4.0 separated the
crates inside one tree, and 10.0.0 separates the repositories.

## What changed

### The package is `vivi`

The Cargo package was `vivarium`; it is now `vivi`. The binary name is
unchanged, and so is the CLI's own name. The repository was renamed from
`ianzepp/vivarium` to [`ianzepp/vivi`](https://github.com/ianzepp/vivi); GitHub
redirects the old URLs.

### The email commands moved out

These left the `vivi` binary for `vivi-mail`:

`init`, `auth`, `token`, `sync`, `sync-events`, `folders`, `doctor`, `proton`,
`render`, `watch-inbox`, `list`, `show`, `thread`, `reply`, `compose`, `export`,
`search`, `index`, `agent`, `exec`, `enqueue`, `queue`, `labels`, `label`.

`vivi` keeps `board`, `boot`, `mailspace`, `mail`, `task`, `need`, `want`,
`memo`, `goal`, `role`, `cycle`, `trace`, `graph`, and `step`.

### Some global flags are gone

`--config`, `--account`, `--insecure`, and `--ignore-permissions` existed for
the mail runtime, and no mailspace command read them. They are removed rather
than left accepted-and-ignored. `--verbose` and `--project` remain, and
`--project` still works before or after the subcommand.

### Judgment config moved into the mailspace

`[judgment]` used to sit in the user-level `~/.vivarium/config.toml`. It now
belongs to the project, in `.vivi/mailspace.toml`:

```toml
[judgment]
provider = "typesafe"
key_cmd  = "cat ~/.config/secrets/typesafe-ai.key"
```

The provider's answers were always written to the project
(`.vivi/judgment-corpus.jsonl`), so the setting now lives where its effects do.
The feature was new and unadopted, so there is no migration path and no
dual-read fallback: if you had configured it, move the table by hand.

`vivi step --apply` reads the provider from the mailspace it was invoked on.

### This binary no longer touches a home directory

With the mail commands gone and `[judgment]` moved, `vivi` resolves no home
directory, reads no `~/.vivarium`, honours no `VIVI_HOME`, and holds no
credentials. Account configuration and the email archive are `vivi-mail`'s.

The last trace of the old arrangement is gone from storage as well: the
`Storage::open` opener for `<mail_root>/.vivarium/storage.sqlite` had no
callers left once the archive import path went, so it and the `.vivarium` name
are removed.

### Release artifacts contain one binary

Archives from this repository now contain `vivi` alone. Through 9.4.0 they also
carried `vivi-pty`, which now ships from its own repository. Install `vivi-pty`
from [`ianzepp/vivi-pty`](https://github.com/ianzepp/vivi-pty) if you use it.

## Upgrading

1. Install `vivi-mail` from
   [`ianzepp/vivi-mail`](https://github.com/ianzepp/vivi-mail) for anything that
   used `vivi sync`, `vivi list`, `vivi search`, `vivi init`, or the other mail
   commands. Its released binary does not include the `outbox` feature, so
   `auth` and `token` need a source build: `cargo install --path . --features
   outbox`.
2. If any project had `[judgment]` configured, move that table from
   `~/.vivarium/config.toml` into the project's `.vivi/mailspace.toml`.
3. Nothing else needs to change. Mailspaces, roles, goals, work graphs, and the
   `mail.sqlite` format are untouched.

## Documentation

`README.md` and `AGENTS.md` were rewritten for the mailspace-only shape, `docs/`
now explains that its contents are the historical record of the pre-split
project, and `VISION.md` and the release smoke checklist moved to `vivi-mail`
along with the code they describe.
