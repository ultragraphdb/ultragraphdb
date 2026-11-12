# Group 1: the wire, phases 1 to 3 (tasks 1.1 and 1.2)

Phase 4 (task 1.3) and phase 5 (task 1.4) follow, judged by the suite below. Every test named here is in
`ultragraphdb_protocol/ultragraphdb_protocol_wire/tests/` unless a crate is named.

## Step A: `ultragraphdb_core_body` (P11)

**Moves.** By `git mv`, each file whole, then edited in place:

| From `ultragraphdb_protocol_wire` | To `ultragraphdb_core/ultragraphdb_core_body` |
|---|---|
| `src/utils/sections.rs` | `src/types/sections/mod.rs`: `Sections`; its free functions in `src/utils/layout.rs` (`write_sections`, `write_sections_into`, `align8`, `as_u32`, `u32_at`, `u32_bytes`, `records`, `u32s`, `text`) |
| `src/types/string_list.rs` | `src/types/string_list/mod.rs`: `StringList`, `EMPTY` now public |
| `src/utils/string_writer.rs` | `src/types/string_writer/mod.rs`: `StringWriter`, now public |

`expect_command`, the one item of `sections.rs` that names a frame, stays in the wire, in `utils/command_header.rs`.

**The error.** Design decision 6 gives the crate no dependency, so it raises an error of its own, `BodyError(pub
BodyErrorEnum)`, one variant per rule of `PROTOCOL.md` §4.1 and §4.3 (`BodyTooShort`, `SectionOutOfBounds`,
`SectionOffset`, `PaddingNotZero`, `TrailingBytes`, `SectionLength`, `CountMismatch`, `NotUtf8`, `IndexOutOfRange`),
naming no command: the wire names the command, the catalog will name the entry. The wire's new `utils/body.rs` maps
each variant to the `ProtocolError` of the same rule and fields with its command (`refusal`) and wraps the readers
(`read_sections`, `read_string_list`, `records`, `u32s`, `text`); 113 files of the wire's `src/` had their imports and
calls rewritten mechanically. Every refusal is the same `ProtocolError` value as before. The wire no longer re-exports
`StringList`, which no crate outside it named; its views return `ultragraphdb_core_body::StringList`.

**The crate.** `Cargo.toml` (workspace lints, no dependency, `ultragraphdb_tool_bdd` for tests), `BUILD.bazel` (library,
doc, one `rust_test` over `tests/mod.rs`; no `bdd` target, since no scenario names the crate), its row in `AGENTS.md`'s
crate table and the wire's row naming it. 27 tests mirror `src/`: `Sections` (7), `StringList` (7), `StringWriter` (4),
the layout's functions (6), `BodyError` (3); every `BodyError` variant is constructed through `Sections::read`,
`StringList::read`, `records`, `u32s` or `text`. Expected values are bodies laid out by hand from §4.1 and §4.3, among
them §8.3's nine empty sections of 72 bytes, every offset 72.

**Fuzzing.** `fuzz/ultragraphdb_core_body` holds the targets `sections` (a choice byte picks 1, 3, 7 or 9 sections and a
fixed part of 0 to 24 bytes; an accepted body written again gives its bytes) and `string_list` (an accepted list written
again gives its two sections); seeds under `testdata/fuzz_seeds/{sections,string_list}/`, five files laid out by hand.
`cargo +nightly fuzz build` builds both, and each ran once over its seeds (`-runs=0`) without a failure; the timed runs
are task 13's.

**Counts.** Before step A: `ultragraphdb_protocol_wire_tests` 720 passed, the wire's `bdd` 52 scenarios (104 steps)
passed, `ultragraphdb_protocol_transport_tests` 180 passed. After step A, before step B: the same 720, 52 and 180, and
`ultragraphdb_core_body_tests` 27 passed.

**Left in place.** The wire's `tests/types/string_list/string_list_tests.rs` still tests a string list read in a
Describe body, its refusals naming command 12, now through `utils/body.rs`. Its mirror would be
`tests/utils/body_tests.rs`; moving it leaves `tests/types/string_list/mod.rs` to remove, which waits for approval.

## Corner cases, enumerated per changed body before the suite

| Body | Cases | Tests |
|---|---|---|
| Warnings (§4.9), in Result 9–11, Prepare 7–9, Install 4–6 | none; one; 64 and 65 (`CountMismatch` `warnings`); 25 bytes (`SectionLength`, checked before the messages); messages' list broken; bytes 20–23 nonzero (`PaddingNotZero`); `diagnostic` bits 24–31 set, language 0, 5, 255 refused and 1 to 4 read; `offset + length` 2³² − 1 read and 2³² refused (`location`); `message` = count (`IndexOutOfRange`); messages of 512 and 513 bytes; two warnings sharing a message; `source` 1 read in an Install's reply, refused in a Result's; warnings in a Prepare request and a pattern's answer (`CountMismatch` `warnings`) | `warnings_tests` (9), `warning_tests`, `warning_view_tests`, `install_view_tests::stage_6`, `prepare_view_tests::stage_6`, `result_view_tests::stage_6::test_a_querys_first_batch_carries_its_warnings_of_source_0` |
| Result (§9.1, §10) | twelve empty sections read; nine refused (`BodyTooShort`, 72 of 96); reifier tag `0x01` read, `0x30` refused (`reifier`, the row); an IRI at the golden's reifier cell now read; datatypes 34 to 36: `0x41` of dimension 2 read, of 0 and short refused; `0x50` with unit `m` read, units of 0 and 16 bytes and not UTF-8 refused; `0x51` 1 and 12 read, 0 and 13 refused; `0x30` codes 34, 35, 36 read, 37 refused (`BadTerm`) | `result_view_tests::stage_6` (5) and the updated `test_a_patterns_batch_holds_its_columns_and_reifier_column`, `test_the_golden_querys_batch_reads_its_names_and_flag` |
| Prepare (§8.1) | ten sections, language 3 read with column type 6 and a warning; built the same; language 4 refused; warnings in a request and a pattern's answer | `prepare_view_tests::stage_6` (3), `test_the_language_is_0_to_3_after_the_unused_bytes` |
| Query (§7.4), Language | language 3 built and read; 0, 4, 255 refused; `Language` 0 to 3, 4 and 255 none | `query_view_tests::stage_6` (2), `test_the_language_is_gql_sparql_or_uql`, `language_tests` |
| Describe (§8.3) | request of nine empty sections, 72 bytes, read and built; every kind 1 to 20 read; kinds 0 and 21; bytes 1–3; `name` = count; 15-byte section; an offset inside the entry before; bytes past section 8; bytes left after the last entry; order by kind, by name bytes (`B` before `a`), a repeated kind and name; a source not UTF-8; retired names: renamed and retired read, 23 and 25 bytes, bytes 2–3, `item` 0 and 6, `fate` 0 and 3, `current` `ABSENT` or past the count for fate 1, a string for fate 2, `version` 0, `until` for fate 2; `extra` 1 refused; entries read back; entries built with a retired name's current name by index; the schema id over the entries and its golden value | `describe_view_tests::stage_6` (9), `describe_tests::stage_6` (3), `describe_entry_tests`, `describe_entry_view_tests`, `retired_name_tests` (2), `retired_name_view_tests`, `entry_kind_tests`, `retired_item_tests`, `retired_fate_tests`, `test_the_schema_id_follows_the_declarations_alone` |
| Describe continuation | an answer past `max_frame` of 4,096: at least three frames, each within it, one command header, the last alone `LAST`, sections concatenating to the answer laid out by hand, read back whole; an answer that fits is one frame; a second frame of another command header (`describe`); a frame of another command; per frame, u32 sections of 3 and 5 bytes, record sections of 8, text ending inside a character; no frame read | `describe_frames_tests` (5) |
| Install (§8.4) | reply with one warning read and built; source 1; each of sections 2 to 6 in a request (`report`); `staged` in a request; a migration report without `staged`; `staged` 5 with its report read and built, its top byte set; reports not UTF-8 in sections 2 and 3; byte 32 (`ReservedNotZero` 160); seven sections, the request built | `install_view_tests::stage_6` (7), `install_tests::stage_6` (3) |
| Purge (§8.5) | request and reply built; decode; 0, 10,000 and 10,001 IRIs; a blank node (`purge term`, kind 2); `epoch` 2⁴² and 2⁴² − 1; `removed` in a request, above and at the count, 0 in a reply; byte 20 | `purge_tests`, `purge_view_tests` (5) |
| Migrate (§8.6) | request, reply and failed reply built; decode; `staged` 0; `action` 0, 6 and 1 to 5; `state` 10 and 1 to 8; `refusal` with state 4, 2200, 2300, 2201, 2299, a cancelled run's 0; `epoch` 2⁴² and 2⁴² − 1; a request's sections; sections not UTF-8; a message with no refusal; bytes 18, 19, 32, 127 | `migrate_tests`, `migrate_view_tests` (5), `migrate_action_tests`, `migrate_state_tests` |
| Reference (§13.15) | get-only request read, mode 0 read, mode 2 refused; statuses 1 to 4 accepted; status 0 and 5 in an accepted get-only reply, status with mode 0, status in a refused reply, an absent entity with an ID, generation or close on a retracted or deleted one, status in a request (`registration`); outcome 30 with mode 0 read and with mode 1 refused, 17 and 26 with mode 1 read; the builder's mode and status | `reference_view_tests::stage_6` (4), `reference_tests::stage_6`, `reference_mode_tests`, `reference_status_tests` |
| Drop (§6.5) | kinds 1 and 2 with `catalog` 7; kind 3 with 7 built and read; kind 4 with 0; unused bytes now 1–7 and 80–127 | `drop_command_view_tests::stage_6` (2), `test_unused_bytes_kind_and_tenant_are_checked_in_order` |
| Operation and ConditionalWrite (§7.3, §6.7) | kind 6 built and read (graph `ABSENT`); kind 5 of a named graph; 3 and 1 args (`CountMismatch` `args`, expected 2); kind 7; `delta`, `properties`, `links` set; the instance `ABSENT`; a ConditionalWrite of kinds 5 and 6 built and read; one of kind 7 | `operation_view_tests::stage_6` (4), `conditional_write_view_tests::stage_6` (2) |
| Codes and commands (§3, §9.4, §11.3) | codes 30, 31 and 32; `Fenced` retryable and running again as `Busy`, `EntityDeleted` neither; commands 24 and 25 in the registry, `ALL` and their own answer; credit spent by Result, Entries, Describe and Catalog alone | `error_code_tests`, `command_tests` |
| Credit and streams (transport) | five Describe frames under credit 4; two Catalog frames under 1; an Error after a Describe frame with no credit; Purge and Migrate each opening a stream, refused on it, the connection open | `ultragraphdb_protocol_transport`: `outbound_tests::stage_6` (3), `test_a_purge_and_a_migrate_open_streams_and_a_refusal_keeps_the_connection` |

## Phase 1: the surface

New, every body a bare `unimplemented!()`: `Warning`, `WarningView`, `Warnings`; `EntryKind`, `DescribeEntry`,
`DescribeEntryView`, `RetiredItem`, `RetiredFate`, `RetiredName`, `RetiredNameView`, `DescribeFrames`; `CatalogFrames`;
`Purge`, `PurgeView`, `Migrate`, `MigrateView`, `MigrateAction`, `MigrateState`; `ReferenceMode`, `ReferenceStatus`;
`Command::spends_credit`; the constants `WARNINGS_MAX`, `WARNING_MESSAGE_MAX`, `WARNING_SIZE`, `PURGE_IRIS_MAX`,
`DESCRIBE_ENTRY_SIZE`, `RETIRED_NAME_SIZE`, `CATALOG_BYTES_MAX`. Added to existing types: `with_warnings`/`warnings` on
`Install`, `Prepare`, `ResultBatch` and their views; `with_staged`, `staged`, `with_migration_report`,
`migration_report` on `Install` and `InstallView`; `with_entries`, `entries`, `frames` on `Describe`, `entry_count`,
`entry`, `entries` on `DescribeView`; `frames` on `Catalog`; `with_catalog`, `catalog` on `DropCommand` and its view;
`with_mode`, `mode`, `answer_get_only` on `Reference`, `mode` on `ReferenceView`, `status` on
`ReferenceRegistration`. Variants: `Language::Uql`, `ErrorCode::EntityDeleted` and `Fenced`, `Command::Purge` and
`Migrate`, `CommitOperation::DeleteInstance` and `RetractInstance`, each new match arm `unimplemented!()`.
`ErrorCode::ALL` and `Command::ALL` are not extended yet: `ErrorCode::number` is the position in `ALL`, and a lookup
through `ALL` would reach an unimplemented arm for every code.

Exhaustive matches outside the three crates had to name the new variants for the repository to build: in
`ultragraphdb_common_frontend` (`utils/term_rules.rs`, `client_session/client_session_writes.rs`) and
`ultragraphdb_common_storage` (`utils/operation_of.rs`) a bare `unimplemented!()` arm for kinds 5 and 6, unreachable
while the wire refuses both kinds; in `ultragraphdb_client_core`'s test server and `ultragraphdb_tests`'
`conformance_steps.rs` the new commands and kinds join existing arms. The transport's `RequestKind::of` takes a bare
`unimplemented!()` arm for Purge and Migrate, and `ClientAuthentication` lists both beside every other unexpected
command.

## Existing tests brought to the new layout

PROTOCOL.md changed what they assert, so they assert the new rule and fail until phase 4 and the regenerated goldens:
the section counts of `with_section` and `split_sections` over Result (9 to 12), Prepare (7 to 10), Install (3 to 7)
and Describe (7 to 9) frames, in the unit tests and `frames_steps.rs`; language 3 no longer refused (Query, Prepare,
`Language`); an IRI in a pattern's reifier column now read; Drop's byte 72 now `catalog`; codes 30 and 31 and `Fenced`
retryable; commands 24 and 25 in the registry; entity types' preorder 0 in the Describe tests and in `bodies_steps.rs`'s
dictionary answer; `DESCRIBE_SCHEMA_ID` the specification's `0x5000B62149F4DB68` (finding 1 below).

## Scenarios, written first

Each verbatim from `specs/wire-protocol/spec.md`, checked by script against it; bound by each crate's `bdd` target,
`fail_on_skipped`. The wire's are `features/wire-protocol/ugs_frames.feature` (steps `tests/ugs_steps.rs`), the
transport's `features/wire-protocol/ugs_streams.feature` (steps `tests/bdd.rs`, a new `bdd` target with its Bazel
target and `[[test]]`).

| Scenario | Unit tests | Against the phase-1 code |
|---|---|---|
| A Describe past the client's frame continues | `describe_frames_tests::test_an_answer_past_max_frame_continues_and_concatenates` and four more | `DescribeEntry::new` unimplemented |
| An Install reply carries its warnings | `install_tests::stage_6::test_an_install_reply_with_a_warning_is_built_and_read_back`, `install_view_tests::stage_6::test_an_install_reply_reads_its_version_epoch_report_and_warning` | `Warning::new` unimplemented; the three-section reader refuses `SectionOffset` 56 for 24 |
| A request with a report is refused | `install_view_tests::stage_6::test_a_request_holding_any_reply_section_is_refused` | `SectionOffset` in place of `report` |
| A Drop of a predicate names no catalog version | `drop_command_view_tests::stage_6::test_a_drop_of_a_predicate_or_graph_names_no_catalog_version` | `ReservedNotZero` 200 in place of `catalog` |
| A handle IRI is taken in the reifier column | `result_view_tests::stage_6::test_a_handle_iri_is_taken_in_the_reifier_column` | the nine-section reader refuses `SectionOffset` 96 for 72 |
| A Describe of five frames takes a Credit (transport) | `outbound_tests::stage_6` (3), `command_tests::test_result_entries_describe_and_catalog_frames_spend_credit` | all five frames leave; `spends_credit` unimplemented |
| The golden frames match their generators | the bdd step: each golden's section count, then its rebuild | the golden `prepare` holds 7 sections, not 10 |
| Purge and Migrate decode and are refused by stage (transport) | `purge_view_tests::test_a_well_formed_purge_decodes`, `migrate_view_tests::test_a_well_formed_migrate_decodes`, the transport's Purge and Migrate test | `Purge::new` and `Command::number` unimplemented |
| Language 3 is read and built | `query_view_tests::stage_6::test_a_uql_query_is_built_and_read_back` | `Language::Uql.number` unimplemented |
| Language 4 is refused | `query_view_tests::stage_6::test_a_query_of_language_4_is_refused` | **passes**: the old code refused 3 and 4 alike |
| A retract operation is read and built | `operation_view_tests::stage_6::test_a_retract_operation_is_built_and_read_back` | the kind-6 arm of `OperationFields::of` unimplemented |
| A delete operation with a third arg is refused | `operation_view_tests::stage_6::test_a_delete_or_retract_takes_two_args` | `FieldValue` `kind` 5 in place of `args` |

No `ProtocolError` variant is new in this group: every refusal the protocol names exists. Each new code and command is
asserted by number; `BodyError`'s nine variants are step A's.

## The failing run

```
bazel test //ultragraphdb_core/ultragraphdb_core_body/... //ultragraphdb_protocol/ultragraphdb_protocol_wire/... \
    //ultragraphdb_protocol/ultragraphdb_protocol_transport/...
```

| Target | Run |
|---|---|
| `ultragraphdb_core_body_tests` | 27 passed |
| `ultragraphdb_protocol_wire_tests` | 811: 689 passed, 122 failed: 90 of the 91 new, and the 32 existing tests above |
| wire `bdd` | 62 scenarios: 51 passed, 11 failed: 9 of the 10 new, and "A Result holds its monotonic flag and column names" and "A hash-only term without its text is refused", whose steps now split twelve sections |
| `ultragraphdb_protocol_transport_tests` | 184: 180 passed, the 4 new failed |
| transport `bdd` | 2 scenarios, both failed |

The 122 failures by reason: 48 an `unimplemented!()` of the surface, 36 an assertion over the old layout or rule, 21 a
refusal or acceptance of the old code, 17 a golden frame of the old layout read with the new section count (the test's
`split_sections` takes a table entry past the old body and panics on the slice). No failure is a compile error, a
missing import, or a panic outside the code under test and its inputs. `bazel build //...` builds the repository.
The crates whose matches gained arms pass their suites unchanged: `bazel test` of `ultragraphdb_client_core`,
`ultragraphdb_common_frontend`, `ultragraphdb_common_storage` and `ultragraphdb_protocol_transport_compio`, integration
tests excluded, 7 targets passed. Clippy was not run: the repository runs it through Cargo alone (`make check`), over
the whole workspace.

## Phase 3: the inputs audited

The suite's inputs were read against the enumeration for values that coincide. Changed: Drop's tenant 3 beside kind
3, now tenant 6; Describe's tenant 0, the zero any reader returns, now 9 in the hand-laid answers, the tenant asserted;
`test_unused_bytes_past_staged_are_zero` and `test_an_error_on_a_describes_stream_spends_no_credit`, which passed
against the old code, now assert what stage 6.6 changes (the top byte of `staged`; the credit a Describe frame spends).
Kept, each beside a test where the values differ: an entry's name index equal to its place (reversed in the order
test); an Operation's instance at index 0 (graph 0 and instance 1 in another); a Purge's `removed` equal to its count
(0 and 2 beside it); a reifier cell in row 0 of column 0 (the golden 3 × 3 batch beside it).

## Findings

1. **The schema id's golden value is stale.** `PROTOCOL.md` §11.7 and ugs-catalog's "The schema id's golden value"
   give `0x5000B62149F4DB68` for the golden `describe_reply.frame`. XXH3-64 (Python's xxhash 3.6.0, libxxhash 0.8.2) of
   its nine-section declarations body gives that value only with the entity type's `extra` 1, the preorder number
   §4.6 retired; with `extra` 0 it is `0x5FC0D2C82332781C`. The test holds the specification's value and cannot pass
   until the specification is fixed.
2. **Write-set operations 8 and 9** are in the requirement "The frames follow PROTOCOL.md as extended for UGS" and not
   in task 1.2's list; no group names the wire for them (group 7 names operation 8). Not in this surface; adding the
   variants also changes `ultragraphdb_transactions_commit`'s replay.
3. **Datatypes 34 to 36** are checked by `ultragraphdb_core_terms`' `EncodedTerm::check`, which the Result view calls;
   `ultragraphdb_core_terms` is group 6's crate. Phase 4 either extends it or checks the three tags in the wire.
4. **The continuation receivers have no named bound.** `DescribeFrames` and `CatalogFrames` must stop at a bound (Ultra
   rule 2): §8.3 and §13.7 say `catalog_bytes_max` bounds the answer but name no refusal for a receiver past it.
5. **§8.3's per-frame rule leaves section 8 unnamed**: "a text only on a character boundary" covers section 1; a
   source's text in section 8 is checked whole, on the concatenated body. The tests take section 8 unconstrained per
   frame.
6. **Unnamed counts.** The `CountMismatch` of `report` for sections 3 to 6, and of `warnings`, `migration report`,
   `purge`, `migrate`, `message` and `entry bytes`, state no count and expected value; those tests assert the command
   and the field, and the count where the protocol fixes it.
7. **Owned elsewhere.** The frontend's refusal by stage of Purge and Migrate, its Describe continuation, and a
   client's or role's refusal past credit are groups 5, 11 and 4's to bind in their crates. The frontend's feature
   still holds "A Describe past the client's frame is refused", which the modified requirement replaces.

## Decisions needed

- The schema id's golden value: correct `PROTOCOL.md` §11.7 and ugs-catalog to `0x5FC0D2C82332781C`, or restore a
  preorder number in `extra`.
- `Describe::with_entity_types` still takes the retired preorder number; dropping it changes callers in
  `ultragraphdb_common_frontend` and `ultragraphdb_store_quads`.
- The bare `unimplemented!()` arms for Operation kinds 5 and 6 in the frontend and storage become reachable once phase
  4 decodes the kinds; each needs its owner's implementation, or a refusal, before then.
- Where datatypes 34 to 36 are checked (finding 3), and whether operations 8 and 9 join this group (finding 2).

## The coordinator's decisions on the findings (2026-11-10)

For phase 4, which reads this before it starts.

1. **Schema id.** The specification is corrected: `PROTOCOL.md` §11.7, ugs-catalog's scenario and task 1.3 give
   `0x5FC0D2C82332781C`, and `tests/utils/golden_bodies.rs`' `DESCRIBE_SCHEMA_ID` follows the specification. The
   constant changed with the specification, before phase 4, not to pass a test.
2. **Continuation is bounded on both sides.** `PROTOCOL.md` §8.3 now states that an answer's sections hold at most
   `catalog_bytes_max` bytes together: the frontend refuses a larger answer with code 6, and a client refuses a stream
   passing it as `FieldValue` (`"describe"`), reading no further frame; §13.7 refuses a Catalog stream whose section 0
   passes it as `FieldValue` (`"catalog"`). Phase 4 first writes the assemblers' tests for both refusals, observes them
   fail and records that here, then implements them. The frontend's own refusal is group 5's (task 5.4).
3. **A text split mid-character** is already settled: §8.3 splits a text only on a character boundary.
4. **`CountMismatch`'s count and expected** stay unasserted where the specification states none.
5. **Write-set operations 8 and 9** are group 7's, with their replay (task 7.7, group 7's crate list).
6. **`Describe::with_entity_types`' preorder argument** stays until task 4.5 removes the preorder numbering.
7. **Datatypes 34 to 36:** phase 4 extends `ultragraphdb_core_terms`' `EncodedTerm::check`, where every datatype's tag
   is checked, rather than checking three tags in the wire.
8. **Operation kinds 5 and 6 in the frontend and storage:** phase 4 replaces each bare `unimplemented!()` arm with a
   refusal, code 7, `detail` 0, the message naming the operation kind, so nothing under `src/` panics; group 7 decides
   the operations.
9. **Purge and Migrate refused by stage** are commands this group adds, so their stage refusal is this group's: phase 4
   implements it where the frontend refuses a later stage's command (`PROTOCOL.md` §11.2), moving the scenario to that
   crate's features if the refusal lives there. "A Describe of five frames takes a Credit" stays in transport if the
   credit is the transport's; otherwise likewise.
10. **The frontend's scenario "A Describe past the client's frame is refused"** stays green until group 5 replaces it
    (task 5.4).
11. **The wire's `tests/types/string_list`** stays where it is; moving it and removing the emptied module waits for the
    user's approval. *Approved by the user and done in phase 4:* `string_list_tests.rs` moved whole by `git mv` to
    `tests/utils/body_tests.rs`, the mirror of `src/utils/body.rs`, whose mapping its three tests exercise (each
    refusal naming command 12); `tests/types/string_list/mod.rs`, emptied, removed and unregistered from
    `tests/types/mod.rs`, `body_tests` registered in `tests/utils/mod.rs`. No test reads `StringList` apart from the
    wire's mapping, so nothing moved to `ultragraphdb_core_body`, whose 7 `StringList` tests stand. Three tests before
    and after. (The user's commit `1cf82d3b` reset the index during phase 4, so the move shows as a deletion and an
    untracked file until staged; git pairs them at commit.)
12. **`BodyError` in `ultragraphdb_core_body`** is right: an error one crate raises lives in that crate, and the
    diagnostic crate is separate because three crates raise it.

## Phase 4 (task 1.3)

**Implemented, per crate.** Every phase-1 body; no `unimplemented!()` is left under any `src/` this group touched.

| Crate | What |
|---|---|
| `ultragraphdb_protocol_wire` | `Command` 24 and 25 (`ALL` of 45, "both" commands), `spends_credit` (Result, Entries, Describe, Catalog); `ErrorCode` 30 and 31 (`ALL` of 31), 31 retryable and running again as 5; `Language::Uql`. Operation kinds 5 and 6: fields, kind 1 to 6, two args, decoded and built in Operation and ConditionalWrite. The warnings (§4.9): `Warning`, `WarningView`, `Warnings::read` in §4.9's order, messages interned on build; in Result (9–11, source 0), Prepare (7–9, refused for a pattern, and by `check_request` in a request) and Install (4–6, source taken). Install: `staged` at 24, seven sections, sections 2–6 empty in a request (`report`), the reports UTF-8, the migration report empty without `staged`. Prepare: ten sections. Result: twelve; `check_pattern` takes a reifier of tag `0x01` or `0x02`; `frame_size_with` counts the warnings, its overhead the 96-byte table and 7 bytes before each of the nine sections that can follow an end off a multiple of 8. Describe: nine sections; entries in 7 and 8, a retired name laid out with its current name's index, the strings the declarations', then the entries' names, then the current names (§4.7, §11.7); every record's `extra` 0 (§4.6), the preorder number kept on the owned answer alone (decision 6); the entries' Order of §8.3; `Describe::frames` and `Catalog::frames` through `utils/continuation.rs`, cutting u32 and record sections between units, section 1 and a source's text in section 8 on a character boundary (decision 3); `DescribeFrames` and `CatalogFrames`, a frame after `LAST` refused, bounded by `CATALOG_BYTES_MAX` (decision 2). Purge and Migrate by §8.5 and §8.6. Reference: `mode` at byte 1, `status` at byte 14, byte 15 zero, statuses and outcomes by mode (§13.15). Drop: `catalog` at 72, 0 for kinds 1 and 2 |
| `ultragraphdb_protocol_transport` | `RequestKind` takes Purge and Migrate as requests opening a stream; `Outbound` meters by `Command::spends_credit`, so Describe and Catalog frames spend credit |
| `ultragraphdb_core_body` | No stub was left; unchanged |
| `ultragraphdb_core_terms` (decision 7) | `EncodedTerm::check` takes tags `0x41` (dimension at least 1, 3 + 2d bytes), `0x50` (a unit code of 1 to 15 bytes of UTF-8) and `0x51` (1 to 12), and codes 34 to 36 in `0x30` and `0x31`; `KnownDatatype` is unchanged, the three datatypes being group 8's |
| `ultragraphdb_common_frontend` (decisions 8, 9) | `check_operation` refuses kinds 5 and 6 with code 7, `detail` 0, naming the kind, before any destination is asked; Purge refused naming stage 8 and Migrate naming stage 9.5, as Install is |
| `ultragraphdb_common_storage` (decision 8) | `operation_of` refuses kinds 5 and 6 with code 7, `detail` 0, naming the kind |

**Purge and the gate.** Migrate passes the gate with `declare`. Purge's action, 7 `purge`, is one `Action` does not name
(its test pins actions 1 to 6), and the gate fails closed on an action it does not know, answering 21 where §11.2 asks
7; Purge is therefore checked for its tenant and refused by stage without the authorizer. `Action` gaining `Purge` is
stage 8's.

**Scenario moved (decision 9).** "Purge and Migrate decode and are refused by stage" is the frontend's:
`features/wire-protocol/ugs_streams.feature` of `ultragraphdb_common_frontend`, steps in `tests/endpoint_steps.rs`
over a real endpoint; removed from the transport's feature with its steps. "A Describe of five frames takes a Credit"
stays in the transport, whose `Outbound` decides it.

**Decision 2's tests, written first.** `describe_frames_tests::test_an_answer_past_catalog_bytes_max_is_refused` and
`catalog_frames_tests::test_a_version_past_catalog_bytes_max_is_refused`: 16 frames of 1,000,000 bytes and one of
777,216 reach `CATALOG_BYTES_MAX` exactly and are taken; one byte more is refused as `FieldValue` naming `describe`
(command 12) or `catalog` (518). The failing run, against the assemblers without the bound:

```
bazel test //ultragraphdb_protocol/ultragraphdb_protocol_wire:ultragraphdb_protocol_wire_tests \
    --test_arg=past_catalog_bytes_max
test ...catalog_frames_tests::test_a_version_past_catalog_bytes_max_is_refused ... FAILED  (a refusal: none)
test ...describe_frames_tests::test_an_answer_past_catalog_bytes_max_is_refused ... FAILED  (a refusal: none)
test result: FAILED. 0 passed; 2 failed; 811 filtered out
```

and, with the bound, `2 passed`.

**Goldens, each by its generator** (`python3 -I <generator> <directory>`, Python's xxhash 3.6.0):

| Generator | Files | Change |
|---|---|---|
| `testdata/golden/bodies/generate_bodies.py` | `prepare`, `prepare_reply`, `prepare_text`, `prepare_text_reply` | ten sections, no warning |
| | `describe`, `describe_reply` | nine sections (the request's body 72 bytes); `Person`'s `extra` 0; no entry |
| | `result`, `result_more`, `result_query` | twelve sections, no warning |
| | `install`, `install_reply` | seven sections, `staged` 0 |
| `testdata/golden/roles/generate_roles.py` | `reference`, `reference_reply`, `reference_unavailable` | `mode` and `status` laid out; mode 0, so the bytes are as before |

The other 34 body frames and all 42 role frames regenerate byte for byte. `tests/utils/golden_bodies.rs`' `BODIES`
takes the 11 rows' size and checksums the generator printed. The generator prints the golden Describe's schema id
`0x5fc0d2c82332781c`, §11.7's value, which `test_the_schema_id_follows_the_declarations_alone` asserts.

**Conformance.** The Rust client's conformance suites (core, tokio, compio) pass under `unit-test`; `ultragraphdb_tests`'
conformance steps, the golden conversations against the built binaries, pass under `integration_test`.

**Tests the suite left asserting the protocol before stage 6.6.** Phase 2 brought most existing tests to the new layout;
these were missed, and each contradicts both `PROTOCOL.md` and a phase-2 test. None is edited (phase 4's rule); each
waits for a decision:

| Test | Asserts | `PROTOCOL.md` |
|---|---|---|
| wire `command_tests::test_exactly_the_registry_is_known`, `test_unassigned_numbers_are_unknown_commands`, `header_view_verify_tests::test_an_unknown_command_is_refused_by_number`; `bdd.rs` steps of "An unknown command is refused, not fatal" and "A retired number is refused" | 24 unknown; assigned `1..=23` | §3: 24 Purge, 25 Migrate |
| wire `command_tests::test_each_request_is_answered_by_section_3s_commands` | `ANSWERS` lacks Purge and Migrate | §3: each answered by itself |
| wire `error_reply_view_tests::test_an_unknown_code_is_refused` | code 30 refused | §9.4: 30 EntityDeleted |
| wire `operation_view_tests::test_command_header_fields_fit_the_kind`, `conditional_write_view_tests::test_an_operations_kind_is_1_to_4` | kind 5 refused | §7.3, §6.7: kinds 5 and 6 |
| wire `reference_view_tests::test_the_command_header_holds_the_reply_flag_alone`, `test_zero_bytes_are_named_at_their_body_offset` | byte 1 unused; byte 14 padding | §13.15: `mode` at 1, `status` at 14 |
| wire `install_view_tests::test_unused_bytes_come_before_the_fields` | byte 24 unused | §8.4: `staged` at 24 |
| wire `body_view_tests` `sections_of`/`used` (3 tests) | Prepare 7, Describe 7, Result 9, Install 3 sections; Install's bytes 24–31 and Drop's 72–79 unused | §8.1, §8.3, §9.1, §8.4, §6.5 |
| wire `result_batch_tests::test_a_reifier_is_a_blank_node_or_absent` (line 153), `test_the_size_with_a_row_is_the_frame_with_its_padding_at_most` (`section_padding`) | section 0 at 72; padding over nine sections from 72 | §9.1: twelve sections, section 0 at 96 |
| `ultragraphdb_core_terms` `encoded_term_tests::test_check_refuses_what_a_peer_may_not_send` | `0x30` code 34 refused | §10.4: codes 34 to 36 |

`test_a_reifier_is_a_blank_node_or_absent` also asserts `ResultBatch::push_row` refuses an IRI reifier, which the
builder still does to pass it, while §9.1 and §10.2 take a declared relation's handle IRI (`0x01`) there: the view takes
it, the builder does not, until that assertion is decided.

**The run.** `make build` passes. `make test`, cached: 21 doc tests pass; unit tests 78 of 80 targets pass, the two
failing being the stale tests above alone (`ultragraphdb_protocol_wire_tests` 813: 798 passed, 15 failed;
`ultragraphdb_core_terms_tests` 55: 54 passed, 1 failed), so `make test` stops there. Run beside it: BDD 37 of 38 targets
pass, the wire's `bdd` 62 scenarios, 60 passed and the 2 above failed; integration 7 of 7 pass. Counts:
`ultragraphdb_core_body_tests` 27; `ultragraphdb_protocol_transport_tests` 184 and its `bdd` 1 scenario; the frontend's
`bdd` 135 scenarios, the moved one among them; `ultragraphdb_common_storage_tests` 213. One run of every unit and BDD
target at once timed out `ultragraphdb_common_storage`'s `test_a_burst_of_answers_leaves_one_role_connection` at its
10 s frame deadline (1,000 DirectWrites, a path phase 4 did not change); the target alone, and `make test`'s own unit
run after it, passed all 213. Not yet traced: a deadline passed under load, or a race.

## Phase 3 corrections after phase 4

Each test the table above lists was read against the section of `PROTOCOL.md` it pins, found contradicting it, and
brought to that section's text; the source of every corrected expectation is the specification, never the
implementation. What the specification keeps, an unknown number refused, stays asserted at a number still unknown.

| Test | `PROTOCOL.md` | Before | After |
|---|---|---|---|
| `command_tests::test_exactly_the_registry_is_known` | §3 | known: 1–23, 512–531 | 1–25, 512–531 |
| `command_tests::test_unassigned_numbers_are_unknown_commands` | §3 | 24 unknown | 26 unknown |
| `command_tests::test_each_request_is_answered_by_section_3s_commands` | §3 | `ANSWERS` of 32, no Purge or Migrate | 34: Purge answered by Purge, Migrate by Migrate |
| `header_view_verify_tests::test_an_unknown_command_is_refused_by_number` | §3 | 24 refused | 26 refused; 24 Purge and 25 Migrate known |
| `bdd.rs`, "An unknown command is refused, not fatal" | §3 | command 24, "unknown command 24" | 26, "unknown command 26" |
| `bdd.rs`, "A retired number is refused" | §3 | every number outside 1–23 and 512–531 | outside 1–25 and 512–531 |
| `error_reply_view_tests::test_an_unknown_code_is_refused` | §9.4 | code 30 refused | 32 refused; 30 and 31 are codes |
| `operation_view_tests::test_command_header_fields_fit_the_kind` | §7.3 | kind 5 refused | kind 7 refused |
| `conditional_write_view_tests::test_an_operations_kind_is_1_to_4`, renamed `…_1_to_6` | §6.7, §7.3 | kinds 0 and 5 refused | 0 and 7 |
| `reference_view_tests::test_the_command_header_holds_the_reply_flag_alone`, renamed `test_the_reply_flag_is_boolean_and_the_unused_bytes_are_zero` | §13.15 | byte 1 unused | byte 2 unused; `mode` at 1 |
| `reference_view_tests::test_zero_bytes_are_named_at_their_body_offset`, renamed `test_the_zero_byte_is_named_at_its_body_offset` | §13.15 | registration bytes 14 and 15 zero | byte 15 alone; `status` at 14 |
| `install_view_tests::test_unused_bytes_come_before_the_fields` | §8.4 | byte 24 unused | byte 32; `staged` at 24 |
| `body_view_tests`' `sections_of` and `used`, read by `test_every_truncation_of_every_body_is_refused`, `test_each_unused_command_header_byte_must_be_zero` and `test_each_layout_rule_is_refused_naming_its_section` | §8.1, §8.3, §9.1, §8.4, §6.5 | Prepare 7, Describe 7, Result 9, Install 3 sections; Install's fields bytes 0–23, Drop's 0 and 8–71 | 10, 9, 12, 7 sections; Install's 0–31; Drop's 0 and 8–79, `catalog` at 72 |
| `result_batch_tests::test_a_reifier_is_a_blank_node_or_absent`, renamed `…_a_blank_node_an_iri_or_absent` | §9.1, §10.2 | an IRI in the reifier column refused; section 0 at 72 | an IRI taken, a literal still refused; section 0 at 96 |
| `result_batch_tests::test_the_size_with_a_row_is_the_frame_with_its_padding_at_most` (`section_padding`) | §4.1, §4.9, §9.1 | padding read over nine sections from 72; up to 7 bytes before each of the nine | read over twelve from 96; up to 7 bytes before each of the nine that can follow an end off a multiple of 8: all but section 0, at the table's end, and sections 4 and 10, after the texts' 8-byte pairs and the warnings' 24 bytes |
| `ultragraphdb_core_terms` `encoded_term_tests::test_check_refuses_what_a_peer_may_not_send` | §10.4 | `0x30` code 34 refused | 37 refused, one above codes 1 to 36; 34, 35 and 36 taken |

The padding test's bound stays 9 × 7 bytes, now derived from the sections that can be padded; before stage 6.6 it
counted 7 bytes before every one of nine sections, section 0's included, which never pads.

**The reifier column.** `ResultBatch::push_row` took a blank node alone in the reifier column, kept so in
phase 4 to pass the stale test, while §9.1, §4.6 and §10.2 and `SCHEMA.md` 7.3 and 11.1 name a declared relation's
edge-encoded instance by its handle IRI, tagged `0x01`. It now takes a blank node, an IRI or `ABSENT`, and refuses a
literal (`FieldValue` `"reifier"`), as the view's `check_pattern` reads it.

**Purge and the gate.** `Action` gains 7, `Purge` (§9.4's action table, `SECURITY.md`'s `purge` at tenant
scope); `action_tests::test_actions_keep_their_numbers` pins 1 to 7 and 8 unknown, where it pinned 1 to 6 and 7
unknown. The tenant gate asks the authorizer for `purge` in a tenant, as for `read`, `write`, `declare` and `drop`, so
the stage 5.5 gate allows every authenticated principal; and the frontend's Purge passes the gate (`Action::Purge`),
then the tenant check, before its stage refusal, as Migrate does (client-endpoint, "Every request naming a tenant
passes one gate"; §8.5, §11.10). Tests: `tenant_gate_tests::test_every_action_a_tenant_takes_passes_this_stage` takes
`Purge`; the frontend's step of "A principal the gate denies learns nothing of a number" sends a Purge naming tenant 0
and one naming tenant 9 through the denying authorizer, each refused with code 21, `detail` 7, alike.

**Failing first.** With `Action::Purge` declared and the gate still failing closed on it and `push_row` unchanged:
`ultragraphdb_protocol_wire_tests` 813, 812 passed, 1 failed (the IRI reifier refused, `FieldValue` `"reifier"` 1);
`ultragraphdb_common_frontend_tests` 92, 91 passed, 1 failed (the gate denying `purge`); the frontend's `bdd` 135
scenarios, 134 passed, 1 failed (the Purge answered `(Refused, 0)` for `(PermissionDenied, 7)`). Every other corrected test
passed against phase 4's code. After the change, all passed.

**The run.** `make build` passes. `make test` passes: doc tests 21 of 21 targets; unit 80 of 80 targets; BDD 38 of
38; integration 7 of 7. Counts: `ultragraphdb_protocol_wire_tests` 813, the wire's `bdd` 62 scenarios;
`ultragraphdb_core_body_tests` 27; `ultragraphdb_core_terms_tests` 55; `ultragraphdb_protocol_transport_tests` 184,
its `bdd` 1 scenario; `ultragraphdb_common_frontend_tests` 92, its `bdd` 135 scenarios;
`ultragraphdb_common_storage_tests` 213. The storage target ran in `make test`'s unit phase beside 35 others, the wire
having changed beneath it, and `test_a_burst_of_answers_leaves_one_role_connection` passed (the suite 66 s); its
earlier time-out did not recur and stays untraced.

## Phase 5 (task 1.4)

### Fuzzing

The targets were written and built (`cargo +nightly fuzz build --fuzz-dir fuzz/<crate>` for the wire, `ultragraphdb_core_body`
and `ultragraphdb_core_terms`), and each seed directory ran through its target once (`cargo +nightly fuzz run --fuzz-dir
fuzz/<crate> <target> <copy of the seeds> -- -runs=0`); the timed runs of `scripts/fuzz.sh` are task 13.2's.

**Targets.** `fuzz/ultragraphdb_protocol_wire`:

- `body` (any bytes through every command's view) did not build: its match over `Command` lacked Purge and Migrate. Both
  added; the promises of `PurgeView` (1 to 10,000 IRIs, `removed` at most their count and 0 in a request) and of
  `MigrateView` (a request holds no message, no report and no epoch; a message and a refusal come with state 9) asserted
  on every accepted frame. Prepare and Install join Write, Describe and Result among the bodies that rebuild to
  themselves rather than to the bytes read: their warnings' messages are interned by the sender's convention alone
  (§4.7), so a received list holding a message twice reads and rebuilds to other bytes.
- `body_fields` (structure-aware) builds the new bodies from arbitrary fields within each builder's rules: Describe with
  entries of kinds 1 to 20, retired names among them, in §8.3's order, also cut into continuation frames within an
  arbitrary `max_frame` of at least 1,024 and taken back by `DescribeFrames`; Catalog likewise through `CatalogFrames`;
  Purge, Migrate, a Prepare of a text in languages 1 to 3 with a reply's warnings, a query's first batch with column
  names, warnings and terms of tags `0x41`, `0x50` and `0x51`, Query in language 3, Install's reply with `staged`, its
  migration report and warnings, Drop's `catalog` for kinds 3 and 4, Operation kinds 5 and 6 in Operation and
  ConditionalWrite, and the reifier column holding IRIs. After the mutations, Install, Query and Describe are read, and
  Purge, Migrate and Catalog read back to the same frame.
- `continuation` (new): any bytes as the frames of one stream, each as long as its `size`, pushed into `DescribeFrames`
  and `CatalogFrames` until one refuses; an answer taken whole, cut again within the largest frame the stream held,
  each frame within it and the last alone `LAST`, assembles to itself.

`fuzz/ultragraphdb_core_body` (`sections`, `string_list`) and `fuzz/ultragraphdb_core_terms` (`encoded_term`) are
unchanged: they already reach `Sections::read`, `StringList::read` and `EncodedTerm::check`.

**Seeds.** `ultragraphdb_protocol_wire/testdata/seeds.py` (new, `python3 -I seeds.py`) lays out each new seed from
PROTOCOL.md's tables, with the golden generators' helpers, independently of the Rust code, into `fuzz_seeds/body/` and
`fuzz_seeds/body_fields/` (23 frames each) and `fuzz_seeds/continuation/` (2 streams). The 11 `body` seeds that were
copies of the goldens phase 4 regenerated were copied again from `testdata/golden/bodies/`. A scratch checker outside
the repository read every new frame through its view before the targets ran: each accepted, and each stream assembled
whole.

| Body | Target | Seeds |
|---|---|---|
| Describe, its entries and continuation | `body`, `body_fields`, `continuation` | `describe_entries` (a schema, a type, a source and a retired name, flagged `LAST`); `describe`, `describe_reply` refreshed; `continuation/describe_three_frames.bin` (the same answer cut into three frames between whole units) |
| Install | `body`, `body_fields` | `install_staged_reply` (`staged` 5, both reports, a warning of source 1); `install`, `install_reply` refreshed |
| Prepare | `body`, `body_fields` | `prepare_uql`, `prepare_uql_reply` (language 3, a column of type 6, a warning); the four `prepare*` refreshed |
| Result | `body`, `body_fields` | `result_query_terms` (tags `0x41`, `0x50`, `0x51` and `0x30` of code 35, column names, a warning); `result_reifier_iri` (a handle IRI in the reifier column); the three `result*` refreshed |
| Reference | `body` | `role_reference_get_only`, `role_reference_get_only_reply` (statuses 1 to 4), `role_reference_entity_deleted` (outcome 30, mode 0) |
| Drop | `body`, `body_fields` | `drop_tenant_data` (kind 3, `catalog` 7), `drop_whole_tenant_reply` (kind 4) |
| Query | `body`, `body_fields` | `query_uql` |
| Operation, ConditionalWrite kinds 5 and 6 | `body`, `body_fields` | `operation_delete`, `operation_retract`, `conditional_write_instances` |
| Purge | `body`, `body_fields` | `purge`, `purge_reply` |
| Migrate | `body`, `body_fields` | `migrate`, `migrate_checked_reply`, `migrate_failed_reply` |
| Catalog continuation | `body`, `body_fields`, `continuation` | `role_catalog_continues` (a first frame, not `LAST`); `continuation/catalog_three_frames.bin` |
| Error codes 30 and 31 | `body`, `body_fields` | `error_entity_deleted`, `error_fenced` |
| `Sections`, `StringList` | `sections`, `string_list` | step A's five |
| Tags `0x41`, `0x50`, `0x51`, codes 34 to 36 | `encoded_term` | `bfloat16vector`, `measure`, `precision`, `known_code_35`, laid out by hand from §10.1 and §10.5 |

**The runs.** `body` 110 seeds, `body_fields` 61, `continuation` 2, `sections` 3, `string_list` 2: no failure.
`encoded_term`, 122 seeds, first failed on each of the four new ones at its assertion "a Result's reader opens every
literal encoding the check accepts": `EncodedTerm::check` took tags `0x41`, `0x50` and `0x51` and codes 34 to 36 since
phase 4 (decision 7), while `EncodedTerm::result_view`, through which a client opens a Result's literal
(`ResultTermView::term`), refused them (`UnknownTag` 65, 80, 81; `UnknownDatatype` 35). The reader now opens them
(decision 1, below); all 122 seeds pass, and the four failing inputs libFuzzer wrote replay without a failure. After the
changes the decisions brought, every target was built again and each seed directory ran once more: no failure.

### Mutation testing

Each crate's runs, with the base `eb15d766` and the group's changes uncommitted on it, two lanes (`MUTANTS_SLOT=g1`
`MUTANTS_JOBS=4` beside the main lane's `MUTANTS_JOBS=6`):

| Crate | Run | Command | Mutants | Caught | Missed | Timed out | Unviable |
|---|---|---|---|---|---|---|---|
| `ultragraphdb_protocol_wire` | 1 | `scripts/mutants.sh ultragraphdb_protocol_wire --diff eb15d766` | 716 | 500 | 61 | 0 | 155 |
| | 2 | the same with `--re` of the 61 files and lines and all of `migrate_view_body_view.rs` | 135 | 128 | 3 | 0 | 4 |
| | 3 | the same with `--re '(utils/continuation\.rs\|result_batch/mod\.rs\|warnings/mod\.rs)'`, after decisions 2 to 4 | 195 | 184 | 0 | 0 | 11 |
| `ultragraphdb_protocol_transport` | 1 | `scripts/mutants.sh ultragraphdb_protocol_transport --diff eb15d766` | baseline failed | | | | |
| | 2 | the same | 4 | 2 | 0 | 0 | 2 |
| `ultragraphdb_core_body` | 1 | `scripts/mutants.sh ultragraphdb_core_body --diff eb15d766` (untracked, so its whole source) | 114 | 96 | 0 | 0 | 18 |
| `ultragraphdb_core_terms` | 1 | `scripts/mutants.sh ultragraphdb_core_terms --diff eb15d766` | 25 | 4 | 18 | 0 | 3 |
| | 2 | the same with `--re 'encoded_term/mod\.rs:(194\|197\|198\|316\|317\|320\|322):'` | 18 | 18 | 0 | 0 | 0 |
| | 3 | the same with `--re '(encoded_term_view\.rs\|term_view_literal\.rs\|tagged_datatype\.rs\|bfloat16_vector_view\.rs\|encoded_term/mod\.rs)'`, after decision 1 | 55 | 48 | 0 | 0 | 7 |
| `ultragraphdb_common_frontend` | 1 | `scripts/mutants.sh ultragraphdb_common_frontend --diff eb15d766` | 11 | 4 | 0 | 0 | 7 |
| `ultragraphdb_common_storage` | 1 | `scripts/mutants.sh ultragraphdb_common_storage --diff eb15d766` | 4 | 0 | 0 | 0 | 4 |
| `ultragraphdb_common_compute` | 1 | `scripts/mutants.sh ultragraphdb_common_compute --diff eb15d766`, after decision 1 | 3 | 2 | 0 | 0 | 1 |
| `ultragraphdb_store_hnsw` | 1 | `scripts/mutants.sh ultragraphdb_store_hnsw --diff eb15d766`, after decision 1 | 4 | 4 | 0 | 0 | 0 |
| `ultragraphdb_store_apply` | 1 | `scripts/mutants.sh ultragraphdb_store_apply --diff eb15d766`, after decision 1 | 4 | 4 | 0 | 0 | 0 |

The unviable mutants return `Default::default()` for types with none (`Operation`, `RequestStep`, `Answer`,
`BodyError`, `ChunksExact` and the like). The transport's first run failed its baseline: Cargo applies the workspace's
`missing_docs` to the harness-less `bdd` target phase 2 added, which reaches `tests/utils/peer.rs`'s undocumented
`CLUSTER` and `Peer` by path; Bazel's target builds without the workspace lints. Both now carry a doc comment, and the
second run is the crate's run over the diff.

**Survivors and their resolution.** Every test below was observed failing against the mutants it kills in the second
run, and passing on the code.

- `ultragraphdb_core_terms`, 18: tags `0x41`, `0x50` and `0x51` in `EncodedTerm::check` and `encoded_len` (the known
  gap). Killed by `encoded_term_tests::test_the_three_tags_of_section_10_4s_codes_34_to_36_take_their_tables_lengths`
  (§10.1's lengths: a bfloat16vector of dimension 258, bytes `02 01`, 3 + 2 × 258; a measure in `m/s`, 10 + 3; a
  precision, 2; dimension 3 and 0) and `test_check_refuses_a_measure_without_a_unit_and_a_precision_outside_1_to_12`
  (§10.1, §10.5: no unit, a unit not UTF-8, `m` and `µm`; precisions 0, 13, 1, 12).
- wire, `utils/continuation.rs`, 45: the cutting of continuation frames. Killed by
  `catalog_frames_tests::test_each_frame_but_the_last_holds_what_fits` (each frame holds what fits within `max_frame`,
  from §2.3 and §4.1 by hand: 1,736 bytes within 2,000, 16,088 within 16,384, 16,152 within 16,456; the three sizes
  chosen with a brute force of §2.3 so that every `data_room` mutant differs at one of them),
  `test_a_header_flagged_last_flags_the_last_frame_alone`, and `describe_frames_tests::
  test_a_sources_text_is_cut_on_a_character_boundary_and_fills_each_frame` (a source of 3,000 three-byte characters
  within 4,096: 3,735, 3,768, 1,497 bytes), `test_a_cut_outside_a_text_falls_where_the_room_ends` (retired names of
  bytes `0xBF` after a source: 3,520 and 220) and `test_a_max_frame_holding_no_part_is_refused` (`FrameTooLarge` naming
  a size above the limit). Three are equivalent, entered in `.cargo/mutants.toml` with their argument and confirmed by
  its `comm` check, which lists exactly them and the four entries before: `Cut::end`'s `r.start < limit` and
  `limit < r.end` versus `<=` (at `limit == r.start` the cut returns `limit` at once; at `limit == r.end` the byte
  there never continues a character in an answer in §8.3's order), and `data_room`'s `CHUNK_SIZE + LEAF_SIZE` versus
  `-` (measured: the same room for every `max_frame` from 0 to `MAX_FRAME_SIZE`).
- wire, `describe_body_builder.rs:68` (`texts` taken for kinds other than the source): killed by the character-boundary
  test above.
- wire, `describe_view_body_view.rs:177` (the `NotUtf8` offset of a source after another entry): killed by
  `describe_view_tests::test_a_sources_invalid_byte_is_named_at_its_section_offset` (§4.1: 4 + 2).
- wire, `purge_view/mod.rs:43` (`iri_count` → 1): `purge_view_tests::test_every_iri_is_read_in_order`.
- wire, `reference_registration.rs`, 5 (the fields `accepted`, `answered` and `refused` set):
  `reference_registration_tests::test_each_answer_sets_status_outcome_generation_and_close` (§13.15).
- wire, `reference_view_body_view.rs:163` (a live entity's `found && member`):
  `reference_view_tests::stage_6::test_a_live_entity_names_its_id_and_its_members_generation`.
- wire, `result_batch/mod.rs`, 4 (`with_warnings`, `warnings`, the warnings counted by `frame_size_with`) and
  `warnings/mod.rs:82` (`is_empty`): `result_batch_tests::test_a_batch_with_warnings_counts_them_and_reads_them_back`
  (the size with a row the frame's with its padding at most, as the test before it derives, and the warnings read
  back).
- wire, `warnings/mod.rs:99` (`holds_sections`): `prepare_view_tests::stage_6::test_a_request_holding_messages_alone_is_refused`
  (§8.1: sections 7 to 9 empty in a request).

After the second runs three survivors were left, in `continuation_frames`' `least`, the size `FrameTooLarge` named when
a `max_frame` carried no part of an answer (`HEADER_SIZE + 16 * sections.len() + 16`, `+` → `*` at column 37, `+` → `*`
and `-` at column 59), a bound nothing stated. Decision 2 below defines it; the third runs leave no survivor.

### Found while writing the targets

- **A Migrate request naming an epoch.** §8.6's command-header table gives `epoch` 0 in a request, but its Order named
  only `epoch` at most 2⁴² − 1, and `MigrateView` took a request (state 0) of any epoch, which `to_body` dropped: the
  `body` target's one-encoding assertion fails on such a frame. PROTOCOL.md §8.6's Order now reads "`epoch`, 0 in a
  request and at most 2⁴² − 1 in a reply"; `migrate_view_tests::test_a_request_names_no_epoch` (epochs 1 and 2⁴² − 1
  refused as `FieldValue` `"epoch"`) was observed failing ("a refusal: none"), then `MigrateView` refuses it; the second
  run caught both mutants of the new check.
- `Warnings::NONE` was never used, so every build of the wire warned `dead_code`; and `ResultBatch::frame_size_with`
  counted no column names, so a query's first batch with names could pass the size it promised. Decisions 3 and 4.

### The coordinator's decisions on what phase 5 left open (2026-11-10)

1. **The Result reader of datatypes 34 to 36.** `EncodedTerm::result_view`, and the store's `view` through the same
   reader, opens tag `0x41` as `TermView::Bfloat16Vector` (a `Bfloat16VectorView` over the little-endian bfloat16s,
   each element the binary32 whose high 16 bits it is), `0x50` as `TermView::Measure` (the big-endian binary64 and the
   unit's inline code), `0x51` as `TermView::Precision` (its byte), and codes 34 to 36 in tags `0x30` and `0x31` as
   `TermView::Typed` of `Datatype::Tagged`, the new `TaggedDatatype` (`from_code`, `code`, `iri`), which the check now
   also reads its codes from. `KnownDatatype` is unchanged. `TermView::to_literal` maps a lexical form of the three to a
   literal of the datatype's IRI and refuses a value held by value as `UnknownDatatype`, naming its code: writing their
   lexical forms is the client's mapping, group 11's. The tests, from §10.1, §10.4 and §10.5's layouts, were written
   first and observed failing (`ultragraphdb_core_terms_tests`, 63: 59 passed, 4 failed: "Unknown term tag 0x41",
   "0x50", "Unknown datatype code 0x23"): `bfloat16_vector_view_tests::test_a_view_reads_the_elements_in_place` (1,
   -2, 0.15625 from `80 3F`, `00 C0`, `20 3E`), `encoded_term_view_tests::test_a_measure_and_a_precision_are_read_by_value`,
   `test_codes_34_to_36_are_read_as_typed_literals`, `term_view_literal_tests::
   test_a_value_of_codes_34_to_36_by_value_names_its_code`; beside them `tagged_datatype_tests` and
   `test_a_lexical_form_of_codes_34_to_36_is_a_literal_of_its_datatype`. The exhaustive matches over `TermView` and
   `Datatype` in `ultragraphdb_common_compute` (`row_terms.rs`, `names.rs`), `ultragraphdb_store_hnsw` and
   `ultragraphdb_store_apply` take the new variants as their vector-less, text-less neighbours. `.cargo/mutants.toml`
   gains `Bfloat16VectorView::is_empty` versus `false` (no reachable view is empty: `encoded_len` refuses dimension
   0); its entry for `encoded_len`'s `|` versus `^` already covered the bfloat16vector's dimension, by the same
   argument. The `comm` check lists exactly the entries for both crates.
2. **`FrameTooLarge`'s size.** Its `size` field's doc in `ultragraphdb_error_protocol`, and the `# Errors` of
   `Describe::frames` and `Catalog::frames`, define it for a continuation that holds no unit as the least frame that
   holds one (§2.3, §4.1): the frame header and the command header, 256 bytes; the section table, 8 a section; the unit
   at its section's start (a record, a u32, a character of a text, a byte of anything else), padded to 8 for the empty
   sections after it unless its section is the last; the least over the sections with bytes left. For a frame of that
   size to hold the unit, `next_parts` lets each section but the last end by the room rounded down to 8, so the padding
   after it fits, where it kept 8 bytes for every later section. Tests first, observed failing against the old bound
   (`FrameTooLarge { size: 288, limit: 264 }` for 265; `416` for 329):
   `catalog_frames_tests::test_a_max_frame_below_the_least_frame_names_it` (256 + 8 + 1 = 265 refused at 264, three
   frames of 265 at 265) and `describe_frames_tests::test_a_max_frame_below_the_least_frame_names_it` (a source's first
   byte in the last section, 256 + 72 + 1 = 329; a name's end padded, 256 + 72 + 8 = 336; a 3-byte character,
   256 + 72 + 3 = 331; a type's bytes, one byte, 329); and `test_a_section_before_the_last_is_cut_where_the_room_ends`
   (300 types within 4,096: the names' 1,200 and 1,200 bytes and 85 entries, 1,360, then 3,440). The frames the earlier
   tests pin are the same.
3. **`Warnings::NONE`** has no natural use, a view's value no builder writes, and is removed: the build has no warning.
4. **Column names in `frame_size_with`**: each name's end and bytes, sections 7 and 8 (§9.1, §4.3).
   `result_batch_tests::test_the_size_with_a_row_counts_the_column_names` (names `n` and `label`) was observed failing,
   541 for 555, the 14 bytes of two ends and six bytes of text, then passed.

### The run

After the decisions: `make build` passes, no warning. `make test` passes, cached where nothing changed: doc tests 21 of
21 targets; unit 80 of 80; BDD 38 of 38; integration 7 of 7. Counts: `ultragraphdb_protocol_wire_tests` 829 (813 and the
16 tests above), the wire's `bdd` 62 scenarios; `ultragraphdb_core_body_tests` 27; `ultragraphdb_core_terms_tests` 63
(55 and 8); `ultragraphdb_protocol_transport_tests` 184, its `bdd` 1 scenario; `ultragraphdb_common_frontend_tests` 92,
its `bdd` 135 scenarios; `ultragraphdb_common_storage_tests` 213; `ultragraphdb_common_compute_tests` 155.
