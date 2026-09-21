# Vivarium 9.3.0

Vivarium 9.3.0 adds `vivi boot`, the project frame in one read, and removes
two performance defects that made large boards expensive to inspect: short
handles no longer grow to prove themselves unique, and `want list` no longer
derives a column that was empty for every row.

## Boot

`vivi boot` renders the whole project frame in a single bounded read: seat
bindings against observed process state, declared cadences and their silence,
unabsorbed mail, open handles with verdicts, registered goals with register
tallies, memos, charter heads for roles holding open work, and the undispatched
backlog sliced into seat-sized groups.

```sh
vivi boot --project /path/to/project
```

It is read-only, stateless, and idempotent: two runs are comparable, and boot
never absorbs, closes, promotes, dispatches, or writes. Act on the frame with
the ordinary verbs. That makes it as useful after a context compaction as at
the start of a session.

Every section caps its rows and the digest closes with a truncation manifest
naming each cap and how much it omitted, so coverage is complete and the
length is bounded.

Handles carry a closed verdict vocabulary: `open`, `blocked`, `stale`, `live`,
`unbound`, `unverified`, `dead`, `zombie`, `remote`, `unknown`. A seat on a
subagent harness reads `unverified`, because an OS process id is not a valid
liveness signal for one; a probe can supply the harness's real seat count.
A registered goal whose `**Status**:` line claims a different completion count
than its own register is reported as `MISMATCH` with both numbers.

### Probes

Boot renders the facts Vivi owns. Facts it cannot own — git ancestry, lane
state, the live seat count of a subagent harness — arrive through probes the
project declares in `.vivi/mailspace.toml`:

```toml
[[probes]]
name = "example"
command = "scripta/boot-probe"
```

A probe is an executable, resolved against the mailspace root, that prints one
JSON object on stdout: `facts` (flat preamble lines), `sections` (named blocks
of lines), and `verdicts` (`handle`, a verdict slug, and an optional detail).
Verdicts merge onto the handle inventory by full handle or by a unique handle
prefix of at least four characters. A probe that is missing, exits non-zero, or
prints unparseable JSON is reported under `probes skipped` and never fails the
run, so a project with no probes still gets every native section.

## Breaking and behavior changes

- **Short handles are a fixed width.** A handle is the first eight characters
  of the message id, so it is a pure function of that one id and does not
  change when unrelated messages arrive. Handles previously grew until their
  prefix was unique across the mailbox. Two messages that share a prefix now
  share a handle, and resolving such a token reports it as ambiguous instead
  of guessing; address that message by its full id. This supersedes the
  handle-growth note in the 9.2.0 release notes.
- **`want list` drops the derived-task column.** The column and its key in
  `want list --json` are gone. It could only be populated for a want that was
  never promoted, and a want carries no task association: the work graph holds
  dependency chains, and a task created from a source records that source on
  the task itself.

## Performance

Short handles no longer require a uniqueness proof. Establishing that a handle
was unique meant inserting every prefix of every id: 1.26 million string
inserts and about 19 MB of transient allocation for a 74k-message mailbox,
roughly a second, rebuilt by each command that materialized handles. Handles
are now computed per id with no cross-record work.

`want list` read the entire mailspace event log once per want to fill its
derived-task column — 29 million event rows scanned to surface 171 matches on
the faberlang mailbox, measured at 62.6 seconds. Removing the column removes
the scan.

Measured on the faberlang mailspace (74,114 messages, 96,083 events):

- `vivi boot` completes in 1.6 seconds.
- `want list --for mind` went from 62.6 seconds to 0.14 seconds.
- `cycle intake --for mind` went from 64.7 seconds to 1.22 seconds.
- `task list` and `need list` complete in about 0.13 seconds each.
