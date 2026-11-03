# Group 8, wire: finished by the coordinator

The wire agent split its work into three parts (`group-8-wire-roles-512.md`, `-519.md`, `-525.md`: the role commands'
bodies, views and builders) and was stopped, on the user's instruction, partway through bringing the client bodies to
`PROTOCOL.md`. The coordinator finished it on 2026-11-03.

## The client bodies

`PROTOCOL.md` and `generate_bodies.py` already held the new layouts; the suites did not. Brought in line, with the
generator as the independent oracle:

- `ErrorCode`: 27 codes (`TimedOut` 24 and `WrongPartition` 26 retryable, `LevelRefused` 25, `AheadOfWatermark` 27).
- `Begin`: level, deadline and token; the golden `begin` is read-only at level 2, deadline 50 ms, token (2, 41, 30).
- `Prepare`: an answer carries a result type per unbound slot and then the reifier column's; `PrepareView::check_reply`
  and the statement cache say so.
- `Write`: a statement with an ID is named by its reifier's skolem IRI (kind 4: graph and reifier; kinds 6 and 7 by
  reifier or, flag bit 0, by quad); the golden Write holds 19 terms and 63 args.
- `Result`: the reifier column last; `ResultBatch::push_row` refuses a row of the wrong width before its reifier.
- `Commit`: three sections (results, the term table's records and text); `Register`, `Commit` and `Drop` name sessions
  by 16 bytes; a reply carries the token of its commit version's epoch.

## Bug fixed

`ErrorReply` built a `WrongPartition` frame whose detail set bits 24–31, which its own view refuses; its `frame` now
refuses such a detail as `FieldValue` `"detail"`. Found by `body_fields` within 30 s; a test pins it.

## The clients

`ultragraphdb_client_core` now names a session by its 128-bit identity, refuses a role command from a server, and its
test server answers by the protocol: a read-write Begin's read version 0, a read-only snapshot's its version, tokens on
Commit and Drop replies, versions in epoch 1, node results named in the reply's term table, the reifier column.
Every expected value in the client suites is now read from its source rather than restated (user, 2026-11-03): a commit
version from `TestServer::version()`, a hand-built reply's token from `TestServer::token`, a session from the
registered `Session`, every golden value from the golden frame itself through the wire crate's views, the shells'
release from the golden Hello's header, the damaged-golden case count summed from the frames' lengths.

## Fuzz

`body`: every role command's view added to the dispatch (its `match` did not compile against the 19 new commands);
seeds refreshed from the current body and role goldens, all 67 replaying. `body_fields` (structure-aware through
`arbitrary`): 16-byte sessions, tokens on replies, no statement-ID flags, the reifier column; 617,860 runs in 60 s after
the `ErrorReply` fix, no failure.

Mutation testing of this group is deferred to task 12.1 (user, 2026-11-03).
