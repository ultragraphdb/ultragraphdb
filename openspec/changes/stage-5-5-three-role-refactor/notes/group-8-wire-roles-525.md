# Group 8, wire: role commands 525–530

The bodies, views and builders of Entries (525), Reference (526), CloseRequest (527), Closed (528), DefaultEntry (529)
and Report (530), `PROTOCOL.md` §13.14–13.19, in `ultragraphdb_protocol_wire`. A fork of the wire agent; the sibling
forks did 512–518 and 519–524.

## PROTOCOL.md changes (§13.14–13.19)

- **§13.14 Entries**: `epoch` has a rule, at most 2⁴² − 1 and 0 when the partition has closed no epoch (an `at` 3 read of
  a partition before its first close), checked after `end`. `FieldValue` `"entry"` and `"entry order"` name the entry's
  index. The cross-batch key order ("after the range's previous batch") is stated as the receiver's, since a view sees
  one frame.
- **§13.15 Reference**: the command header gains `reply` u8 at 0 (0 the request, 1 the home's reply). Without it a
  request and a reply could not be told apart: a reply's fields can all be 0 (close 0 when the home has had no close
  delivered, generation 0 for no group), and the request's rules (`entity` zero exactly when `iri` names it) contradict
  the reply's (an accepted registration's `entity` filled although `iri` names it). The Order line is rewritten: fields
  common to both (partition, group, iri, entity well formed, close an epoch, generation 0 without a group), then the
  request's (outcome and close 0, the entity named one way) or the reply's (outcome 0, 7 or 26; accepted: entity filled,
  generation ≥ 1 with a group; refused: generation and close 0, entity as sent). A refused reply's `entity` is "as sent".
  `FieldValue` `"registration"` names the registration's index, `"reference term"` the entry's kind.
- **§13.17 Closed**: the first partition is at least 1; `epoch` at most 2⁴² − 1. "`epoch` 0 in a request" cannot be a
  view's rule (a reply of epochs all 0 holds the request's bytes), so it is stated as the storage process's, which
  receives the request: `ClosedView::check_request`.
- **§13.18 DefaultEntry**: `found` 0 or 1; `FieldValue` `"default entry"` names the entry's kind; "`found` 0 in a request"
  is the frontend's check (`DefaultEntryView::check_request`), for the same reason.
- **§13.19 Report**: a refusal (kind 2) takes one or two entries, one when the term at fault is the predicate's IRI
  itself (a term table holds no duplicate, so "two entries" could not be met); the second entry is kind 1 to 4, never a
  triple term (whose parts would need entries of their own). `CountMismatch` `"report terms"` names the most the kind
  takes; the reply holds the request's bytes.

## Public API

- `Entries::new(range: u32, end: EntriesEnd, epoch: u64)`, `push(&mut self, key: &[u8], value: &[u8])`,
  `range/end/epoch/entries()`; `EntriesView::{range, end, epoch, entries() -> KeyValues<'a>}`;
  `EntriesEnd { More, Ended, Limit }` with `number`/`from_number`.
- `Reference::request()`, `Reference::new(reply, registrations, terms)`, `register_iri(&mut self, partition: u32,
  group: Option<BucketGroup>, iri: &str, generation: u64) -> usize`, `register_entity(&mut self, partition, group,
  entity: SystemId, generation) -> usize`, `into_reply(self)`, `accept(&mut self, index, entity: SystemId, generation:
  u64, close: u64) -> bool`, `refuse(&mut self, index, outcome: u16) -> bool`, `is_reply`, `registrations()`, `terms()`,
  `iri(index) -> Option<&str>`. `ReferenceView::{is_reply, len, is_empty, registration(i), registrations(), iri(i),
  terms}`. `ReferenceRegistration::{request(partition, group, iri: Option<u32>, entity: Option<SystemId>, generation),
  accepted(self, entity, generation, close), refused(self, outcome), partition, group, iri, outcome, generation, close,
  entity, encode}`.
- `CloseRequest::new(partition: u32, epoch: u64)`, `CloseRequestView::{partition, epoch}`.
- `Closed::new(Vec<ClosedPartition>)`, `Closed::request(&[u32])`, `partitions()`; `ClosedView::{len, is_empty,
  partition(i), partitions(), check_request()}`; `ClosedPartition::{new(partition, epoch), partition, epoch, encode}`.
- `DefaultEntry::request(tenant, predicate)`, `with_found(bool)`, getters; `DefaultEntryView::{tenant, found,
  predicate, check_request()}`.
- `Report::{passed(partition, tenant, version), refused(partition, tenant, version, predicate: &str, at_fault:
  RequestTerm) -> Result<Report, ProtocolError>, build_failed(partition, tenant, version, predicate: &str)}`, getters;
  `ReportView::{partition, kind, tenant, version, predicate(), at_fault(), terms}`; `ReportKind { Passed, Refused,
  BuildFailed }`.
- Crate-private: `utils/term_kind.rs` (`term_kind(&RequestTermView) -> u8`); test helper `tests/utils/term_records.rs`
  (hand-laid term tables, `refusal::<V>`).

## Golden frames (`testdata/golden/roles/`, `generate_roles.py` block 525–530)

`entries`, `reference`, `reference_reply`, `close_request`, `closed`, `closed_reply`, `default_entry`,
`default_entry_reply`, `report_refused`, `report_passed`, `report_build_failed`, each laid out from §13's tables.

## Phases

1. Surface compiling with bare `unimplemented!()` bodies; `SURFACE READY` posted.
2. Suite written: 69 unit tests over 16 test modules. Bazel (`--test_arg` filtered to the six commands' modules): **69
   failed, 0 passed, every failure `not implemented`**.
3. Inputs reviewed for coincidences: every golden field distinct from its neighbours (Entries count 3, range 2, end 1,
   epoch 0x1234; Reference partitions 3, 5, 8, groups 0x2A and 0x10, generation 4 → 5, close 41; CloseRequest 7 and 30;
   Closed partitions 2, 4, 9, epochs 28, 0, 31; DefaultEntry tenant 3; Report partition 6, tenant 4, versions 12,
   13, 14). An entity byte change in the reply tests first hit a byte that was already 0 (byte 32, the type prefix's
   high byte); it now clears byte 33.
4. Implemented: **69 passed** under Bazel; `cargo clippy -p ultragraphdb_protocol_wire --all-targets -- -Wclippy::all`
   reports nothing in these files.

Corner cases and their tests:

| Case | Test |
|---|---|
| Each end's and kind's byte; 3, 4, 255 (and 0 for a kind) naming none | `entries_end_tests`, `report_kind_tests` |
| A partition record's bytes; all zero; all ones | `closed_partition_tests` |
| A registration's fields at their offsets; by entity with `ABSENT` group and iri; accepted; refused keeping an entity named by ID and clearing one named by IRI | `reference_registration_tests` |
| Entries: unused bytes 9, 15, 24, 127; end 0, 2, 3; epoch 0, 2⁴² − 1, 2⁴²; end before epoch; an entry past the section; padding at its body offset; a removal; equal, descending, prefix-after-longer and high-byte keys; a prefix before its extension and an empty key taken; count 0, 2, 4 against 3; an empty batch | `entries_view_tests` |
| CloseRequest: unused 4, 7, 16, 127; partition 0 before epoch 0; partition `u32::MAX`; epoch 0, 2⁴², `u64::MAX` refused, 1 and 2⁴² − 1 taken; a body | `close_request_view_tests` |
| Closed: header bytes 0, 127; no partition; 31 bytes; zero bytes 4 and 7 at their body offset; partition 0, a repeat, a descent; 1 and `u32::MAX`; epoch 2⁴², record order; `check_request` on a reply's first and last epoch | `closed_view_tests` |
| DefaultEntry: unused 9, 127; found 2; 0 and 2 entries; a blank node, a literal, a language literal; a bad term kind; `check_request` on the reply | `default_entry_view_tests` |
| Reference: reply 2; unused 1, 127; a literal entry; no registration; 143 bytes; zero bytes 14, 15; partition 0, group 2¹⁸, iri at the table's length, an entity with a reserved bit, a generation without a group (request and reply), close 2⁴², an entity with a zero type prefix; group 2¹⁸ − 1, iri 1, partition `u32::MAX`, close 0 and 2⁴² − 1, outcome 7 taken; request outcome, close, entity named both ways and neither way; reply outcome 5, accepted without entity or generation, refused with generation, close or entity; a refusal of an entity named by ID | `reference_view_tests` |
| Report: unused 5, 7, 24, 127; partition before kind before version; kind 0, 4, 255; each kind's term counts both sides; a literal or blank node first; a triple second; a blank node, language literal and IRI second taken; one entry naming predicate and fault | `report_view_tests`, `report_tests` |

Bazel after phase 4 and the mutants fix: the crate's unit suite **457 passed** (this part's 70 among them), bdd 23
scenarios passed.

## Outside this part's files

To get a green crate baseline for mutants I updated four tests of the parent's Command work, which still expected the
19-command registry: `command_tests` (REGISTRY of 38 rows with 512–530; known numbers 1–19 and 512–530; unknown samples
531 and 767 in place of 512), `header_view_verify_tests::test_an_unknown_command_is_refused_by_number` (531 and 767
unknown, 512 and 530 known), and the bdd step "a frame carries a retired command's number" (every number but the 38
assigned, `65_536 − 38`). Tests for `is_role` and `on_client_connection` remain the parent's.

## Phase 5: mutation testing

Per-file first runs (`scripts/mutants.sh ultragraphdb_protocol_wire <file> --diff cabae5ec`, the script as it then was):

| File | Mutants | Caught | Unviable | Missed |
|---|---|---|---|---|
| `types/entries_end.rs` | 7 | 6 | 1 | — |
| `types/report_kind.rs` | 7 | 6 | 1 | — |
| `types/closed_partition.rs` | 7 | 6 | 1 | — |
| `types/reference_registration.rs` | 32 | 25 | 6 | 1 |
| `utils/term_kind.rs` | 2 | 2 | 0 | — |

The survivor, `reference_registration.rs:59:13`, deleting `outcome` from `accepted`'s struct expression: no test
accepted a registration refused before. `test_accepting_a_refused_registration_clears_its_outcome` now does; no run has
confirmed the kill yet.

The other files' runs were killed (SIGKILL, exit 137) mid-run, and then `scripts/mutants.sh` changed under the run to
one run per crate (`scripts/mutants.sh <crate> --diff <base>`, three runs per crate and base, a file argument refused).
A crate-wide run spends the crate's budget, which this crate shares with the parent and both sibling parts, so this part
does not start one: the crate's run belongs to the wire agent once every part of the crate is done. Open for that run
(and task 12.1 if it leaves survivors): every logic file of 525–530 but the five above —
`entries/{mod,entries_body_builder}.rs`, `entries_view/entries_view_body_view.rs`,
`close_request/close_request_body_builder.rs`, `close_request_view/close_request_view_body_view.rs`,
`closed/{mod,closed_body_builder}.rs`, `closed_view/{mod,closed_view_body_view}.rs`,
`default_entry/default_entry_body_builder.rs`, `default_entry_view/{mod,default_entry_view_body_view}.rs`,
`reference/{mod,reference_body_builder}.rs`, `reference_view/{mod,reference_view_body_view}.rs`,
`report/{mod,report_body_builder}.rs`, `report_view/{mod,report_view_body_view}.rs`, and the re-run of the
`reference_registration.rs` kill. The ledger holds one per-file run each for `entries/entries_body_builder.rs`,
`entries_view/entries_view_body_view.rs` and `close_request/close_request_body_builder.rs` under the old scheme; they
were killed before testing a mutant.

## Choices the spec left open

- `CountMismatch` `"entries"`: `count` the entries the section holds, `expected` the header's `entries`.
- `Reference::refuse` takes any outcome; the view refuses one but 7 and 26. A refused registration's entity is cleared
  when its IRI names it and kept when its ID does, which is "as sent".
- `Report::refused` refuses a triple term at fault, and a literal the term table refuses, as `BadTerm` (530, section 0,
  entry 1).
- `ClosedView::check_request` and `DefaultEntryView::check_request` give the request-only rule to the receiver of the
  request; `from_frame` reads both directions.
