# Linear Integration Phase 01: Dry-run exporter

## Interpreted phase problem

The Linear mapping is now defined, but the first implementation step must prove
what would be projected from a project mailspace before any Linear write path is
allowed. The `~/work/ianzepp/vivarium` mailspace is seeded with Linear
integration tasks, needs, and wants for dogfooding, but repeatable tests still
need a fixture. A useful dry-run must work against a selected project root or
fixture and must not treat an empty board as proof that the exporter is
complete.

## Normalized phase spec

### Goal

Add a read-only dry-run exporter that reports which Linear records would be
created, updated, skipped, or considered ambiguous for one explicit Vivi project
mailspace.

### Functional requirements

- Accept one explicit project root.
- Read through public Vivi command/read-model semantics, not by parsing `.vivi`
  storage files directly.
- Report counts for:
  - would-create
  - would-update
  - would-skip
  - ambiguous/conflict candidates
- Group records by Vivi kind: task, need, want, mail, memo, graph, and campaign
  where applicable.
- Include the intended Linear target for each projectable kind:
  - task -> issue
  - need -> issue
  - want -> backlog issue
  - campaign -> project
  - receipt -> issue comment or project update
- Explain local-only skips for memo, raw mail chatter, roles, and graph
  scheduling state.
- Make no Linear API or CLI writes.

### Constraints

- No inbound Linear-to-Vivi sync.
- No direct mutation of `.vivi` storage.
- No token storage or credential printing.
- No historical bulk import in this phase.
- No dependency projection from Vivi graph to Linear in this phase.
- No Linear attachment creation until need `0af2fe17` settles the durable
  external reference identity and URL pattern.

### Out of scope

- Creating Linear projects or issues.
- Binding-store persistence.
- Posting factory receipts.
- Bidirectional sync.
- A general analytics/query DSL.

## Repo-aware baseline

- `docs/linear-integration-goal.md` owns the current mapping and pilot conflict
  rule.
- `vivi board --project <root> --json --graph` exposes the compact open-work
  board and graph frontier.
- `vivi task|need|want list --project <root> --for <role> --json` expose
  per-role lists.
- `vivi want list --status all --json` can include closed wants.
- `vivi task|need|want show --project <root> <handle> --json` can load full
  selected records.
- Current `vivarium` board is empty, so tests need a fixture or seeded
  mailspace.

## Implementation shape

Prefer the smallest external dry-run surface first. A script or subcommand is
acceptable only if it can be tested without Linear credentials.

Candidate minimal command shape:

```sh
vivi-linear-dry-run --project <root> --linear-team IAN --linear-project '<name-or-id>' --json
```

If implemented inside `vivi`, use a namespaced command such as:

```sh
vivi linear dry-run --project <root> --team IAN --project-name '<name>' --json
```

The first version should not require a Linear token because it is not resolving
remote state yet. It can mark all projectable unbound records as `would-create`.
Binding-aware `would-update` belongs to the next phase unless a local binding
file is already provided.

## Stage graph

1. **Shape output model**
   - Define dry-run JSON fields for project root, target team/project, counts,
     records, skips, and warnings.

2. **Collect local records**
   - Discover roles from the mailspace read model.
   - Collect task, need, and want list records by role.
   - Use `show` only for selected records that need body/provenance detail.

3. **Classify projection**
   - Project task/need/want records.
   - Skip memo, role, raw mail chatter, and graph scheduling state with reasons.
   - Include graph frontier summary as local-only context.

4. **Render dry-run**
   - Text output for humans.
   - JSON output for tests and later automation.

5. **Fixture/test**
   - Build or reuse a fixture mailspace with at least one task, need, want,
     memo, mail, and graph marker where cheap.
   - Assert zero external Linear writes.

## Checkpoints and gates

### Checkpoint target

A dry-run against a non-empty fixture or selected project root reports proposed
Linear projection without creating or updating Linear records.

### Batching / split decision

Execute as one phase if implemented as a small read-only command. Split before
binding-store or Linear remote lookup work.

### Gate plan

- Verify the command runs with no Linear token.
- Verify project root is explicit.
- Verify empty mailspaces produce an honest empty report with a warning, not a
  false success claim.
- Verify local-only kinds are counted and explained.

## Validation

For documentation-only completion:

- Inspect `docs/linear-integration-goal.md` and this delivery spec.

For implementation completion:

- `cargo fmt --check`
- `cargo test --test hygiene`
- Targeted CLI/integration test for the dry-run surface
- Wider `cargo test` if the implementation touches shared mailspace code

## Open decisions

1. Should the dry-run be a native `vivi linear dry-run` subcommand or a separate
   helper script first?
2. Should the first implementation read internal Rust mailspace APIs directly,
   or should it shell through the installed/current `vivi` CLI to enforce the
   public-surface boundary?
3. Where should the eventual binding file live: project-local `.vivi/`, a
   repo-visible config file, or user-local state?

## Phase checkpoint

Not implemented yet. This spec is ready to lower into a factory phase once the
command placement decision is made.
