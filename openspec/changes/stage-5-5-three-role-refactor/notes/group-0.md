# Group 0: workspace and protocol

## 0.1 The scripts

- `scripts/coverage.sh <crate> [<file> ...]` runs `cargo llvm-cov -p <crate> --all-targets --lcov`, writes
  `target/coverage/<crate>.lcov`, lists each uncovered line of the files named (or of the crate's `src/`), and exits 1
  when any is left. Tried on `ultragraphdb_error_log`: 8 tests, 0 uncovered lines.
- The held-back lints (`indexing_slicing`, `arithmetic_side_effects`, `cast_possible_truncation`, `cast_sign_loss`)
  stay out of this change (the user, 2026-11-02). `scripts/held_back_lints.sh`, written for the requirement before it
  was dropped, denies them for the crates named; nothing in the change runs it, and it awaits the user's decision to
  keep or remove.
- Phase 1 leaves bare `unimplemented!()` bodies: `clippy::unimplemented` warns by design, the warnings are ignored
  through the phase-2 run, and a warning left after phase 4 names an overlooked body (the user, 2026-11-02).
  `ULTRA_STYLE.md`'s lint table and its phase-1 paragraph now say so.

## 0.2 No skeleton crates

The task as drafted created every crate of the design's table here, empty. That contradicts the repository's rule that
a crate exists only once it is implemented in full (2026-10-28). The task now states that rule: each group creates its
own crates, and the group creating the first crate of a new folder adds the folder to `[workspace] members`.

## 0.4 Decisions for the user's review before groups 9 and 10

Taken in drafting (design, *Decisions taken in drafting*):

| Decision | Where it binds | Basis |
|---|---|---|
| Principals live in the metadata group | group 6, group 9 (SCRAM against the group) | the user, 2026-11-01; backlog item 3 |
| The client's credit is automatic: the shells expose no `grant`, the core's grant takes `NonZeroU32` | group 10 | backlog item 9; no caller needs manual flow control, and a zero grant is refused on the wire |
| A commit in flight is in doubt once sent: `ConnectionLost` before its frame is sent, `InDoubt` after, on any attempt; `RetriesExhausted` folds into `InDoubt` | group 10 | backlog item 9; one in-doubt state, resolved through the session |
| The Welcome's write-set bound comes from the log's round size | group 9 | backlog item 1; a client never sends a write set the log cannot hold |

Taken in the final review (2026-11-02, approved by the user):

| Decision | Where it binds |
|---|---|
| A session keeps one counter; each partition keeps its own session table and accepts the gaps the session's commits elsewhere leave; the metadata group registers each session with its principal; a retry the frontend cannot route is asked of every partition | groups 4, 6, 9, 10 |
| Skolem IRIs are `…/.well-known/genid/{bucket}-{random}`, with no part of the ID but its bucket | groups 3, 9 |
| The client endpoint speaks TLS 1.3 from the first byte when it names a certificate and key, clear text only with `plaintext = true`; `SCRAM-SHA-256-PLUS`, `EXTERNAL`, `OAUTHBEARER` and TLS between roles wait for stage 8 | groups 8, 9, 10, 11 |
| Roles authenticate each other with SCRAM against a shared role secret, each verifying its peer locally | groups 9, 11 |
| Exit codes follow `sysexits`: 0, 69, 70, 74, 78, and 128 plus the signal for a killed role | group 11 |
| One owner per fact (Shard-per-Core note, P24): a partition writes every entry its subjects' statements derive, reverse entries included; a first reference to another partition's entity registers at its home, which gets or creates the ID and adds the referrer's bucket group to the entity's referrer set; reverse reads fan out through that set; a partition stamps exactly one above its last delivered close and asks the sequencer for a close when its sequence would run out; catalog versions take effect at `c + 2`; no change streams or split reverse parts | groups 2–9, 6b |
| A statement without a statement ID is named to clients by its quad; one with a statement ID, which carries properties or repeats a quad as a parallel edge, by its reifier, a skolem IRI minted as a blank node's with the statement's bucket and held in the dictionary; result batches return reifier IRIs, never statement IDs | groups 3, 8, 9 |

The statement-ID question raised here before group 8 is decided (2026-11-02): statements reach clients by their quad
when they hold no statement ID and by their reifier IRI when they hold one, as the decisions table states.
