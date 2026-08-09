# Vivarium 7.2.1

Vivarium 7.2.1 makes cadence-driven role dispatch self-contained on the board.

## Changes

- Board identities now include the configured role `model` and `thinking`
  capacity in text and JSON output.
- An `overdue` schedule now sets `schedule.action_required` to `true` in JSON
  and prints `ACTION REQUIRED` in text output.
- A `due` schedule remains advisory. No mailspace schema changes are required.

`vivarium` and `vivi-pty` remain versioned in lockstep at 7.2.1.
