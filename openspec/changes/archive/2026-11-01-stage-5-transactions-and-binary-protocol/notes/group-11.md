# Group 11: the transaction-verification scenarios (11.1) and fuzzing at the stage's close (11.7)

The fuzzer audit covers the formats that stay: the posting format, the frame header and its validation, the chunk
table, the superblock and the WAL scan (the user's scope of 2026-11-01). The bodies' views, the round and the
write-set record change with the distribution restructure, so their audit is deferred (below). The full
`scripts/fuzz.sh` run covers every target.

## 11.1 The scenarios

| Scenario | Bound to | Run |
|---|---|---|
| The model builds without gaps | `scripts/lean.sh`: `lake build` in `lean/`, failing on Lean's "declaration uses 'sorry'" warning and on any `sorry` token in the sources | passed, below |
| A fuzz target finds a planted defect | the fuzzer audit | the posting decoder without its length bound found in 3.0 s, below |
| A fuzzer finding becomes a regression test | Gherkin, verbatim, in `ultragraphdb_transactions_log/features/transaction-verification/transaction_verification.feature`, replaying `testdata/fuzz_regressions/scan/` through `ultragraphdb_tool_bdd::fuzz_regressions` as the target `scan` replays it | `bazel test //ultragraphdb_transactions/ultragraphdb_transactions_log:bdd`: 14 scenarios, 14 passed |
| A time-boxed run reports per target | `scripts/fuzz.sh`, every crate, 60 s per target | below |

`scripts/lean.sh` (Lean 4.14.0, the pin in `lean/lean-toolchain`):

- In the tree: `lake build` passed from Lake's cache in 2.6 s; no `sorry`.
- In a copy of `lean/` without `.lake/`: all 8 modules of `RoundModel` built in 7.2 s; no `sorry`.
- The check itself, in that copy: a theorem `planted_gap : (1 : Nat) = 2 := by sorry` appended to `Txn.lean` fails the
  script (exit 1) by both rules, the warning and the token, and so does the rerun from Lake's cache, which replays the
  warning; with the theorem removed the script passes again.

The bdd target declares `glob(["testdata/fuzz_regressions/**"])` beside its feature files: the cases exist.

## 11.7 The full runs

`CARGO_BUILD_JOBS=4 scripts/fuzz.sh`, nightly `cargo 1.100.0-nightly (2026-09-16)`, `cargo-fuzz` 0.13.2, 60 s per
target, 20 targets in 8 packages, one fuzzing process at a time.

**Run 1** (12:26 to 12:37) ended inside `body_fields` with exit 1 and no report: a target that fails on a seed stops
before libFuzzer prints a coverage line, and `grep` finding none failed the script under `pipefail`. Fixed in
`scripts/fuzz.sh` (the two figures default to `?`). Before it ended, `encoded_term` failed (finding 1) and
`body_fields` failed on a seed (finding 2).

**Run 2** (12:46 to 13:10, 1,442 s), with findings 1 and 2 fixed and the strengthened `frame` target: three targets
failed, `encoded_term` (finding 3), `body_fields` and `term_table` (finding 4); the other 17 ran 60 s without a failure.

**Run 3**, the closing run, after findings 3 and 4 were fixed (13:10 to 13:31, 1,246 s, exit 0): every target ran its 60 s with no failure.

| Crate | Target | Executions | Coverage | Outcome |
|---|---|---|---|---|
| `ultragraphdb_client_core` | `reply` | 83,049 | 2451 | no failure |
| `ultragraphdb_core_codec` | `posting_list_decode` | 5,617,885 | 740 | no failure |
| `ultragraphdb_core_terms` | `encoded_term` | 5,412,287 | 714 | no failure |
| `ultragraphdb_protocol_scram` | `scram_client` | 39,789 | 740 | no failure |
| `ultragraphdb_protocol_scram` | `scram_server` | 4,069,860 | 1607 | no failure |
| `ultragraphdb_protocol_transport` | `framing` | 357,079 | 640 | no failure |
| `ultragraphdb_protocol_transport` | `server_connection` | 491,858 | 2259 | no failure |
| `ultragraphdb_protocol_wire` | `body` | 1,862,284 | 3744 | no failure |
| `ultragraphdb_protocol_wire` | `body_fields` | 548,963 | 5606 | no failure |
| `ultragraphdb_protocol_wire` | `chunk_table` | 2,647,002 | 340 | no failure |
| `ultragraphdb_protocol_wire` | `frame` | 2,004,098 | 463 | no failure |
| `ultragraphdb_protocol_wire` | `frame_fields` | 1,415,687 | 667 | no failure |
| `ultragraphdb_protocol_wire` | `quad_pattern` | 2,174,465 | 1156 | no failure |
| `ultragraphdb_protocol_wire` | `round` | 1,884,423 | 908 | no failure |
| `ultragraphdb_protocol_wire` | `term_table` | 351,630 | 1496 | no failure |
| `ultragraphdb_protocol_wire` | `write_set_record` | 3,079,410 | 581 | no failure |
| `ultragraphdb_transactions_commit` | `session_entry` | 947,103 | 126 | no failure |
| `ultragraphdb_transactions_log` | `scan` | 1,404,103 | 484 | no failure |
| `ultragraphdb_transactions_log` | `superblock` | 2,002,617 | 82 | no failure |

## Findings

| # | Target | Input | Defect | Fix |
|---|---|---|---|---|
| 1 | `encoded_term` | 25 bytes, `testdata/fuzz_regressions/encoded_term/duration_parts_of_opposite_signs` (tmin: already minimal) | `EncodedTerm::check` accepted a duration (tag `0x1D`) of positive months and negative seconds; `TermView::to_literal` panicked, `oxsdatatypes`' `Display` returning `fmt::Error` for it. A server's Result term reaching a client's `to_literal` | `check` refuses a duration whose months and seconds differ in sign (`Malformed`, "a duration whose months and seconds differ in sign"); PROTOCOL.md §10.1 and §10.5 state the rule: a duration is negative as a whole, so parts of opposite signs are no whole encoding |
| 2 | `body_fields` | a seed: a Result of six columns and no rows | the target asserted that a batch built with statement IDs reads back with them; the statement IDs are empty or 8 bytes per row (PROTOCOL.md §9.1), so a batch of no rows carries none and reads back without them. The decoder is right | the target expects the batch without statement IDs when it has no rows; `result_view_tests::test_a_batch_of_no_rows_asked_for_statement_ids_reads_back_without_them` holds the rule. No bytes committed: the input is the structure-aware target's own encoding. Decided 2026-11-01: the protocol stays as §9.1 states it; a flag marking statement IDs as asked for was not taken, since a batch of no rows has none to lose |
| 3 | `encoded_term` | 19 bytes, `testdata/fuzz_regressions/encoded_term/timezone_offset_of_i16_min` (tmin: already minimal) | `check` accepted a gDay (tag `0x1B`) with timezone offset `0x8000`, i16::MIN minutes; `to_literal` panicked negating it (`attempt to negate with overflow` in `oxsdatatypes`' `TimezoneOffset` `Display`) | `check` refuses an offset beyond 14 hours, XML Schema's −840 to 840 minutes (0xFFFF, −1, still marks none), for tags `0x15` to `0x1C`; PROTOCOL.md §10.1 and §10.5 state the range |
| 4 | `body_fields`, `term_table` | structure-aware inputs: a literal of datatype `Some("")` | `TermTable::intern` took a literal of an empty datatype, which the table writes as `xsd:string`'s empty second text, so it read back as the literal without a datatype: two owned terms of one encoding | `intern` refuses an empty datatype with `BadTerm`, as it refuses a stated `xsd:string`; `term_table_tests::test_literals_take_one_form_each` holds it. The `term_table` artifact no longer fails once rebuilt. No bytes committed, as for 2 |

Each regression case of `encoded_term` is replayed under Bazel by
`encoded_term_tests::test_the_encoded_term_fuzz_regressions_replay_without_a_panic`, through the target's steps, with
each case's refusal asserted exactly; `test_check_refuses_a_duration_whose_parts_differ_in_sign` and
`test_check_refuses_a_timezone_offset_beyond_fourteen_hours` hold the two rules at their bounds. Both were red before
their fix. After both fixes `encoded_term` ran 240 s (5,117,053 executions) in a copy without a failure.

## The audit of the fuzzers

Each defect was planted in a copy of the repository under the agent's scratch directory, never in the tree, and the
target run with `cargo +nightly fuzz run` from an empty corpus plus the crate's seeds for the script's budget, 60 s.
The time is the wall time of that run, cargo's check of the build and libFuzzer's start included (about 2 to 3 s); the
executions are libFuzzer's count at the failure.

| Target | Defect planted | Kind | Found | Time | Executions |
|---|---|---|---|---|---|
| `posting_list_decode` | `Reader::bytes` slices `len` bytes unchecked: the bound on a postings length behind a valid checksum (the spec's scenario) | unchecked length, panic | yes | 3.0 s | 7 |
| `posting_list_decode` | the trailing-bytes check `r.at + 1 < body.len()`, one byte after the splits accepted | off by one, round trip | yes | 3.4 s | 771 |
| `posting_list_decode` | the flags byte unchecked behind the checksum | missing bound, round trip | yes | 7.3 s | 1,005 |
| `frame` | `validate_body` slices the body to the header's `size` without the length check | unchecked length, panic | yes | 4.8 s | 25 |
| `frame` | the reserved bytes read from 67, byte 66 unchecked | off by one, round trip | **no** (65 s, 592,962 executions) | | |
| `frame`, strengthened | the same | | yes | 9.7 s | 24,365 |
| `frame_fields` | the flags rule dropped | missing bound | yes | 4.9 s | 18,186 |
| `frame_fields` | `size < HEADER_SIZE - 1`: a size of 255 passes the floor | off by one | yes | 6.7 s | 19,615 |
| `chunk_table` | `ChunkLayout::of_body` takes a length whose chunk count does not add up | missing bound | yes | 6.0 s | 16 |
| `chunk_table` | `ChunkedBody::verify` checks chunks `0..chunks - 1`, the last one unchecked behind the table's checksum | off by one, missing bound behind the checksum | yes | 46.2 s | 204 |
| `superblock` | the length check dropped and the fields sliced unchecked | unchecked length, panic | yes | 5.4 s | 1 |
| `superblock` | the checksum over `..CHECKSUMMED - 1`, the last checksummed byte unprotected | off by one behind the checksum | **no** (64 s, 832,746 executions) | | |
| `superblock`, strengthened | the same | | yes | 6.0 s | 2 |
| `scan` | `check_body` skipped: a round whose body checksums fail accepted | missing check behind the checksum | yes | 7.2 s | 3 |
| `scan` | the frame's `size` unchecked against the segment and the frame sliced by it | unchecked length, panic | yes | 6.5 s | 36 |
| `scan` | the chain's parent unchecked | missing bound behind the checksum | **no** (65 s, 391,054 executions) | | |
| `scan`, strengthened | the same | | yes | 6.0 s | 404 |

Three misses, each behind a checksum the mutator could not satisfy; each target was strengthened and finds its defect:

- `frame`: each input is checked twice, as given and with its first eight bytes replaced by the checksum of the header
  after them, as `posting_list_decode` forges its checksum.
- `superblock`: a record of the input's first 56 bytes as its seven fields encodes to bytes that must decode to it,
  the structure-aware direction, which reaches the checksum without forging it.
- `scan`: the input is scanned again with each frame's header checksum resealed, frame after frame from the first
  block, reaching the cluster, incarnation, position and chain rules behind it.

The scan's audit ran on `scan.rs` as it stood before 13:20, when M3's fix made `frame_at` refuse a Round frame
claiming position 0 (PROTOCOL.md §12.1, positions gapless from 1); it was not redone for that change. The target's
oracle has since asserted that every accepted round's position is at least 1, which run 3 holds.

The strengthened `frame` ran clean in run 2 (475,700 executions), the strengthened `superblock` and `scan` in run 2
and run 3, and the strengthened `scan` 64 s (219,177 executions) in the audit copy, with no failure.

### Deferred by the scope of 2026-11-01

Planted before the scope narrowed, recorded as data and not acted on; the formats change with the distribution
restructure, and the misses are their targets' to answer then.

| Target | Defect | Found | Time |
|---|---|---|---|
| `body` | `Sections::read` takes one trailing byte | no (65 s) | |
| `body` | `StringList` ends unordered and sliced unchecked | no (65 s) | |
| `body_fields` | `StringList::get` starts each string at its own end | yes | 5.0 s |
| `body_fields` | `KeyValues`' key sliced unchecked | yes | 6.6 s |
| `term_table` | the depth bound one deeper | yes | 12.1 s |
| `term_table` | a term's text sliced unchecked | yes | 8.1 s |
| `quad_pattern` | a bound slot's term index unchecked | no (63 s) | |
| `quad_pattern` | the statement cache's parameter count unchecked | no (63 s) | |
| `round` | a record sliced unchecked | yes | 9.8 s |
| `round` | a committed decision's detail unchecked | yes | 13.1 s |
| `write_set_record` | a list delta's absent value accepted | no (65 s) | |
| `write_set_record` | `KeyValues`' key sliced unchecked | yes | 3.9 s |
| `session_entry` | a result's padding checked from byte 10 | yes | 6.5 s |
| `session_entry` | the result count unchecked, the results sliced by it | yes | 6.9 s |
| `encoded_term` | an inline length of 16 accepted | yes | 8.0 s |
| `encoded_term` | a boolean byte of 2 accepted | yes | 7.2 s |
| `framing` | the framer wants one byte less than the frame | yes | 4.8 s |
| `framing` | `finish` keeps the previous frame's size | yes | 5.0 s |
| `server_connection` | requests taken before authentication | yes | 47.8 s |
| `server_connection` | frames sent after the closing Error | no (65 s) | |
| `reply` | a refused frame leaves the client connected | yes | 3.2 s |
| `scram_client` | `attribute` slices past a one-character part | yes | 6.5 s |
| `scram_client` | a server nonce not extending the client's accepted | no (104 s) | |
| `scram_client` | an iteration count of 4,095 accepted | no (64 s) | |
| `scram_server` | `unescape` slices unchecked | yes | 3.8 s |
| `scram_server` | a client nonce not validated | yes | 5.8 s |
| `scram_server` | any 32-byte proof authenticates | no (63 s) | |

The body target strengthened as `frame` was (each input also read as the frame its header builds around the bytes
after it) ran clean in run 2 (503,787 executions); it was reverted under the scope, and is kept outside the tree for
whoever takes up the bodies' audit.

## 11.8 Mutation testing

Scoped on 2026-11-01 to the stage's code the distribution restructure keeps (`tasks.md`, 11.8); `scripts/mutants.sh`'s
cargo-mutants, approved as an exception to Bazel-only. The equivalences are in `.cargo/mutants.toml`, 37 entries; the
comm check over the fourteen crates below lists exactly 50 excluded mutants, each the one its entry argues for:
`core_terms` 3, `core_algo` 30, `protocol_transport` 4, `protocol_transport_compio` 2, `client_tokio` 1,
`transactions_log` 10. Every timeout examined is a mutant that loops or waits forever, counted as caught.

### The core crates (agent M1, stopped 2026-11-01 before its last crates)

`cargo mutants -p <crate> -j 4`; survivors verified with `--file` and the equivalence entries as `--config`. Load
average 100–200 throughout.

| Crate | Mutants | Caught | Missed before → after | Timeouts | Unviable |
|---|---|---|---|---|---|
| `ultragraphdb_core_mapping` | 201 | 117, then 41 more | 22 → 0 | 0 | 62 |
| `ultragraphdb_core_terms` | 238 | 196 | 4 → 0 (3 equivalences, 1 test) | 0 | 38 |
| `ultragraphdb_core_algo` | 422 | 319 | 63 → 0 (30 equivalences, tests and golden cases) | 37 | 3 |
| `ultragraphdb_core_codec` | 322 | 278 | 22 → unverified | 16 | 6 |

Tests added in `core_mapping` (the predicate, relation and type declarations, the changeset and the property-graph
view), `core_terms` (`encoded_term_cmp_tests.rs`; two duration cases at the sign check's boundary), `core_algo`
(`packed_tests.rs`, `uint64_heap_tests.rs`) and `core_codec` (codec, decoder, encoder and error tests). Seven golden
cases were added to `core_algo/testdata/golden/algo_setops.json` by the repository's generator (`tools/golden/algo.go`,
the Dgraph commit unchanged), six strategy cases and `lists/fourth-narrows`; no existing case changed. The thirty
`core_algo` equivalences rest on arguments and on a Go measurement: Dgraph's `IntersectWithLin` and
`IntersectWithJump` return the same list on 200,000 seeded pairs.

Stopped at the user's request while `core_codec`'s survivors were being worked: the codec tests added for its 22
survivors pass under Bazel, but no re-run confirmed which survivors they kill. `store_posting`, `store_quads` and
`store_hnsw` were not run. Both are deferred with the rest (backlog, item 8).

### The protocol and the client shells (agent M2)

`cargo mutants -p <crate> -j 4` (cargo-mutants 27.1.0), the after-runs with the equivalence entries as `--config`;
load about 200 throughout. Format: total: caught / missed / timeouts / unviable.

| Crate | Before | After |
|---|---|---|
| `ultragraphdb_error_protocol` | 29: 1 / 0 / 0 / 28 | unchanged |
| `ultragraphdb_protocol_transport_compio` | 30: 9 / 7 / 2 / 12 | 28 (+2 excluded): 14 / 0 / 2 / 12 |
| `ultragraphdb_protocol_scram` | 130: 97 / 8 / 2 / 23 | 130: 105 / 0 / 2 / 23 |
| `ultragraphdb_protocol_transport` | 368: 259 / 25 / 19 / 65 | 364 (+4 excluded): 280 / 0 / 19 / 65 |
| `ultragraphdb_protocol_wire`, the header, checksums, chunk tables, frame validation and their utilities | 167: 135 / 0 / 0 / 32 | unchanged |
| `ultragraphdb_client_compio` | 45: 21 / 2 / 2 / 20 | 45: 23 / 0 / 2 / 20 |
| `ultragraphdb_client_tokio` | 74: 26 / 6 / 6 / 36 | 73 (+1 excluded): 31 / 0 / 6 / 36 |

Every timeout is a mutant that loops or waits forever. Tests added in `transport_compio` (connection), `scram` (client
and server), `transport` (pooled buffer, received frame, simulated stream and network, against an independent
SplitMix64), and the shells (`client_requests_tests.rs`, `client_shell_tests.rs`); each confirmed by a re-run to kill
its mutants. Six equivalence entries with their arguments (the compio `fill` bounds ×2, `NetworkFaults::none`, the
anonymous mechanism's `Ok(vec![])`, the Commit arm's `open` guard at `server_connection/mod.rs:375`, the tokio
simulated stream's `poll_flush`); the comm check over the seven crates excludes exactly nine mutants.

Noted, unchanged: `NetworkFaults::with_delay` says "each by 1 to `max_delay` steps"; the code delivers a delayed message
after 1 to `max_delay` steps in total. The new delay test pins the behaviour.

Dropped by the scope cut, counts when stopped: `client_core` 37 of 414 (9 caught, 1 missed: `ClientConfig::name ->
""`, unresolved, 27 unviable); the whole of `protocol_wire` 327 of 1,795 (277 caught, 2 missed in body types, killed by
tests that stay).

### The log, its traits and its errors (agent M3)

`cargo mutants -p <crate> -j 4`, verified per survivor with `--re` and `--config` the equivalence entries, the comm check
against an empty `exclude_re`. Load average 100–190 throughout; every timeout examined is a real infinite loop.

| Crate | Mutants | Caught | Missed before → after | Timeouts | Unviable |
|---|---|---|---|---|---|
| `ultragraphdb_error_log` | 15 | 1 | 0 → 0 | 0 | 14 |
| `ultragraphdb_traits_log` | 27 | 26 | 0 → 0 | 0 | 1 |
| `ultragraphdb_transactions_log` | 584 | 486, then 29 more | 32 → 3 | 3 (`zeros.rs`) | 63 |

Twelve tests added in `transactions_log` (`wal_tests.rs`, `sim_storage_tests.rs` against an independent SplitMix64 checked
on its published seed-0 outputs, `scan_tests.rs`); ten equivalences recorded with their arguments (`AlignedBuf::zeroed`
×2, `Superblock::decode` `| with ^`, `SimFile::write_at`, `SimState::perform`, `encode_round` ×2, `frame_at` ×2, one
anchored by file, line and column, and `step` `> with >=`, equivalent once `frame_at` refuses position 0).
A survivor exposed a defect: the scan accepted a Round frame claiming position 0 as a fresh log's first round, though
`Position::ZERO` is the place before the first round (PROTOCOL.md §12.1: positions are gapless from 1). `frame_at` now
refuses it as header damage (approved 2026-11-01; two tests, seen failing first). The three survivors left: the
`OPNOTSUPP` mapping in `compio_file_durable_file.rs:67:34`, compiled on Linux only; `CompioFile::sync` and
`CompioStorage::sync_directory` replaced by `Ok(())`, a dropped fsync that no in-process test can observe (the kill
tests, backlog item 8, can).

Dropped by the scope cut (2026-11-01), counts when stopped: `transactions_commit` 614 of 751 run (472 caught, 40
missed, 11 timeouts, 91 unviable); `transactions_txn` 326 (263 caught, 8 missed, 55 unviable); `tool_test` 208 (188
missed by design: its code runs only inside other crates' suites); `tool_bench` not run. The tests already added to
`commit` and `txn` stay and pass under Bazel.

## 11.9 The write-ahead log's compio suite on Linux (2026-11-01)

Run by agent C in the OrbStack machine `ugdb-linux`: kernel 7.2.12-orbstack, aarch64, Ubuntu 26.04.1 LTS; Bazelisk 1.25.0
running Bazel 9.3.0 (`.bazelversion`); the repository mounted at its `/Users` path over virtiofs; Bazel's output base on
btrfs inside the machine, with `--symlink_prefix=/` and `--lockfile_mode=off` so nothing was written into the shared
tree; tests in `linux-sandbox` with `--nocache_test_results`, `TEST_TMPDIR` on btrfs.

| Target | Result |
|---|---|
| `ultragraphdb_transactions_log_tests` | 174 passed (173 on macOS; the extra one is Linux-only) |
| `ultragraphdb_transactions_log` `:bdd` | 14 scenarios, 28 steps |
| `ultragraphdb_traits_log_tests` | 9 passed |
| `ultragraphdb_error_log_tests` | 8 passed |

No test fails only on Linux.

| Linux path | Tests exercising it |
|---|---|
| `O_DIRECT` | `test_aligned_files_on_compio`, `test_the_log_on_compio_survives_a_reopen`, and the new Linux-only `types::compio_storage::compio_storage_tests::test_an_aligned_file_is_opened_with_o_direct_and_a_plain_one_without`, which reads the open file's flags from `/proc/self/fdinfo` |
| `fallocate` | `test_preallocate_reserves_the_blocks_on_linux`, `test_preallocate_and_truncate_on_compio`, `test_aligned_files_on_compio`, `test_the_log_on_compio_survives_a_reopen` |
| Directory syncs | `test_directories_and_names_on_compio`, `test_the_copies_on_compio`, `test_the_log_on_compio_survives_a_reopen` |

Defects planted in a copy of the tree, run in Linux:

| Defect | Result |
|---|---|
| `O_DIRECT` removed | Only the new test fails; before it, nothing told whether `O_DIRECT` was set |
| `fallocate` made to punch a hole | `test_preallocate_reserves_the_blocks_on_linux` and `test_preallocate_and_truncate_on_compio` fail |
| A directory sync made a no-op | Survives: a directory entry's durability cannot be observed without a crash; the syncs run, and the kill tests (backlog item 8) are what can tell |

## Files

- `scripts/lean.sh` (new): the Lean check.
- `scripts/fuzz.sh`: the report survives a target failing on a seed.
- `ultragraphdb_transactions/ultragraphdb_transactions_log/features/transaction-verification/transaction_verification.feature`
  (new), `tests/bdd.rs` (its two steps), `BUILD.bazel` (the bdd target's regression data).
- `ultragraphdb_core/ultragraphdb_core_terms/src/types/encoded_term/mod.rs`: findings 1 and 3;
  `tests/types/encoded_term/encoded_term_tests.rs`: three tests; `testdata/fuzz_regressions/encoded_term/` (new), two
  cases.
- `../../../../../specification/PROTOCOL.md` §10.1 and §10.5: the duration's signs and the timezone range.
- `ultragraphdb_protocol/ultragraphdb_protocol_wire/src/types/term_table/mod.rs`: finding 4;
  `tests/types/term_table/term_table_tests.rs` and `tests/types/result_view/result_view_tests.rs`: findings 4 and 2.
- `fuzz/ultragraphdb_protocol_wire/fuzz_targets/body_fields.rs` (finding 2), `frame.rs`,
  `fuzz/ultragraphdb_transactions_log/fuzz_targets/superblock.rs` and `scan.rs` (strengthened).
