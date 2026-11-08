# Group 12: final verification and documentation

Tasks 12.2, 12.3, 12.4 and 12.7 ran side by side over the tree at `39bf3977`, a line `git blame` gives to `08ff8f26` counting as unchanged. 12.1's mutants are in `notes/group-9.md`, `notes/group-9-frontend-mutants.md` and `notes/group-11.md`.

Four defects in `src/` were found and fixed: `metadata_snapshot_codec.rs:256` overflowed on a placement version of 2^64 − 1, a panic first found on 2026-11-03 and never fixed; `SessionEntry::decode` accepted a node text twice and wrote it back with one node fewer; two warnings named a refusal's text `message`, the field tracing gives the event's own text, so a JSON log lost the refusal's reason; and `ultragraphdb_common_role`'s crate documentation linked `compute_role`, a private function of the compute binary. Three fuzz targets asserted rules their decoders no longer hold and now assert the specification's. `make format` reformatted 16 test files and `make fix` reported 16 warnings in this wave's tests; both now leave the tree unchanged and report nothing.

Wave 3b closed what owner decisions 34 to 36 settled. The accept loops pause after a failed `accept`, and the two inbox drains take at most a bound a turn (`AcceptPace` and `InboxTurn` in `common_service`). A client reads a Result's literal of a datatype no code names by its datatype's hash (`EncodedTerm::result_view`), and a Scan's `after` of tag `0x01` naming no valid system ID is refused with Error 1. The documentation sweep gave the change's `src/` lines 1,065 `#[must_use]`, 83 `# Errors` sections, 260 new cast reasons, 270 cancellation notes and 33 queue bounds. The coverage passes wrote the tests wave 3 left unwritten in ten crates. Seven more defects were found and fixed: both Rust client shells failed on a timeout past an `Instant`, the tokio shell refusing to connect and the compio shell panicking; `CompioTlsStream`'s note misstated what a dropped write leaves; five doc links named a private item or nothing (`serve_http`'s `drain`, `BIND_RETRY`, `NAME_FAILURES_MAX`, `commit_step`'s `Sequencer` and `Input::Register`); and a concurrent edit's `shadow_unrelated` in `term_view_literal.rs` stopped clippy for every crate above `core_terms`. `make format` then changed nothing and `make fix` reported 4 warnings in wave 3b's tests, fixed in the tests; both now leave the tree unchanged and report nothing.

12.4 closes by the owner's decision of 2026-11-08, coverage being high and a further pass of diminishing return. It leaves untested about 150 added lines in `common_frontend` (the client session, the role link and the role double) reachable in process, 14 in `common_compute`, and in `tool_test` 97 reports of a broken rule, which would need a damage hook in `src/`, and 35 fault and seed arms not traced. Every other added line is covered or listed as unreachable by construction, an OS failure, a cost past a test's or a binary's line run only as a process.

What stays open, each with its reason below. Under 12.3: the owner's decisions on the 17 closed ranges (`..=`) of the first half and those of the second, `Copy` types above 16 bytes, public fields (`ReportAsked`, `Registered`, `ChangesetWorkload`'s counters), `Server`'s function pointer and the atomics' orderings; the planes of rule 3; `serve_listener`'s accept loop and its tasks; `PartitionCore`'s `closes` and `remote` drains; the commit queue's production bound, storage's lease wants and the partition mesh, none of them capped; and three defects found by reading and not fixed: `ClientCore`'s `pending` entries of dropped request futures, `SessionEntry::encode` dropping a term table's entries that are no IRI, and `Pipeline::sync`, which is not safe to drop. `run_read.rs:55`, a refusal whose frame may pass 1 MiB, is a possible defect. Nothing stays open under 12.2.

## 12.2 Fuzzing

The change added 21 targets and changed 11, in 20 crates; `scripts/fuzz.sh` ran all 32, each at its default 60 s on macOS arm64, two agents side by side. The task names the added targets; the changed ones ran too, by the lead's decision, since they fuzz decoders the change rewrote. Five failed on their first run: two decoders were wrong and are fixed (`snapshot_decode`, `session_entry`), and three targets asserted what their decoders no longer promise (`write_set_record`, `encoded_term`, `hnsw_lists_decode`) and now assert the specification's rule. Each of the five passed a second 60 s run. Five crates hold regression cases, and each case replays by name under Bazel. Wave 3b answered the first half's two specification questions (owner decision 36, *Wave 3b: the specification's two questions* below): a client reads a Result's typed literal through `EncodedTerm::result_view`, and a Scan's `after` of tag `0x01` naming no valid system ID is refused with Error 1; the two targets reaching the changed readers then ran 60 s clean.

### First half: the wire, transport, client, terms, HTTP, telemetry, TOML and config targets

Two targets failed on their first run, each on an assertion the target held from before this change and the decoder
no longer promises; both decoders are right by `PROTOCOL.md`, both targets now hold the rule the specification gives,
and both found inputs are committed and replayed under Cargo and Bazel. Every target then ran its 60 s clean.

Added: `object_records`, `resume_key`, `address`, `address_parse`, `http_reply`,
`http_request`, `http_response_decode`, `document`, `config_record`. Changed: `body`, `body_fields`, `quad_pattern`,
`round`, `write_set_record`, `server_connection`, `reply`, `encoded_term`.

Each ran as `scripts/fuzz.sh <crate> <target>` at the default 60 s on macOS (aarch64), beside the other fuzz agent's
runs. Runs are libFuzzer's `number_of_executed_units`. Coverage, features and the corpus are read afterwards by
replaying the target's corpus directory with `-runs=0`: `cov` and `ft` are libFuzzer's, `corp` the inputs it keeps of
those on disk, `files` the inputs on disk.

**`ultragraphdb_protocol_wire`**

- `object_records` (added): 2,795,699 runs; cov 97, ft 399; corp 74/23 KB of 189 files; no failure.
- `resume_key` (added): 2,876,699 runs; cov 912, ft 1,189; corp 86/18 KB of 94 files; no failure.
- `body`: 2,616,269 runs; cov 4,385, ft 7,590; corp 92/54 KB of 839 files; no failure.
- `body_fields`: 260,215 runs; cov 8,276, ft 29,896; corp 2,018/1,018 KB of 10,242 files; no failure.
- `quad_pattern`: 1,207,135 runs; cov 1,212, ft 1,746; corp 175/50 KB of 485 files; no failure.
- `round`: 1,511,346 runs; cov 1,459, ft 2,751; corp 200/847 KB of 375 files; no failure.
- `write_set_record`: failed after 839,387 runs. After the fix: 3,294,057 runs; cov 1,302, ft 1,966; corp 162/296 KB
  of 310 files; no failure.
  The input: a kind-2 record of 160 bytes, its fixed part and every section empty, which `cargo fuzz tmin` cannot
  shorten. `WriteSetRecordView` accepts it, as `PROTOCOL.md` §12.2 says: a registration record holds one registration
  or none, for an entity the home got or created for a registration naming no bucket group. The view took that rule in
  tasks 7.1–7.3 and the target still asserted exactly one. `fuzz_targets/write_set_record.rs:31` now asserts at most
  one. The case is `testdata/fuzz_regressions/write_set_record/registration_of_no_registration`, replayed by
  `test_the_write_set_record_fuzz_regressions_replay` in `tests/types/write_set_record_view/write_set_record_view_tests.rs`:
  every case the view accepts encodes to its own bytes and counts the registrations its section 4 holds, and this one is
  a registration with none.

**`ultragraphdb_protocol_transport`**

- `address` (added): 3,726,985 runs; cov 271, ft 820; corp 206/7.6 KB of 482 files; no failure.
- `address_parse` (added): 3,663,267 runs; cov 495, ft 1,653; corp 306/19 KB of 743 files; no failure.
- `server_connection`: 666,493 runs; cov 2,302, ft 5,277; corp 63/150 KB of 203 files; no failure.

**`ultragraphdb_client_core`**

- `reply`: 172,212 runs; cov 1,254, ft 1,254; corp 2/509 B of 37 files; no failure.

**`ultragraphdb_core_terms`**

- `encoded_term`: failed after 6,597 runs. After the fix: 15,004,891 runs; cov 728, ft 1,095; corp 270/4.4 KB of 435
  files; no failure.
  The input: tag `0x01` and 16 bytes whose reserved bits are not zero, 17 bytes, which `cargo fuzz tmin` cannot
  shorten. `EncodedTerm::check` accepts it, since §10.1's whole encoding puts a hash there in a Result and holds no
  rule on it; `EncodedTerm::view`, the store's reader, takes the 16 bytes as a system ID and refuses them with
  `Malformed`, "an ID that is no valid system ID". The target, written when a term's ID was any 8 bytes, expected the
  reader to open every encoding the check accepts; group 2's 128-bit IDs made the reader validate, and the target was
  not run since. `fuzz_targets/encoded_term.rs:29` now takes that refusal, and only that one. The case is
  `testdata/fuzz_regressions/encoded_term/id_of_nonzero_reserved_bits`; `test_the_encoded_term_fuzz_regressions_replay_without_a_panic`
  in `tests/types/encoded_term/encoded_term_tests.rs` replays it with the two earlier cases and holds that the check
  accepts it and the reader refuses it so.

**`ultragraphdb_transport_http_client`**

- `http_reply` (added): 1,440,706 runs; cov 315, ft 1,178; corp 316/62 KB of 1,527 files; no failure.

**`ultragraphdb_transport_http_server`**

- `http_request` (added): 1,324,533 runs; cov 332, ft 1,236; corp 371/70 KB of 1,601 files; no failure.

**`ultragraphdb_common_telemetry`**

- `http_response_decode` (added): 590,757 runs; cov 1,350, ft 4,693; corp 1,113/217 KB of 2,282 files; no failure.

**`ultragraphdb_tool_toml`**

- `document` (added): 4,173,860 runs; cov 1,080, ft 4,489; corp 658/109 KB of 822 files; no failure.

**`ultragraphdb_common_config`**

- `config_record` (added): 832,868 runs; cov 83, ft 83; corp 14/128 B of 24 files; no failure.

**Regression cases under Bazel.** Of the nine crates, two hold cases. `ultragraphdb_core_terms_tests` replays
`encoded_term`'s three, `duration_parts_of_opposite_signs`, `timezone_offset_of_i16_min` and
`id_of_nonzero_reserved_bits`, each by name: 51 tests passed. `ultragraphdb_protocol_wire_tests` replays
`write_set_record`'s one by name: 713 tests passed. Both targets' `data = glob(["testdata/**"])` already names the
files. Both replays also pass under Cargo (`cargo test -p <crate> --test unit fuzz_regressions`).
`ultragraphdb_protocol_transport`'s `address_fuzz_tests.rs` replays `address_parse`'s regressions, of which there are
none; its data names the seeds alone, as it should until one is committed.

**Seen, closed in wave 3b.**

- `ResultTermView::term`'s documentation said a client reads a Result's literal through `EncodedTerm::view`. For
  tags `0x32` and `0x33` a Result holds the hash of the datatype's IRI where the reader takes a system ID, so the reader
  refused nearly every such term as above. A client now reads it through `EncodedTerm::result_view`.
- `object_identity` (`ultragraphdb_store_quads/src/utils/postings.rs`), a Scan's `after`, accepted tag `0x01` with
  16 bytes that are no system ID, by the same check, and placed it as a value's fingerprint. §13.13 now counts such an
  `after` as no whole object encoding, and `object_identity` refuses it, answered with Error 1.

**Seen, not changed.**

- `fuzz/Cargo.lock` gained `socket2` under `ultragraphdb_protocol_transport_compio`, written by this wave's fuzz builds:
  the crate took the dependency in 698a6098 and the fuzz workspace's lock had not been resolved since.

### Second half: cluster, core, store and transactions targets

Three of fifteen targets failed on their first run: one decoder overflowed, one decoder accepted bytes it could not write back, and one target asserted Dgraph's error text where the decoder names its own. All three are fixed. Each passed a second 60 s run, and every regression case in these crates replays under Bazel.

**What ran.** `scripts/fuzz.sh <crate> <target>` once per target at the default 60 s, `nightly` (cargo 1.100.0-nightly 2026-09-16), on macOS arm64, beside the other fuzz agent and Bazel builds, so the executions per second are the shared machine's. The changed targets are `session_entry`, `scan` and `superblock`. Runs are libFuzzer's `stat::number_of_executed_units`. Coverage is the `cov` edge count `fuzz.sh` reports, and features are left out because the script filters libFuzzer's status lines. Corpus is the file count in `fuzz/<crate>/corpus/<target>/` after the run, the target's seeds being read beside it.

- `ultragraphdb_cluster_metadata`
  - `command_decode`: 3,619,244 runs, cov 901, corpus 210 (16 seeds), no failure.
  - `snapshot_decode`: failed after 40,400 runs, cov 1,323: `attempt to add with overflow` at `metadata_snapshot_codec.rs:256`. A snapshot whose last placement version is 2^64 − 1 with no pending version made `checked_sub(0)` give 2^64 − 1, and the `+ 1` after it overflowed. The first expected version is now `checked_add(1)`, `None` past the top, which no loop reads when nothing is pending. The same panic had been found on 2026-11-03: `fuzz/ultragraphdb_cluster_metadata/artifacts/snapshot_decode/crash-d7979c88…` reproduced it unchanged and was never fixed. `cargo fuzz tmin` stopped at 1,263 bytes, the snapshot's structure, which is committed as `testdata/fuzz_regressions/snapshot_decode/placement_version_at_its_maximum`. `test_the_snapshot_fuzz_regressions_decode_or_are_refused` in `metadata_snapshot_codec_tests.rs` replays it through the target's rule, and it failed with the overflow when the old line was restored. Re-run: 3,640,880 runs, cov 1,664, corpus 751 (4 seeds), no failure.
- `ultragraphdb_cluster_placement`
  - `placement_decode`: 2,027,495 runs, cov 450, corpus 141 (10 seeds), no failure.
- `ultragraphdb_cluster_sequencer`
  - `record_decode`: 7,772,483 runs, cov 374, corpus 159 (11 seeds), no failure.
  - `snapshot_decode`: 4,292,593 runs, cov 581, corpus 268 (3 seeds), no failure.
- `ultragraphdb_core_catalog`
  - `catalog_contents_decode`: 625,763 runs, cov 891, corpus 589 (10 seeds), no failure.
- `ultragraphdb_core_codec`
  - `uid_pack_decode`: 1,993,551 runs, cov 487, corpus 286 (15 seeds), no failure.
- `ultragraphdb_core_ids`
  - `system_id_decode`: 29,218,152 runs, cov 72, corpus 6 (9 seeds), no failure.
  - `version_decode`: 45,215,506 runs, cov 24, corpus 4 (6 seeds), no failure.
- `ultragraphdb_core_keys`
  - `quad_key_parse`: 15,684,075 runs, cov 534, corpus 123 (19 seeds), no failure.
- `ultragraphdb_store_hnsw`
  - `hnsw_lists_decode`: failed after 3 runs, on its seed `matrix_zero_rows`. The target required a refused dead list to read `Cannot convert <text> to uint slice`, Dgraph's text, and `parse_edges` refuses with `HnswErrorEnum::MalformedDeadList(<text>)`, as it has since e3371c71. So any input that was not a list failed the target, and the target had never run past its seeds. The decoder is right and the target was wrong. The target now compares the refusal's variant and the text it names, `fuzz/ultragraphdb_store_hnsw/fuzz_targets/hnsw_lists_decode.rs:47-54`. No regression case was added: the input is a seed, and the unit suite already decodes the seeds. Re-run: 2,525,456 runs, cov 685, corpus 450 (13 seeds), no failure.
- `ultragraphdb_store_quads`
  - `referrer_set_decode`: 1,037,520 runs, cov 161, corpus 19 (15 seeds), no failure.
- `ultragraphdb_transactions_commit`
  - `session_entry`: failed after 1,138 runs, cov 330: `an accepted entry has one encoding`. A 56-byte entry, with no results and two empty node texts, decoded, then encoded with one node. `TermTable::iri` answers a text it already holds with that text's first index, so the table kept one node where the entry counted two, and a result naming node 1 would have been refused although the count allowed it. `decode_results` now refuses a node text the table already holds, `a session entry names a node twice` (`session_entry/mod.rs:189-194`), and `decode`'s errors list it. libFuzzer's crash minimisation left the 56 bytes, the fixed part and two empty lengths, as they were. They are committed as `testdata/fuzz_regressions/session_entry/a_node_named_twice`. The unit target's `data` names `testdata/fuzz_regressions/**` beside the seeds. `test_the_session_entry_fuzz_regressions_have_one_encoding_or_are_refused` replays the case, and `test_every_malformed_entry_is_refused_as_corrupted` refuses the golden entry's node named twice. Both failed with the check disabled. Re-run: 797,635 runs, cov 391, corpus 161 (2 seeds), no failure.
- `ultragraphdb_transactions_log`
  - `scan`: 1,807,266 runs, cov 552, corpus 145 (5 seeds), no failure.
  - `superblock`: 4,177,727 runs, cov 101, corpus 11 (3 seeds), no failure.

**Regression cases under Bazel.** The regression cases sit in three of these crates, and each crate's replay test ran under `--nocache_test_results`:

- `ultragraphdb_cluster_metadata_tests` (189 passed) ran `test_the_snapshot_fuzz_regressions_decode_or_are_refused` over `snapshot_decode/placement_version_at_its_maximum`.
- `ultragraphdb_transactions_commit_tests` (261 passed) ran `test_the_session_entry_fuzz_regressions_have_one_encoding_or_are_refused` over `session_entry/a_node_named_twice`.
- `ultragraphdb_transactions_log_tests` (190 passed) ran `test_the_scan_fuzz_regressions_end_within_a_block_of_the_bytes` over `scan/end_past_the_bytes`. Its `bdd` target (15 scenarios) ran "A fuzzer finding becomes a regression test" over the same case.

Each replay test asserts that its cases are not empty, so a case missing from the runfiles fails it. `metadata` and `transactions_log` already declared `testdata/**`. The other eight crates hold no `testdata/fuzz_regressions/`.

### Wave 3b: the specification's two questions (owner decision 36)

Two findings the first half saw and did not change are fixed, each test first: a client reads a Result's literal of
a datatype no code names, and a Scan's `after` of tag `0x01` holding no valid system ID is refused. Each test failed
before its fix and passes after it; the targets reaching the changed readers then ran their 60 s clean.

**A Result's typed literal.** `PROTOCOL.md` §10.1 gives tags `0x32` and `0x33` the hash of the datatype's IRI where
the store's encoding holds its system ID. `ResultTermView::term` hands the term over as an `EncodedTerm`, and
`EncodedTerm::view`, the store's reader, took those 16 bytes as a system ID and refused nearly every such term with
`Malformed`, "an ID that is no valid system ID". The bytes alone cannot tell the two encodings apart, so the reader
the caller names does:

- `ultragraphdb_core_terms`: `Datatype::Hashed(Hash128)`, a datatype by its IRI's hash, beside `Known` and `Id`.
  `EncodedTerm::result_view` reads an encoding as a Result holds it, as `view` reads the store's but for tags `0x32`
  and `0x33`, whose datatype it reads as that hash; it refuses tag `0x01`, an IRI or a blank node a Result names by
  its hash. `view` and `result_view` share one reader, `read_view` in `encoded_term_view.rs`, which takes how to
  read the datatype. `TermView::to_literal` reads a `Hashed` datatype as the IRI its `string` resolver gives for the
  hash, and refuses one that is no IRI with `Malformed`, "a datatype that is no IRI".
- `ultragraphdb_protocol_wire`: `ResultTermView::term`'s documentation names `EncodedTerm::result_view` as the
  client's reader. No code changed there.
- `ultragraphdb_common_compute`: `row_terms::result_term` matches `Datatype` exhaustively and takes the new variant
  as `None`; it reads a stored term through `view`, which never gives one.

`test_a_literal_of_another_datatype_reads_back_with_its_datatype_by_hash` in the wire crate's
`tests/types/result_term_view/result_term_view_tests.rs` builds a Result holding a `0x32` and a `0x33` term, reads
each back through `ResultView`, and reads its datatype by hash, its lexical form inline or as the term's text, and its
literal from the term's text and annotation. Written first against `EncodedTerm::view`, the path the documentation
named, it failed on the `0x32` term with that `Malformed`. `tests/types/encoded_term/encoded_term_view_tests.rs`
(new, three tests) holds `result_view` against `view` on both tags, a datatype by a valid ID read both ways, and the
refusal of tag `0x01`; `test_a_datatype_by_hash_is_the_iri_its_hash_names` in `term_view_literal_tests.rs` holds the
literal and its refusal. The wire-protocol spec gains the scenario *A literal of another datatype names its datatype
by hash*.

**A Scan's `after`.** `object_identity` (`ultragraphdb_store_quads/src/utils/postings.rs`) took an `after` of tag
`0x01` whose 16 bytes are no system ID, which `EncodedTerm::check` accepts, and placed it as a literal's fingerprint.
It now refuses it with `TermError` `Malformed`, "an ID that is no valid system ID", which `scan_check::after_of`
answers with Error 1 (Protocol), "range {index}: its `after` is not one whole object encoding", the message naming the
range, before any frame. `PROTOCOL.md` §13.13 now states it: an `after` is in the store's encoding, which names an IRI
or a blank node by its system ID, and one of tag `0x01` whose 16 bytes are no valid system ID (section 1) is no whole
object encoding. The quad-reads spec's scenario *A malformed `after` is refused* names that case. The tests: the
store's `test_a_resume_at_an_object_takes_the_identity_of_its_posting` (`quad_store_partition_tests.rs`) refuses an
ID's tag whose last reserved bit is set, and the partition core's
`test_a_range_naming_no_tenant_or_keyspace_or_a_wrong_after_is_refused` (`scan_check_tests.rs`) sends a Scan with that
`after` and reads Error 1 naming it. Both failed before the fix.

`openspec validate stage-5-5-three-role-refactor`: valid.

**Bazel.** `ultragraphdb_core_terms_tests` 55 passed and `bdd`; `ultragraphdb_protocol_wire_tests` 714 and `bdd`;
`ultragraphdb_store_quads_tests` 177 and `bdd`; `ultragraphdb_common_storage_tests` 144;
`ultragraphdb_common_compute_tests` 135. `bazel build //... --keep_going` succeeded.

**Fuzzing.** No target reaches `object_identity`. `encoded_term` now also holds that `result_view` opens every
encoding the check accepts but tag `0x01`, refused with its one message, and that its literal never panics
(`fuzz_targets/encoded_term.rs`); the reader's one refusal, an invalid system ID, stands. Each ran as
`scripts/fuzz.sh <crate> <target>` at 60 s on macOS (aarch64):

- `ultragraphdb_core_terms` `encoded_term`: 4,528,385 runs; cov 1,048; no failure.
- `ultragraphdb_protocol_wire` `body`, which reads Result terms through `ResultTermView::term`: 3,868,954 runs; cov
  7,394; no failure.

**Seen, not changed.**

- An `after` of tag `0x32` or `0x33` whose datatype bytes are no valid system ID still resumes by its fingerprint, as
  any literal the list does not hold; §13.13 states the rule for tag `0x01` alone, as decision 36 asks.
- `AGENTS.md`'s `ultragraphdb_core_terms` row names neither `EncodedTerm::result_view` nor `Datatype::Hashed`; the
  file is under other agents' edits.
- Clippy: the closing run's `make fix` and `cargo clippy --workspace --all-targets -- -D warnings` cover these crates
  and are clean.

## 12.3 Ultra Style conformance

Rule 5 was replaced on 2026-11-08, after this review ran: its quota of two checks per function, paired and in both spaces, a habit of C and Zig that Rust's types make redundant, gave way to "A bug stops the component through a returned invariant error". An invariant that would corrupt what is stored or sent is checked by `invariant(holds, name)?`, constants by `const` asserts, anything else by `debug_assert!`, and `src/` holds no run-time `assert!`. The counts of checks per function below are measurements, and no finding.

`make format` and `make fix` leave the tree unchanged and report nothing, once this wave's test files were formatted and the 16 warnings its tests raised were fixed; no `unimplemented` warning is left. The unlinted rules were reviewed in two halves, 43 crates and 31, each half listing its rules, how each was checked, each crate's exceptions with their reasons and what stays open for the lead: 8 departures fixed in the first half and 64 in the second, all keeping behaviour.

Wave 3b fixed the accept loops and inbox drains rule 2 named (owner decision 34) and swept the change's `src/` lines for
the documentation departures (owner decision 35): `#[must_use]`, `# Errors`, cast reasons, cancellation, dropped sends
and queue bounds; its two sections close this part. The open lists of both halves hold what remains after it.

### `make format` and `make fix`

Run once every agent of the wave had finished, `git diff` and the untracked files' hashes compared before and after each, `lean/` left out as another line of work's.

**`make format`** changed 16 files, all tests this wave added to or edited, by rustfmt alone; the change keeps them: `ultragraphdb_tests/tests/utils/harness_tests.rs`, `client_tokio`'s `tokio_tls_stream_tests.rs`, `cluster_partition`'s `partition_referrer_tests.rs`, `partition_host_bound_tests.rs` and `partition_host_read_tests.rs`, `common_compute`'s `read_request_tests.rs`, `common_config`'s `config_api_tests.rs` and `config_store_tests.rs`, `common_config_check`'s `check_target_tests.rs`, `common_frontend`'s `term_rules_tests.rs`, `common_role`'s `serve_tests.rs`, `store_quads`' `data_dictionary_tests.rs` and `quad_write_tests.rs`, `transactions_txn`'s `frame_changes_tests.rs` and `transaction_partition_tests.rs`, `transport_http_server`'s `serve_http_tests.rs`. buildifier changed nothing.

**`make fix`** changed no file and reported 16 warnings in seven crates' unit targets, every one in a test this wave wrote. Each cause is fixed in the test:

| Crate | Where | Lint | Fix |
|---|---|---|---|
| `store_sim` | `simulation_tests.rs:170` | `shadow_unrelated` | the second simulation is `fewer` |
| `common_frontend` | `term_rules_tests.rs:214`, 103 lines | `too_many_lines` | the nine changes are `change_cases` and `property_cases`, two helpers of 5 and 4 |
| `transactions_txn` | `frame_changes_tests.rs:173`, `202`, `320` | `shadow_unrelated` | the frame a closure reads is `frame`, not `changes` |
| `transactions_txn` | `frame_changes_tests.rs:210`, `341` | `wildcard_enum_match_arm` | an `if let` for the one literal; the four expected reads one `matches!` in `reads_in_place`, a function of its own, which also ends the two `graph` shadows at `324` and `328` |
| `transactions_commit` | `session_entry_tests.rs:132` | `unused_results` | the refusal's reason bound as `_reason: String` |
| `transport_http_server` | `serve_http_tests.rs:586`, `590` | `shadow_unrelated` | the TLS client's write and read are `sent` and `answered` |
| `cluster_metadata` | `metadata_snapshot_codec_tests.rs:503` | `wildcard_enum_match_arm` | the two refusals the target allows as two `matches!` |
| `cluster_partition` | `partition_host_bound_tests.rs:371`, `partition_host_read_tests.rs:161` | `shadow_unrelated` | `_answered` and `_values_page` |

`make fix` passes `-Wclippy::all` alone; `cargo clippy --workspace --all-targets -- -D warnings` then refused two more of the same tests, `too_many_lines` on `test_every_change_reads_its_terms` once rustfmt laid out its one `matches!` over 32 lines and `needless_pass_by_value` on `reads_in_place`, which now takes `&ChangeRef<'_>`. The second `make format` and `make fix` changed nothing and reported nothing; `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check` and `cargo fmt --all --check --manifest-path fuzz/Cargo.toml` are clean, and the workspace build prints no `unimplemented` warning; no `unimplemented!` or `todo!` stands in any crate. The seven crates' unit targets pass under Bazel.

**Wave 3b.** Run the same way once every agent of wave 3b had finished. `make format` changed nothing, buildifier
included. `make fix` changed no file and reported 4 warnings in three crates' unit targets, each in a test wave 3b wrote,
each fixed in the test:

| Crate | Where | Lint | Fix |
|---|---|---|---|
| `protocol_wire` | `result_term_view_tests.rs:81` | `shadow_unrelated` | the long literal's hash is `long_hash`, not `lexical` |
| `common_frontend` | `owner/owner_catalog_tests.rs:243`, 76 lines | `too_many_lines` | the declarations of every kind are `every_kind`, a helper |
| `common_frontend` | `role_session_tests.rs:343`, 81 lines | `too_many_lines` | the endpoint where a partition's commands arrive is `refused_where_partitions_arrive`, a helper |
| `store_apply` | `changeset_core_rebuild_tests.rs:847`, 74 lines | `too_many_lines` | the declaration of four levels is `redeclare_with_levels`, a helper |

The second `make format` and `make fix` changed nothing and reported nothing. `cargo clippy --workspace --all-targets --
-D warnings`, `cargo fmt --all --check` and `cargo fmt --all --check --manifest-path fuzz/Cargo.toml` are clean, and no
`unimplemented` warning is printed. No `src/` holds a run-time `assert!`, `assert_eq!` or `assert_ne!` outside a
`#[cfg(test)]` module (`tool_bench`'s `apply_run.rs` and `pipeline_run.rs`) and the test crate `ultragraphdb_tests`
(`harness.rs:490`); every other `assert!` under `src/` is a `const` assert. The three crates' unit targets pass under
Bazel. `cargo clippy --all-targets -- -D warnings` over the fuzz workspace, which `make fix` does not run, refuses three
targets that neither wave of this group edited: `protocol_wire`'s `body_fields` (9: `manual_inspect` 6,
`too_many_arguments` 1, `type_complexity` 2), `client_core`'s `reply` (2) and `protocol_transport`'s `server_connection`
(1).

### The unlinted rules, first half: protocol, transport, client, core, store, transactions, traits, error

**Scope.** The 43 crates under the eight directories with `src/` changes since `1f4b67a3`, read at `39bf3977`: 978 source files, 46,049 added lines. A line `git blame` gives to `08ff8f26` counts as unchanged (16 of the hits below). The lint commits `1dd895b6`, `26cab7ae`, `7f9fa641`, `ae110cd7` and `6c1af360` touched every crate; `core_text` and the error crates `error_invariant`, `_json`, `_log`, `_num`, `_options` and `_store` changed through them alone, as `core_tok` and `core_types` did except for one commit each. A hit on a line those commits wrote counts as the sweep's below, and as written otherwise. Every crate's `Cargo.toml` takes `[lints] workspace = true` and adds no `[lints.*]` table. No `unimplemented!`, `todo!`, `unreachable!`, `macro_rules!` or `#[allow]` is left in any of their `src/` trees.

#### The unlinted rules and how each was checked

The method was a scan of the added `src/` lines (`scan.py` over `git diff -U0 1f4b67a3 39bf3977`), each hit blamed and read in place, and a reading where no pattern exists.

| Rule | Check |
|---|---|
| 1, acyclic call graph | A function calling itself by name, `Self::` or `self.` in its own body, over every function holding an added line; mutual recursion is not checked |
| 1, static dispatch | `\bdyn\b`, `fn(` in a type, `impl/dyn Fn` |
| 2, bounded loops | Every added `loop`, `while`, `repeat`, `cycle`, `from_fn` and `successors`, read for a named bound or a comment saying why it ends; capacities read only where a loop or queue was found |
| 3, planes | `data plane` or `control plane` in each crate's `src/` |
| 5, check density | `invariant(`, `debug_assert*!` and `const _: () = assert!` against the `fn`s on added lines, `utils_test/` left out |
| 7, results | `.ok()` read for a dropped error, `let _`, `assert!` in `src/`, `resume_unwind`, `unreachable!`; the held-back `indexing_slicing` and `arithmetic_side_effects` not run |
| 8, `cfg` | `#[cfg(` and `cfg!(` other than `feature = "test-utils"` |
| 9, shared ownership | `Arc`, `Rc`, `Cell`, `RefCell`, `Mutex`, `RwLock`, `Once*` and atomics at their declaration, read for a reason and for nesting |
| 10, exceptions | `#[expect(` and `Exception (Ultra rule` |
| Types, casts | ` as (u|i)(8..128|size)` and `as f32/f64`, read for a stated reason; `cast_possible_truncation` and `cast_sign_loss` are held back |
| Types, `#[must_use]` | a `pub fn` returning neither `Result` nor `()` with no `#[must_use]` |
| Types, `Copy` at 16 bytes | each `derive(Copy)` struct's fields summed where their sizes are known |
| Concurrency, atomics | every `Ordering::` read for a comment on why it suffices |
| Concurrency, cancellation | every added `async fn` and its type's documentation, for what dropping its future leaves |
| Performance, explicit options | `..Default::default()` and `..X::default()` |
| Naming, qualifiers last | `max_*`, `min_*`, `MAX_*`, `MIN_*` |
| Comments, `# Errors` | a `pub fn` returning `Result` with no `# Errors` heading |
| Off-by-one, half-open | `..=` |
| AGENTS.md structure | more than one `pub` type in a new file; public fields in a public struct; `pub mod` in `lib.rs`; `use ultragraphdb_x::module::`; the error shape `FooError(pub FooErrorEnum)`; a subscriber installed by a library; every `tests/**/*.rs` reached from a test root through `mod` or `#[path]` |

Clean on every crate: no recursion written by the change, no `dyn` beyond `Error::source`'s signature (`error_cluster/src/errors/cluster_error/cluster_error_error.rs:12`), no `cfg` beyond `test-utils` and `target_os`/`target_arch` in `transactions_log`, no `let _` or `.ok()` dropping an error without its reason, no `pub mod` in a `lib.rs`, no import reaching through a module, every error a `FooError(pub FooErrorEnum)`, no subscriber, and every test file registered.

#### The exceptions the code takes, with their reasons

- **`#[expect]`, 17, all written by the lint sweep with their reasons.** `clippy::disallowed_methods`, 11: the shells' clock reads, `client_compio/src/types/client/mod.rs:53` and `:62` and `client_tokio/src/types/client/mod.rs:25` ("the shell reads the clock and tells the core, which reads none"); `core_algo/src/uidlist.rs:391` and `:475`, the 10 merge threads ("Dgraph's algo merges large inputs on 10 goroutines; query code is outside the storage simulation"); `store_kv`'s `moka_list_cache/mod.rs:25` and `term_cache/mod.rs:24` (per-process hash keys against collisions clients choose), `syncer/mod.rs:29` (the sync thread) and `thread_executor/thread_executor_executor.rs:14` and `:29` (threads, which the simulator's stepped executor replaces); `store_sim/src/bin/store_sim_fuzz.rs:59` (fresh seeds from the wall clock, each printed). `dead_code`, 5, on the generated tables, `core_text/src/tables/mod.rs:10`, `:13`, `:16`, `:19` and `core_tok/src/tables/mod.rs:9` ("the generator mirrors Go's tables whole"). `clippy::large_enum_variant`, 1, `transactions_txn/src/types/transaction/mod.rs:32` (moving `Prepared` once costs less than a heap allocation per read-write commit).
- **`Exception (Ultra rule 7)`, 1:** `client_core/src/types/client_core/mod.rs:213`, `transmit`'s stamp, whose error only a frame shorter than its header gives (group 10).
- **Rule 7, a panic carried on.** `core_algo/src/uidlist.rs:415` and `:506` and `store_kv/src/types/thread_executor/thread_executor_executor.rs:45` `resume_unwind` a joined thread's panic, each with its comment: the merge threads run panic-free code, and the executor panics as running the tasks inline would.
- **Rule 7, an error turned into `None` on purpose.** `protocol_wire`'s `error_reply/error_reply_body_builder.rs:49` and `term_table_view/mod.rs:303` (sweep), each with its comment: a checked table interns whole. The 46 `.ok()`s the change wrote turn a failed parse into the absent value their function returns, as `record_registration.rs:74-76` and `parse_reply.rs:48`; the one that looked like a dropped error is commented below.
- **Rule 1, a function pointer.** `store_quads/src/types/quad_store/mod.rs:36` holds the fingerprint as `fn(&[u8]) -> u64`, passed to `utils/postings.rs:42` and `:69`; `with_fingerprint` (`mod.rs:91`) says why, a test forcing collisions. The field predates the change; the two parameters are the change's.
- **Rule 9, shared ownership with its reason at the declaration.** `client_core`'s `stream_state.rs:35` and `client_core/mod.rs:85`, `:88`; `protocol_transport`'s `buffer_pool/mod.rs:36` (`Send` and `Sync` for a multi-threaded runtime, one uncontended lock per buffer); `protocol_transport_compio`'s `tls_read_half.rs:19` and `tls_write_half.rs:18`; `store_apply`'s `changeset_applier/mod.rs:46` and the locked block of `changeset_core/mod.rs:62-73`; `store_hnsw`'s `persistent_hnsw/mod.rs:55-56`; `store_kv`'s `fjall_store/mod.rs:85-90` and `term_cache/mod.rs:17-19`; `store_posting`'s `list/mod.rs:38-40` and `mutable_layer/mod.rs:33-34`; `store_quads`'s `dictionary/mod.rs:33-35`, and `kernel_scan.rs:27`, `page.rs:18`, `page_walk.rs:31`, whose `Arc<List>` `KernelScan::holding` explains (`kernel_scan.rs:67`, a list crossing many pages is read once); `store_sim`'s `sim_store/mod.rs:52-53`, `:74-77` and `store_model/mod.rs:35`; `traits_blocks`' and `traits_catalog`'s test sources; `transport_tls`' `tls_acceptor/mod.rs:26`, `tls_connector/mod.rs:22`, `tls_trust/mod.rs:28` (rustls takes its configuration as an `Arc`), `utils_test/test_pipe.rs:20` and `test_pki.rs:62`.
- **Rule 9, nested past one level, its sharing explained but not its depth.** `store_posting`'s `List` holds `Arc<PostingList>` and `Arc<[Arc<PostingList>]>` (`list/mod.rs:39-40`, the sweep's `rc_buffer` turned `Arc<Vec<Arc<_>>>` into a slice) and `MutableLayer` `Arc<BTreeMap<u64, Arc<PostingList>>>` (`mutable_layer/mod.rs:34`, before the change); `store_quads` holds that `List` in an `Arc` (above), and `store_apply`'s `Arc<ChangesetCore>` holds `Mutex<BTreeMap<u64, Arc<Catalog>>>` (`changeset_core/mod.rs:64`, before the change); `store_sim`'s `Arc<Mutex<Inner>>` holds `Arc<State>`s, copied on write.
- **Naming, the protocol's names.** `max_frame` (28 written uses), `MAX_FRAME_SIZE`, `MAX_TERM_DEPTH` and `max_write_set` are `PROTOCOL.md`'s and `ULTRA_STYLE.md`'s own; `MAX_FRAME_FLOOR`, `MAX_EPOCH`, `max_read_set`, `max_list_size` (`store_posting`, Dgraph's), `max_levels` (`store_hnsw`, Dgraph's) and `max_delay` (`protocol_transport`'s simulated network) follow them, no comment saying so.
- **Rule 8.** `transactions_log/src/types/compio_file/compio_file_durable_file.rs:60` and `compio_storage/compio_storage_durable_storage.rs:18-30` select by `target_os` and `target_arch`, which the rule allows.

#### Fixed, comments only

Eight departures the change wrote, each now stating its reason; the eight libraries build under Bazel (`bazel build` of the eight `rust_library` targets, exit 0), and no behaviour changed:

- `client_core/src/types/result_stream.rs:17-19`: why `read_to_end` is an `Arc<AtomicBool>` (the core counts the handle's holders; atomic so the handle is `Send` and `Sync`) and why `Relaxed` suffices (the flag orders no other memory).
- `store_kv/src/types/term_cache/mod.rs:18-20`: why the generation is `SeqCst` — a reader fills then loads, a writer moves on then invalidates, a store-then-load pattern Acquire and Release do not order.
- `store_posting/src/utils/walk_lists.rs:46` and `store_quads/src/types/quad_store/quad_store_partition.rs:281`: why the merge loops end.
- `transport_http_client/src/types/http_client.rs:164-165`: why the reply loop ends.
- `transport_http_server/src/utils/serve_http.rs:109-111`: `serve_listener`'s accept loop is an event loop, ending when its caller drops the future, the accepted connections going on in their detached tasks — its end and its cancellation.
- `transactions_txn/src/types/write_set_builder/mod.rs:252`: the `filter_map(... .ok())` drops no posting, since `by_objects` held that every one names its object.
- `protocol_wire/src/types/term_table/mod.rs:157-158`: `import`'s `None` also covers a term this table refuses.

#### Open, for the lead

Each is wider than a local fix, or needs a decision. Wave 3b's sweep (*Wave 3b: the documentation sweep* below)
closed items 2 to 5, 7 and the loops of 8; each item says what is left of it.

1. **Copy types above 16 bytes, none with a reason.** Known sizes at least: `protocol_wire`'s `Bind` 52, `ReferenceRegistration` 70, `write_set_record_view`'s `Fixed` 56, `ResumeRow` 36, `Lease` 30, `Recall` and `RecordRegistration` 28, `term_table_view`'s `Entry` 26, `OperationFields` 17, and the borrowed views holding two or more slices (`CatalogView`, `EpochCloseView`, `PlacementView`, `ScanView`, `ScanRange`, `KeyRange`, `ObjectRecord`, `VerifyView`, among 20); `store_apply`'s `ReadCounts` 40 and `MarkedReference` 28; `store_quads`' `ReferrerMember` 20; `transactions_commit`'s `EpochStamp` 44; `transport_http_server`'s `HttpLimits` 32; `core_types`' `CivilTime` 56. Dropping `Copy` changes every caller that copies; group 2 did so for `Elem`. Wave 3b left them for that reason.
2. **`#[must_use]`, closed in wave 3b.** The review counted about 1,170 `pub fn`s returning neither `Result` nor `()`: `protocol_wire` 544, `transactions_commit` 83, `store_quads` 82, `store_apply` 70, `error_cluster` 37, `transactions_txn` 36, `store_posting` 33, `core_catalog` 32, `error_protocol` 30, the rest below 30 each. The sweep put 921 on these crates' changed lines (615 in protocol, transport, client, traits and error; 306 in core, store and transactions), every hit of `clippy::must_use_candidate` and `clippy::return_self_not_must_use` passed on the command line, and a second run finds none in scope. The lint passes over generic constructors such as `TlsDial::new`, which carry none.
3. **`# Errors`, closed in wave 3b but one.** The review found it absent on 86 `pub fn`s returning `Result`: `store_quads` 22 (`write_terms/mod.rs` 8, `quad_write/mod.rs` 6, `quad_store_read.rs` 4, `quad_store/mod.rs:48`, `data_dictionary`, `shapes_pass.rs:35`), `store_apply` 18 (`changeset_applier/mod.rs:61`, `:72`, `:92`, `:477-487`, `changeset_core`'s 10, `changeset_write` 2, `predicate_prefix/mod.rs:77`), `protocol_wire` 16 (`check_request`, `from_client_frame`, `stamp_stream`, `push_row`, among them), the three client crates 16 (`begin_with`, `describe`, `direct_write`, `conditional_write`, `resume_session`, `grant`), `store_kv` 4, `core_catalog` 3, `store_posting` 3, one each in `core_mapping`, `protocol_scram`, `protocol_transport` and `store_sim`. The sweep wrote 71 (34 and 37), by `clippy::missing_errors_doc`'s hits. Open: `EncodedTerm::result_view` (`core_terms/src/types/encoded_term/encoded_term_view.rs:34`), added in wave 3b, names its refusals in its summary, as `view` beside it does, under no heading.
4. **Casts, closed in wave 3b.** The review found 121 written with no stated reason, 50 by the lint sweep: `protocol_wire` 88, nearly all `usize as u64` and `u32 as usize`, lossless on the 64-bit targets the repository builds for, which no comment says; `transactions_commit` 11, `client_core` 6, `transactions_log` 4, `transactions_txn` 3, `core_catalog`, `core_mapping` and `protocol_transport` 2 each. `core_mapping/src/types/random_source.rs:54` truncates on purpose (Lemire's low product), its comment saying what but not that the cast keeps the low half. `u64::try_from` or a stated target width would settle the rest. Wave 3b gave 175 their reason (121 and 54, 4 of the 54 stated already) and turned `self.delta as u64` into `cast_unsigned()`; `protocol_wire/src/utils/constants.rs` asserts `usize::BITS >= 32` and `<= 64` at compile time, `core_terms` `ID_LEN <= 255`, and `random_source.rs` says the low half is kept on purpose. Open: the other crates' reasons resting on a 64-bit `usize` hold no such `const` assert.
5. **Cancellation, closed in wave 3b.** It was unstated on 35 `async fn`s: `protocol_transport_compio` 22 (`Carrier`, `Listener`, `CompioTlsStream`, the TLS halves), the two shells' `tls_dial.rs:36`, `transport_http_client` 3, `transport_http_server` 4 besides `serve_listener`, `transactions_log`'s `wal_ordered_log.rs:249` and `wal_write.rs:41`, `transactions_commit`'s `replay/mod.rs:79` and `:107`. The client shells state it once on `Client` and `TokioTlsStream`, `BufferPool` on its type. Wave 3b stated it on 50, each read against its awaits (45 and 5).
6. **Atomics' orderings unexplained.** `store_sim/src/types/sim_clock.rs:25`, `:30` (`SeqCst` where each value is read alone); `store_kv`'s `gc_settings.rs:31-43` (Acquire, Release and AcqRel on values read alone; line 43 is the sweep's), and outside the diff `fjall_store`'s `applied_ts`, `poisoned`, `next_temporary` and `store_apply`'s `sequenced` (`changeset_core/mod.rs:111`, `:115`). Whether a weaker ordering suffices is behaviour, so wave 3b left them.
7. **Rule 1, recursion in the Go ports the sweep moved, closed in wave 3b.** `core_types`' `regioncoverer/mod.rs:198` `expand_children` (bounded by its level count), `shapeindex/mod.rs:246` `update_edges` (the cell tree, at most 30 levels), `geometry_wkb.rs:332` `write_one` (one level of a multi-geometry), and `core_text`'s `table/mod.rs:123` `append_next` (a decomposition's runes). `expand_children` (at most 3 deep) and `append_next` (one level) now state their bound; `update_edges` and `write_one` stated theirs already.
8. **Rule 2, loops the sweep moved with no comment on their end, now stating it (wave 3b):** `core_text/src/utils/parse.rs:355`, `:362`, `:385`, `:389`, `:400`, `:404`, `core_tok/src/bleve/segment_words.rs:149`, `:172`, `core_types`' `big_float_format.rs:242` and `conversion.rs:564`. Open: `serve_listener` spawns a task per accepted connection with no count, each bounded in time by `HttpLimits` alone, and accepts again at once after a failed `accept`. A bound on connections in flight and a pause each need a number, or `AcceptPace` moved beneath `common_service` (*Wave 3b: accept loops and inbox drains* below).
9. **Rule 3, planes stated in 7 files** (`client_core` 1, `core_codec` 2, `transport_http_client` 2, `transport_http_server` 1, `transport_tls` 1) of the 1,322 in the eight directories' `src/` trees.
10. **Rule 5, checks per function, measured against the quota rule 5 no longer sets.** Per function on added lines: `core_algo` 1.06, `core_terms` 0.47, `transactions_log` 0.43, `core_codec` 0.33, `core_keys` 0.28, every other crate below 0.25, and 0 in `protocol_wire` (818 functions), `protocol_scram`, `store_hnsw`, `store_kv`, `store_sim`, `transport_http_client` and the error crates. Group 10 recorded the same for the client; the decoders refuse bad bytes with operating errors, which the rule does not count.
11. **One type per module, five new files with two.** `core_mapping/src/types/change_ref.rs` (`ChangeRef`, `StatementView`), `store_quads/src/types/referrer_set/mod.rs` (`ReferrerSet`, `Registered`), `transactions_txn/src/types/frame_changes.rs` (`FrameTerms`, `FrameChanges`), `transport_http_client/src/types/http_reply.rs` (`HttpReply`, `ParsedReply`), `transport_http_server/src/types/parsed_head.rs` (`HttpHead`, `ParsedHead`). The error crates' `FooError` and `FooErrorEnum` share a file by the repository's error convention. A split moves each crate's mirrored tests too; owner decision 35 lets them stay.
12. **Public fields on a public type.** `store_quads`' `Registered` (`referrer_set/mod.rs:30`, `:32`, exported from `lib.rs:36`) carries `pub generation` and `pub changed`; `referrer_set_tests.rs` builds and reads them. The owner's choice.
13. **Half-open ranges, 17 `..=` left of 19.** Wave 3b wrote `core_catalog`'s `catalog_types.rs` hierarchy walk `0..len + 1` and `transactions_log`'s `wal_recovery.rs` chain walk `0..segments + 1`. The 17 left each match an interval `PROTOCOL.md` writes closed: `contains` checks (`client_handshake/mod.rs:68`, `term_table_view/mod.rs:45`, `compute_read_view_body_view.rs:74`, `challenge_view_body_view.rs:32`, `result_view/mod.rs:117`, `report_view_body_view.rs:48`, `commit_operation_layout.rs:78`, `changes.rs:434` and `:436`, `parse_reply.rs:49`), match arms (`write_set_record_view/mod.rs:403`, `:407`, `operation.rs:35`), `entries_sequence.rs:177` and `:198`, `ROLE_COMMANDS: RangeInclusive<u16> = 512..=767`, which other crates call `contains` on, and `serve_http.rs:81`'s loop of one turn more than a length. Written `start..end + 1` each keeps its behaviour but reads less like the specification, and `ROLE_COMMANDS` would change type in every crate naming it. Whether the rule reaches intervals the specification states closed is the owner's call.
14. **Found by the sweep, open.** `ClientCore`'s `pending` map keeps a request's answer until `take_reply`, and a shell request future dropped after its request was sent takes its `Copy` `Pending` with it, so the core never removes that entry: a caller wrapping requests in its own timeouts grows the map by one per dropped future. The map predates the change (`daadf494`, group 10); a fix needs a held flag, as `ResultStream`'s `read_to_end` has, or pruning when the request's stream ends. `CommitStep::with_queue` bounds `waiting`, `deferred`, `held` and the pipeline's `registrations`, but defaults to `usize::MAX` and no caller outside the crate's tests sets it, although its doc says "the node sets it": the bound needs a value (`PROTOCOL.md` §11 names the Busy refusal for a full queue, not its size), the storage role setting it and a test at the bound and past it. `ChangesetApplier::over` returns `Result` and never fails, its `# Errors` saying so; returning `Self` changes 10 call sites in 9 files across six crates.

Not checked: rule 3's allocations function by function, rule 6's scopes, the memory section's buffer bleed and large values, the signatures' argument order, and abbreviations; each needs a reading of every function, which this review did not make.

### The unlinted rules, second half: `ultragraphdb_common/`, `ultragraphdb_cluster/`, `ultragraphdb_tool/`, `ultragraphdb/`

The rules no active lint enforces were checked over every line `git diff 1f4b67a3 HEAD` adds to the `src/` of the 31 crates
those four folders hold that the change added files to or edited: the eleven `ultragraphdb_common_*` (`_version` gained one
line), the four `ultragraphdb_cluster_*`, `tool_bdd`, `tool_bench`, `tool_test`, `tool_toml`, and the twelve crates under
`ultragraphdb/`, 45,000 added `src` lines in all. The six `_tests` crates and `ultragraphdb_tests` hold almost all their
code under `tests/`, which was checked for macros and `#[expect]` alone. Sixty-four departures are fixed in place, four
signatures, one rename and 59 comments. Rule 3 is not met across the new data-plane crates and is listed as open for the
lead. Wave 3b stated the cancellation of every added `async fn` and the bound of every channel and queue, paced the
accept loops that spun on a failing `accept` and bounded the inbox drains; what stays open is below.

No crate's `Cargo.toml` overrides the workspace lint table; there is no
`#[allow]` and no `unimplemented!` or `todo!` in any of the 31 crates.

#### The unlinted rules, and how each was checked

The table holds every rule of `ULTRA_STYLE.md` and `AGENTS.md`'s code-structure rules that no lint of `[workspace.lints]`
enforces, the commented-out lints included. A pattern is matched against the added lines, then each hit was read.

| Rule | Check |
|---|---|
| 1. Acyclic call graph; no `dyn`, function pointers or stored callbacks | `\bdyn\b`, `[:<(,]\s*fn\s*\(`; a function whose body calls its own name, `self.name(` or `Self::name(` |
| 2. Every loop, queue, retry and wait bounded; an event loop says so | every `loop {` and `while`, read with the function's doc; `unbounded`, `mpsc::channel()`, `VecDeque::new()`; `repeat`, `cycle`, `from_fn` without `take` |
| 3. Data-plane memory allocated at startup; each module states its plane | `data plane` or `control plane` in a crate's `src/`; the queues of rule 2 |
| 5. At least two checks per function | `debug_assert`, `invariant(`, `const _: () = assert` against `fn` definitions, per crate |
| 6. Smallest scope, held once (beyond `shadow_unrelated`) | not reviewed past the lint |
| 7. No `let _ =`, no `.ok()` dropping an error, no `assert!` or `unreachable!` under `src/` | `let\s+_\s*[:=]`, `\.ok\(\)`, `drop(…send(…))`, release `assert!`/`assert_eq!`, `unreachable!` |
| 7. Indexing, slicing, arithmetic on outside values | `clippy::indexing_slicing` and `arithmetic_side_effects` are commented out, and `design.md` ("Ultra Style first, then behaviour") keeps them out of this change: not counted |
| 8. No repository macros; each `cfg` other than platform and `test-utils` justified | `macro_rules!`, `#[cfg`, `cfg!(` |
| 9. Each `Arc`, `Rc`, `Cell`, `RefCell`, `Mutex`, atomic and trait object justified; one level of nesting | those type names in declarations, read with the field's and the type's doc; `Arc<Mutex<…<Box|Arc|…<` |
| Types: `as` lossless and saying so | `cast_possible_truncation` and `cast_sign_loss` are commented out and kept out of this change by `design.md`: not counted |
| Types: `#[must_use]`, simple signatures, no swappable arguments | not reviewed: no tool marks a candidate |
| Concurrency: each atomic's ordering says why it suffices | `Ordering::` read with the comments around it |
| Concurrency: each `async fn` states its cancellation safety | `async fn` whose doc names a cancel or a dropped future |
| Performance: no `..Default::default()` of a library call | `\.\.\s*(Default::default\(\)\|\w+::default\(\))` |
| Naming: qualifiers last (`_max`, not `max_`) | `\b(max\|min)_[a-z]` in a binding, field or function |
| Comments: `# Errors` on every public function returning `Result` | `pub fn` returning `Result` without `# Errors` in its doc |
| Ranges half-open | `..=` |
| Logging: structured fields, not a formatted message | `(info\|warn\|error\|debug\|trace)!(…"…{` |
| One type per module; errors as `FooError(pub FooErrorEnum)` | more than one `struct`, `enum` or `trait` at a file's top level; the error's shape under `src/errors/` |
| Tests mirror `src/`, every test file registered | an added `src` file without a `<name>_tests.rs`; a file under `tests/` no `mod` or `#[path]` names |
| Public types' fields private; exports at `lib.rs`; imports from a crate's root; no prelude | `pub` fields of a type `lib.rs` exports; `pub mod` in `lib.rs`; `use ultragraphdb_x::module::`; `prelude` |
| Service binaries: `main.rs` and `service.rs`, no library | the files under each binary's `src/` |

#### Rules met in every crate reviewed

- **Rule 8.** No `macro_rules!` under any `src/`; `ultragraphdb_tests/tests/shells.rs:334` defines one test macro. The only
  `cfg` under `src/` is `feature = "test-utils"`, in `common_frontend/src/lib.rs:16,40,42`.
- **Rule 10.** Every `#[expect]` names its reason: 37, of which 35 are `clippy::disallowed_methods` on a clock read or a
  thread the shell owns, which `clippy.toml` provides for, and 2 are `dead_code` on Go's generated tables
  (`common_utils/src/tables/mod.rs:10,13`, "the generator mirrors Go's tables whole").
- **Errors.** Every error under `src/errors/` is `FooError(pub FooErrorEnum)`, the struct and its enum in one `mod.rs` as
  across the repository.
- **Exports and imports.** No `lib.rs` declares a `pub mod`; no added line imports through a module of another
  `ultragraphdb_` crate; no prelude.
- **`# Errors`.** Every exported `pub fn` returning `Result` has one, apart from `ultragraphdb_tests`' harness
  (`src/utils/harness.rs:166–307`, eight functions of a test crate returning `TestResult`).
- **Service binaries.** Each of the five is `main.rs` and `service.rs`; `ultragraphdb_config_cli` is `main.rs` and
  `restore.rs`. A `service.rs` holding several private types (`storage`: 3, `supervisor`: 5) follows from that rule.
- **`..Default::default()`.** The five spreads are of the crate's own types (`Recalling`, `Membership`, `Routes`,
  `ChangesetWorkload`), not a library call's options.
- **Test registration.** Every file under a `tests/` is named by a `mod` or by `#[path]`; `cluster_metadata` and
  `cluster_sequencer` include their models by `#[path]` from `tests/bdd.rs`, and `tests/utils/mod.rs` says why.

#### Fixed

Each fix keeps behaviour. Bazel: the 15 touched packages build (`bazel build`, 87 targets); unit and BDD targets of every
crate with a changed signature or name pass: `common_compute` unit 133, `common_frontend` unit 58 and bdd 119 scenarios,
`common_role` unit 38 and bdd, `common_service` unit 47, `tool_bench` unit 88, src 5 and bdd 2 scenarios, `tool_toml`
unit 68 and bdd 11 scenarios. The other edits are comments.

- **Rule 1, static dispatch.** `Link::close` (`common_compute/src/types/link.rs:150`), `resume_refused`
  (`common_compute/src/utils/refusals.rs:24`) and `cannot_listen` (`common_service/src/utils/listen.rs:89`) took
  `&dyn Display`; each takes `&(impl Display + ?Sized)`. `split_run` (`tool_toml/src/utils/chars.rs:28`) took a function
  pointer; it takes `impl Fn(char) -> bool`.
- **Rules 1 and 9, trait objects.** Fifteen closures take an error as `&dyn Display`, since a closure takes no type
  parameter; each now says so: `common_compute` `core_listener.rs:76`; `common_frontend` `owner_thread.rs:162`,
  `utils_test/role_double.rs:54`; `common_role` `role_client/mod.rs:170`; `common_service` `bootstrap.rs:89`,
  `listen.rs:74`, `run_service.rs:140,383`; `ultragraphdb_config` `service.rs:142,168`; `ultragraphdb_config_cli`
  `restore.rs:71`; `ultragraphdb_frontend` `service.rs:161`; `ultragraphdb_storage` `service.rs:552`;
  `ultragraphdb_supervisor` `service.rs:248,581`. `Error::source`'s `&(dyn Error + 'static)` is std's signature.
- **Rule 2, loops.** Twenty-nine loops whose end neither a comment nor the function's doc stated now carry one, after the
  lifts' precedent (`notes/group-4-txn.md` 4.1, `notes/group-7.md` 7.1): six Scan loops in `common_compute`, ending at the
  frame the partition marks last, each frame awaited within the stream's stall bound (`compute_core_lookups.rs:105,138`,
  `partition_rows/mod.rs:203,250`, `partition_rows_fill.rs:121`, `partition_rows_terms.rs:160`); the heap's two sifts
  (`frontier.rs:30,48`); the merge (`run_read.rs:467`); config recovery (`config_store/mod.rs:42`); the role link's two
  tasks (`role_link_tasks.rs:65,93`) and the test double's accept (`role_double.rs:78`); a connection's serve loop
  (`serve.rs:212`); `run_service`'s wait and the parent watch (`run_service.rs:197,284`); `tool_test`'s two paging loops
  (`partition_workload/mod.rs:592,952`); in the binaries the accept loops (`compute` `service.rs:172`, `frontend`
  `service.rs:214,253`, `storage` `service.rs:592`), compute's peer watch (`service.rs:205`), the config service's wait
  (`service.rs:229`), storage's wait, discard thread, answer forwarding and frontend requests (`service.rs:172,393,843,903`),
  and the test double's request read (`ultragraphdb_tests` `harness.rs:332`).
- **Rule 9, shared state.** `RouteSink::hold` (`common_frontend/src/types/route_sink.rs:22`) and `ReadPins::taken`
  (`common_compute/src/types/read_pins.rs:28`) say why an `Rc` holds them.
- **Atomics.** No atomic said why its ordering suffices. `ConnectionBound` and `OpenRequests` (`common_role`, which
  `HeldConnection` and `OpenFrame` refer to) say a count orders no other memory; `Readiness` and `Stopping`
  (`common_service`) that a `Release` store and an `Acquire` load suffice for a flag; the stop flag's loads in
  `fetch_settings.rs:34`, `run_service.rs:197,284` and the config service, the supervisor's `Signals`, `tool_bench`'s
  `partitions_run::Shared` and `ultragraphdb_tests`' `DIRECTORIES` likewise.
- **Naming.** `tool_bench`'s `apply_run.rs:418–419` named `max_l0` and `max_sealed`; they are `l0_tables_max` and
  `sealed_memtables_max`, the JSON keys unchanged.

One fix was reverted: `role_link_tasks.rs:42`'s warning is a formatted message, and turning `address` and `dial_ms` into
fields failed `common_frontend:bdd`, whose step reads `"{DIAL_MS} ms"` in the line
(`tests/endpoint_steps_requests.rs:2601`). The message stays, its text pinned by the scenario.

#### Per crate: exceptions with their reasons, and what stays open

Counts are over the added lines. "Open" is a departure with no reason stated, too large or too behavioural to fix here.

**`ultragraphdb_common_compute`** (4,454 added `src` lines, 208 functions). Exceptions: `RefCell` and `Rc` in
`ComputeCore`, `Link`, `ReadShared` and `ReadPins`, each type's doc stating that a read runs wholly on one core's thread;
the clock read in `utils/clock.rs:9`. Open: no module states its
plane, and the core is data plane (rule 3); `link.rs:66,109` and `compute_session.rs:38,70` are unbounded channels, and
`partition_rows/mod.rs:83` an unsized `VecDeque`, each now stating what bounds it (wave 3b), `Link`'s frames other than
Entries bounded only by the peer keeping the protocol, which a count on the compute side would change;
`link.rs` holds `Link` and `OpenStream`, `run_read.rs` `Opened` and `Asked` (one type per module); 21 added files have no
`_tests.rs` of their name (`graph_slot.rs`, `page_entry.rs`, `scan_facts.rs`, `utils/bounded.rs` among them).

**`ultragraphdb_common_config`** (1,113 lines, 52 functions). Exceptions: `ConfigApi`'s keys and `AdminService`'s state
under `Rc`, "the runtime is one thread". Open: `config_api`
holds `Scope` beside `ConfigApi`; 6 files without a test file of their name.

**`ultragraphdb_common_config_check`** (1,597 lines, 71 functions). Control plane. Open:
`cross_checks.rs:226,553` take `1..=PARTITIONS_MAX` where a half-open range is the rule, held for the owner's decision.

**`ultragraphdb_common_frontend`** (7,784 lines, 408 functions). Exceptions: six `#[expect(clippy::disallowed_methods)]`
on the shell's clock and the owner's thread; `RoleLink`, `RoleLinks`, `AuthThrottle`, `EndpointCore` state their `Rc`;
`role_link_tasks.rs:42`'s formatted warning, which a scenario reads. Open:
- no plane stated (rule 3);
- `client_session/mod.rs:120` and `owner_thread.rs:110` are unbounded channels, `client_session/mod.rs:139` and
  `client_session_requests.rs:216` unsized `VecDeque`s; each now states its bound (wave 3b), and of the 48 sends dropping
  their `Result` 21 gained a reason, the others covered by their function's doc or a comment;
- eight files hold more than one type: `client_session/mod.rs` (4), `role_link/mod.rs` (3), `asked.rs`,
  `auth_throttle/mod.rs`, `owner/mod.rs`, `role_links.rs`, `role_session.rs`, `owner_thread.rs`;
- `ReportAsked` (`types/report_asked.rs:8`), exported, has four `pub` fields; making them private needs a constructor of
  two `u64`s, which the rule on swappable arguments refuses, so it is the lead's choice;
- `owner_thread.rs:121`'s `"{cpu}"` message, which the supervisor's and storage's scenarios read by its text;
- 29 added files without a test file of their name.

**`ultragraphdb_common_role`** (1,129 lines, 78 functions). Exceptions: `ParkedBytes`' `Rc` of cells, "both sides run on
one core's thread"; `ConnectionBound`'s and `OpenRequests`' `Arc` of atomics, a count across cores; the shell's clocks
(`serve.rs:35`, `joining_role.rs:18`). Open: `Server` stores `welcome: fn(Welcome) -> Welcome` (`utils/serve.rs:54`), a
function pointer kept for later, where every caller but one sets the initial credit alone, the owner's choice of
shape; `traits/session.rs` holds `Answer` beside the trait, `parked_bytes.rs` `Shared`,
`serve.rs` `Event`.

**`ultragraphdb_common_service`** (1,065 lines, 56 functions). Exceptions: `Readiness` and `Stopping`, an `Arc` of
atomics read across cores, control plane (`readiness/mod.rs:16`); `watch_parent`'s thread and the start's clock
(`run_service.rs:275`, `fetch_settings.rs:92`). `cpu_list.rs`' `first..=last` says why it is closed: the kernel's `first-last` names both ends. Open:
`run_service.rs:182`'s `"{cpu}"` message, as in the frontend.

**`ultragraphdb_common_service_config`** (912 lines). Control plane. Open: 12 setting tables
without a test file of their name.

**`ultragraphdb_common_storage`** (3,913 lines, 216 functions). Exceptions: `PartitionCore`'s `Rc<RefCell<…>>`, "shared
by this core's tasks, all on its one thread" (`partition_core/mod.rs:166`); the shell's clock (`partition_core/mod.rs:375`).
Open: no plane stated; `partition_core/mod.rs` holds twelve types and `partition_core_one_frame.rs` two;
`PartitionCore::turn` drains `closes` and `take_remote` drains `remote` with `try_recv` until empty, a close an epoch
and other partitions' answers and registrations at the rate they send, each an `InboxTurn` once the owner picks its
bound. Stated in wave 3b: the bound of the unbounded `storage_session.rs:35` and of the six `VecDeque::new()`
(`partition_core/mod.rs:366`, `partition_core_frames.rs:107`, `retention.rs:34`, `scan_board/mod.rs:34`,
`storage_session.rs:47`), the reason of each of the 19 dropped sends, why `held_catalogs.rs:28,40` and
`scan_board/mod.rs:87,197` range to `u64::MAX` with `..=` (a half-open range cannot name it), and the cancellation of the
10 `async fn`; the inbox drain at `partition_core/mod.rs:399` takes at most `MESSAGES_PER_TURN_MAX` a turn.

**`ultragraphdb_common_telemetry`** (356 lines). `read_to_end` (`utils/http1.rs:98`) is bounded by `MAX_RESPONSE`, its doc
says so.

**`ultragraphdb_common_utils`** (257 lines). Exceptions: the generated tables' `dead_code`; `go_strconv.rs:224`'s
`max_mant_digits` is Go's `maxMantDigits`, kept for the port. Open: `json_value/mod.rs` and `go_strconv.rs` hold
two types each.

**`ultragraphdb_cluster_metadata`** (3,067 lines, 178 functions). Control plane, stated (`metadata_group/mod.rs:38`,
`metadata_state/mod.rs:21`). Open: `metadata_state/mod.rs` holds `TenantCatalog` beside
`MetadataState`; `metadata_group_access.rs:73` checks `MIN_ITERATIONS..=MAX_ITERATIONS`.

**`ultragraphdb_cluster_partition`** (2,603 lines, 157 functions). Data plane, stated in `lib.rs` (wave 3b): it writes
the partition's log and store. Open: `PartitionMailbox::mesh` is bounded by what the partition does, not by a capacity,
as storage's remote channel is (the binaries, below); `bound_transaction.rs` and `partition_message.rs` hold two types
each.

**`ultragraphdb_cluster_placement`** (704 lines, 54 functions, 9 checks). `placement_map/mod.rs:72`'s
`0..=u8::MAX` says a half-open range of `u8` cannot name 255 (wave 3b). Nothing open.

**`ultragraphdb_cluster_sequencer`** (1,242 lines, 82 functions, 8 checks, 4 through `invariant`). Control plane,
stated (`sequencer/mod.rs:40`). Open: `sequencer_snapshot/mod.rs` holds `Part` beside the snapshot.

**`ultragraphdb_tool_bdd`**, **`ultragraphdb_tool_toml`**. Nothing open; `tool_toml` states its control plane
(`lib.rs:15`).

**`ultragraphdb_tool_bench`** (lifted in 7.1). Exceptions: the five `disallowed_methods` on the wall clock and the
benchmark's threads; `ReadCounts`' and `TimedStore`'s atomics, relaxed, "each count is read once the threads are done".
Open: five run files hold several private types (`partitions_run.rs` 3, `apply_run.rs`, `pipeline_run.rs`,
`rollup_run.rs`, `counted_snapshot/mod.rs`); `partitions_run.rs:121`'s `1..=partitions`, held for the owner's decision.

**`ultragraphdb_tool_test`** (2,854 lines). Open: `unfaulted` (`partition_workload/mod.rs:902`) calls itself once per
store of the workload, nesting a closure per call: a recursion whose depth is the workload's partition count (rule 1);
`SimStore` offers only a closure, `unfaulted_reads`, so an iterative form needs a guard on `SimStore`.
`partition_workload/mod.rs` holds six types; `ChangesetWorkload`'s seven counters are `pub` fields, the owner's choice as
`ReportAsked`'s are; `..=` at `partition_workload/mod.rs:138,153`, `crash_harness.rs:164,186`, held for the owner's
decision.

**The role binaries.** Each `main.rs` drops a variable that is not Unicode with `.ok()?`, and says why ("names no
setting"). The 14 `#[expect(disallowed_methods)]` of `compute`, `storage` and the test crates name a real thread or clock.
The supervisor's unbounded wait for a role (`service.rs:449`) is the owner's decision, `notes/group-11.md`, *Waiting*.
`ultragraphdb_tests/src/utils/harness.rs:490`'s `assert_eq!` is a test crate's harness failing its test. The accept
loops of `compute`, `frontend` (two) and `storage`, which logged a failed `accept` and accepted again at once, take
`AcceptPace` (wave 3b, below); every added `async fn` across the binaries states its cancellation, and storage's seven
unbounded channels each state what bounds them (wave 3b). Open:
- storage's lease wants (`wanted`, `service.rs`) have no bound: `RefillBlocks::lease` (`common_storage/src/types/
  refill_blocks.rs:31`) sends a want on every call, and `Sequences::next` (`store_quads/src/types/sequences/mod.rs:61`)
  calls it at each allocation while no block is held ahead, so while `frontend_requests` waits on an unreachable
  frontend the queue grows by one per allocation, a refused write's included. Duplicates are harmless (`lease_ahead`
  leases until `BLOCKS_AHEAD` are held), so one pending want per type would bound it at the number of types: a change to
  `common_storage`, and a decision on whether a want is a flag or a message;
- storage's remote channel and `PartitionMailbox::mesh` are bounded by what the partition does: writes parked at a peer
  each resend a batch a second until their read-write limit. A capacity with a refusal past it needs a decision on what a
  full inbox does with a close, which must not be lost, as against a registration or an answer, which a resend repeats;
- `storage`'s `service.rs:228` logs `"{cpu}"`, its text read by the supervisor's and storage's scenarios.

#### Across the crates

| Rule | Held | Open |
|---|---|---|
| 5, checks per function (the quota replaced, no finding) | placement 9 in 54, sequencer 8 in 82, toml 7, bench 6, tool_test 4 | compute 1 in 208; frontend, storage, partition, metadata, role, service, config and the binaries none |
| 3, a module states its plane | sequencer, metadata, toml, service's `Readiness`, partition (wave 3b) | frontend, compute, storage: the data plane; their 14 unbounded channels and 12 unsized `VecDeque`s each state their bound (wave 3b) |
| Cancellation safety | every added `async fn` (wave 3b: 172 in the common crates, 48 in cluster, tool and the binaries) | `Pipeline::sync` (`transactions_commit/src/types/pipeline/mod.rs:288`) clears `unsynced` before awaiting the log's sync, so a future dropped there leaves a round neither applied nor counted unsynced; every caller runs the turn in the task owning the host, and setting the flag after the await would make it safe |
| Tests mirror `src/` | | 118 added files without a `_tests.rs` of their name, mostly data carriers and setting tables, each crate's suite reaching them through their users |
| Dropped send results | every one states its reason (wave 3b) | |

### Wave 3b: accept loops and inbox drains (owner decision 34)

Two of 12.3's open rule-2 departures are fixed, each test first: the accept loops that spun on a failing `accept`, and the
two inbox drains that took messages for as long as senders kept up. Both rules sit in `ultragraphdb_common_service`, the
crate beneath every role where `listen` and `bind_tcp` already live, so each is written once.

#### The accept loops

The four loops of 12.3 (`compute` `service.rs` `accept_on_core`, `frontend` `accept_roles` and `accept_clients`, `storage`
`accept_connections`) share one shape: accept on a `Listener`, spawn the connection's task, and on a failure log a warning
and accept again at once. `/health`'s accept in `common_service`'s `run_service.rs` has the same shape over a compio
`TcpListener` and the same spin, so it takes the same fix.

- `AcceptPace` (`common_service/src/types/accept_pace.rs`), exported with `ACCEPT_PAUSE_FIRST` (5 ms) and
  `ACCEPT_PAUSE_MAX` (1 s). `failed` counts a failure and gives the pause before the next try, doubling from the first up
  to the maximum; `accepted` ends the run and says how many failures it ended. `AcceptPace::accept(connections, accept)`
  is the shell every loop calls: it returns the next accepted connection, pausing after each failure. The warning is
  written once a run, at its first failure, with the error and the pause; the failures after it are debug events; the
  accept that ends the run logs `connections are accepted again` at info with the count. A lasting `EMFILE` so costs one
  accept a second and one warning. The pauses are Go `net/http`'s accept loop's, 5 ms to 1 s. The warning's text changed;
  no BDD step reads the old ones (`a role connection was not accepted`, `a client connection …`, `a health connection …`,
  each now the field `connections`).
- Each loop is now `let carrier = pace.accept("role", || listener.accept()).await;` followed by its spawn, one `AcceptPace`
  per loop and so per core. The four role loops keep their event-loop comment; `/health`'s gains one.

Tests, `common_service` `tests/types/accept_pace_tests.rs`: the pauses double from the first to the maximum, the failure
reaching it and the one past it each pausing the maximum, the expected pauses read from the constants; an accepted
connection ends the run, and the next failure pauses the first pause again; `accept` tries again after each of two
failures and returns the third try's connection with the run ended. Observed failing with the cap removed (`failed` doubling
without `.min(ACCEPT_PAUSE_MAX)`): the first two failed, no pause reaching the maximum.

#### The inbox drains

- `InboxTurn` (`common_service/src/types/inbox_turn.rs`): what one turn may still take of its inbox, made with a bound;
  `receive(|| inbox.try_recv())` gives the next message while the turn may take one and leaves the inbox unasked once it
  has taken its bound. An empty or closed inbox ends the taking and costs the turn nothing of its bound.
- `PartitionCore::run` (`common_storage/src/types/partition_core/mod.rs`) takes at most `MESSAGES_PER_TURN_MAX` waiting
  messages after the one it waited for: `TURN_BYTES / HEADER_SIZE`, 1,024, the most requests of the smallest size, a header
  alone, that the turn's 256 KiB budget holds. A turn of the smallest requests so takes in what it may build of Scans'
  frames; a larger request counts as one, its bytes bounded by its connection's `max_frame` and held bytes. At one sync a
  turn and the Shard-per-Core note's 2,188 syncs a second, the bound admits two million requests a second.
- `Owner::run` (`common_frontend/src/types/owner/owner_turn.rs`) takes at most `ASKS_PER_TURN_MAX`, 1,024
  (`owner/mod.rs`), for the same reason: an ask stands for a frame an endpoint core read, so the owner's turn and a
  partition's take their inboxes alike, and the asks a turn takes share its one sync of the metadata group, while an
  epoch falling due waits behind at most those 1,024 decisions. `run`'s doc says so.

Tests, `common_service` `tests/types/inbox_turn_tests.rs`: a turn takes an inbox holding its bound whole; of one holding
one more it takes the bound and the next turn the one left; a spent turn asks the inbox no more; an empty inbox costs the
turn nothing. Observed failing with the bound check removed from `receive`: the second and third failed, the turn taking
all five of five and asking a spent inbox again. Each bound's test is the type's, with the bound a fixture; the two
constants are design values with their reasons in their docs, and the loops' wiring is reached by the suites below.

#### Specification

`PROTOCOL.md` says nothing of a role's accept, and §13.14 and the `quad-reads` spec bound a turn's Scans by bytes and
its empty keys by count; a bound on the messages a turn takes leaves both true. Nothing changed there. `AGENTS.md`'s
`ultragraphdb_common_service` row names `AcceptPace` and `InboxTurn`.

#### Run

Bazel: `common_service` unit 56 (49 before, 7 added), `common_frontend` unit 59 and bdd 119 scenarios, `common_storage`
unit 144, `compute_tests:bdd` 88 scenarios, `frontend_tests:bdd` 20, `storage_tests:bdd` 98; all pass. The frontend's and
storage's BDD steps that read log lines pass with the new warning. `cargo clippy --no-deps` over `common_service`,
`common_storage` and the three binaries (all targets) and `common_frontend` (lib) reports nothing with
`-D clippy::disallowed_methods`; `common_frontend`'s test targets fail clippy on `shadow_unrelated` in
`tests/types/owner/owner_{catalog,check,turn}_tests.rs` and `tests/utils/owner_thread_tests.rs`, files another agent
was writing at the time, none of them this fix's; the closing run's workspace clippy is clean. The `:doc` targets of the three common crates build.

#### Open

- `serve_listener` (`ultragraphdb_transport_http_server/src/utils/serve_http.rs:121`), the admin endpoint's and the config
  service's accept loop, spins on a failing `accept` the same way. It sits beneath `common_service`, so it cannot take
  `AcceptPace` from there; pacing it means moving the pace down to `ultragraphdb_protocol_transport_compio`, beside
  `Listener`, or a copy in the HTTP crate: the lead's choice of level.
- `PartitionCore::turn` drains `closes` and `take_remote` drains `remote` with `try_recv` until empty, as the inbox did.
  `closes` holds at most a close an epoch; `remote` takes other partitions' answers and registrations, at the rate they
  send. Each takes an `InboxTurn` in two lines once the lead picks their bounds.

### Wave 3b: the documentation sweep (owner decision 35)

Four agents swept the change's `src/` lines, a line `git blame` gives to `08ff8f26` left out, by folder. Every edit is a
comment, a doc section, an attribute, two loops written half-open or a compile-time assert; none changes behaviour, so no
test was added. In all: 1,065 `#[must_use]`, 83 `# Errors` sections, 260 casts given their reason, 270 `async fn`
cancellation notes, 33 queue bounds.

| Part | Crates | `#[must_use]` | `# Errors` | Cast reasons | Cancellation | Queue bounds |
|---|---|---|---|---|---|---|
| a | protocol, transport, client, traits, error (25) | 615 | 34 | 121 | 45 | 1 |
| b | core, store, transactions (19) | 306 | 37 | 50 (54 with 4 stated already) | 5 | 2 |
| c | `ultragraphdb_common_*` (11) | 75 | 0 | 38 | 172 | 17 |
| d | cluster, tool, `ultragraphdb/` (20) | 69 | 12 | 51 | 48 | 13 |

#### Part a: protocol, transport, client, traits, error

**Scope.** The 25 crates under `ultragraphdb_protocol/`, `ultragraphdb_transport/`, `ultragraphdb_client/`,
`ultragraphdb_traits/` and `ultragraphdb_error/`, over every `src/` line `git blame` gives to a commit of
`1f4b67a3..HEAD` other than `08ff8f26`, or to the working tree: the code the change added or changed. Older code is left
as it is. `traits_collator` and `traits_catalog` held nothing to sweep. No behaviour changed, so no test was added. Under
Bazel, the 40 non-integration test targets of the five folders pass. `cargo clippy --no-deps --all-features -D warnings`
is clean on the 25 libraries. `rustfmt --check` passes on every changed source file. `cargo doc --all-features` passes
under `-D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links`, and `cargo check --workspace
--all-targets` finds no caller that the new `#[must_use]` breaks.

**How each kind was found.**

| Kind | Check |
|---|---|
| `#[must_use]` | `cargo clippy -p <crate> --no-deps [--features test-utils] -- -W clippy::must_use_candidate -W clippy::return_self_not_must_use`, the lints passed on the command line only and each hit blamed. The scan was run again after the sweep and found no hit in scope |
| `# Errors` | the same run with `-W clippy::missing_errors_doc` |
| casts | ` as (u\|i)(8..128\|size)` and ` as f32/f64` on lines in scope. The held-back `cast_possible_truncation` and `cast_sign_loss`, passed on the command line, separate the narrowing casts from the widening ones |
| cancellation | every `async fn` on lines in scope, read against its awaits |
| dropped results | `drop(` on lines in scope |
| queues | `VecDeque`, `channel(`, `unbounded` and `mpsc` on lines in scope |

**Per crate.**

| Crate | `#[must_use]` | `# Errors` | Casts given a reason | Cancellation stated |
|---|---|---|---|---|
| `error_cluster` | 34 | | | |
| `error_protocol` | 30 | | | |
| `error_log` | 13 | | | |
| `error_store` | 10 | | | |
| `error_json`, `_num`, `_options`, `_invariant`, `_role` | 5, 3, 3, 1, 1 | | | |
| `traits_blocks` | 8 | | | |
| `traits_index` | 5 | 3, `find` of `CacheType`, `LocalCache` and `Txn` | | |
| `traits_log` | 2 | 1, `OrderedLog::append` | | |
| `protocol_scram` | 8 | 1, `ScramServer::name_in` | | |
| `protocol_transport` | 14 | 1, `Outbound::open_with_credit` | 1 | 1, `BufferPool::acquire_waiting` |
| `protocol_transport_compio` | | | | 22 |
| `protocol_wire` | 467 | 15 | 111 | |
| `transport_tls` | 8 | | | |
| `transport_http_client` | | | | 3 |
| `transport_http_server` | | | | 4 |
| `client_core` | 3 | 5 | 8 | |
| `client_compio` | | 4 | | 7 |
| `client_tokio` | | 4 | 1 | 8 |

- **`#[must_use]`, 615.** Each goes on the line above its `fn`, as the crates already place it. Most are error
  constructors, getters, constructors and `with_*` builders; three are in `client_core`'s `TestServer` and three in
  `traits_blocks`' `TestBlockSource`. `clippy::must_use_candidate` passes over generic constructors such as
  `TlsDial::new`, so those carry none, by the lint's measure.
- **`# Errors`, 34.** Each lists the error constructors that function returns and when it returns each one. Where a
  function's summary already listed its refusals, as the wire's `check_request`, `from_client_frame`, `stamp_stream`,
  `ObjectRecords::read` and `ResumeKeyView::read` did, the refusals moved under the heading and the summary kept the
  rest. The client shells' `begin_with`, `direct_write` and `conditional_write` name the connection's errors, the core's
  refusals and the server's answer; `in_doubt`, `outcome_unknown` and `connection_lost` say what a write lost on its way
  becomes.
- **Casts, 121.** A widening cast carries a trailing `// lossless: a `usize` is at most 64 bits` (from `usize` to `u64`
  or `u128`) or `… at least 32 bits` (from `u32` to `usize`), the wording `sim_network` already used. The comment goes
  on the line above when the code ends in `{`, which rustfmt would move, and once above a run of such lines as "The
  casts are lossless". `protocol_wire/src/utils/constants.rs` now holds `const _: () = assert!(usize::BITS >= 32);` and
  `assert!(usize::BITS <= 64)`, so the width each comment relies on is the compiler's to check. The narrowing casts are
  stated one by one:
  - `ColumnType::number`: `i` is below the 13 types of `ALL`.
  - `server_connection`'s `MAX_FRAME_FLOOR as u32`: 2^19.
  - `TestServer::read`'s `want as u32`: a header's 256 bytes or a frame's `size`, a `u32`.
  - The test server's resume byte `i as u8`: truncated on purpose; a client takes a resume key as opaque bytes.
  - `CommitOperationLayout::check`'s `self.delta as u64`: now `self.delta.cast_unsigned()`, the same two's complement
    bits.
- **Cancellation, 45.** The sentence follows the convention of `common_service`: "A future dropped at its X leaves Y".
  - `protocol_transport_compio`: what the TLS stream and its halves lose at each read and write; what `Carrier` and
    `Listener` leave at connect, read, write, shutdown, bind and accept.
  - `transport_http_client`: a request's connection is closed, with the request sent in whole, in part or not at all.
  - `transport_http_server`: `serve_http` closes its stream and drops the service's call where it stood; also `drain`,
    `read_request` and `write_response`.
  - Client shells: `TlsDial::connect`, the TLS handshakes, compio's `read_by`, `within` and `take_arrived`, and tokio's
    `take_arrived`. A request future dropped after it sent its request leaves that request with the core, whose answer
    no later call takes. `fill` and `serve_listener` already stated their cancellation.
- **Dropped results.** 11 `drop(...)` of a `Result` sit on lines in scope: the compio `Connection`'s shutdown and
  refused Welcome, the TLS handshakes' alert writes, the tokio shell's shutdown and the test server's refusals. Each
  already carries its reason, and none is a channel send: these crates hold no channel.
- **Queues.** `ClientCore::kept_reads` now states its bound. Each frame kept holds one of the client's frame buffers
  and counts against `ClientConfig::buffers` in `reserve_buffers`. The other `VecDeque`s added are test utilities:
  `TestPipe`'s lanes, `TestServer`'s answers and `TestBlockSource`'s scripted blocks.

**Defects found.**

1. `CompioTlsStream`'s type note said that a dropped read or write left "records not yet written queued" in the
   session. The records `transmit` had already taken for the dropped write are not in the session; they reach the
   stream in whole, in part or not at all. The note now says which records stay queued and which are lost
   (`compio_tls_stream/mod.rs:24-27`).
2. `serve_http`'s public documentation linked the private `drain`, which fails `cargo doc` under
   `-D rustdoc::private_intra_doc_links`, and its last line ran past 120 columns. It now names the drain in words.
3. **Open, needs a design.** `ClientCore`'s `pending` map keeps the answer of every request until `take_reply`. A
   shell request future dropped after its request was sent takes its `Pending` with it, and since `Pending` is `Copy`,
   the core cannot see the drop. The entry is never removed, so a caller that wraps requests in its own timeouts grows
   the map by one entry per dropped future. The map predates the change (`daadf494`, group 10); the shells' new notes
   state this. A fix needs one of two designs: a held flag, as `ResultStream`'s `read_to_end` uses, or pruning when the
   request's stream ends.

**Other agents' lines touched.** Two comments in the working tree ran past 120 columns and were rewrapped with no
change of words: the reply loop's `// Ends:` in `transport_http_client/src/types/http_client.rs` (wave 3) and "A
connection with no deadline …" in `client_compio/src/types/client/mod.rs`. A rewrap of
`protocol_transport_compio/tests/utils/probe.rs`'s module comment was undone at once.

**Open, with what decides each.**

- **Half-open ranges, 17 `..=` on lines in scope.**
  - `contains` checks over closed intervals: `client_handshake/mod.rs:68`, `term_table_view/mod.rs:45`,
    `compute_read_view_body_view.rs:74`, `challenge_view_body_view.rs:32`, `result_view/mod.rs:117`,
    `report_view_body_view.rs:48`, `commit_operation_layout.rs:78`, `changes.rs:434` and `:436`, and
    `parse_reply.rs:49`.
  - Match arms: `write_set_record_view/mod.rs:403` and `:407`, and `operation.rs:35`.
  - `entries_sequence.rs:177` and `:198`.
  - The public constant `ROLE_COMMANDS: RangeInclusive<u16>`, which other crates call `contains` on.
  - `serve_http.rs:81`'s loop of one turn more than a length.

  Each matches a closed interval as `PROTOCOL.md` writes it. Rewritten as `start..end + 1`, each keeps its behaviour
  but reads less like the specification, and `ROLE_COMMANDS` would change type in every crate that names it. Whether
  rule *Off-by-one* reaches intervals the specification states closed is the owner's call.
- **`serve_listener`.** It spawns one task per accepted connection with no count, each bounded in time by `HttpLimits`
  alone, and a failing `accept` loops at once. A bound on connections in flight and a pause between failed accepts each
  change behaviour, and each needs a number: either the owner sets one, or `common_service`'s `AcceptPace` is adopted.
- **`Copy` types above 16 bytes in `protocol_wire`** (12.3's open item 1). Dropping `Copy` changes every caller that
  copies.
- **Rule 3's planes, stated in no module of these crates but `client_core`, `transport_http_client`,
  `transport_http_server` and `transport_tls`** (12.3's open item 9). Stating a plane is a line per module, over
  several hundred modules, and needs each module's plane decided.
- **Two types in one file.** `transport_http_client`'s `http_reply.rs` holds `HttpReply` and `ParsedReply`;
  `transport_http_server`'s `parsed_head.rs` holds `HttpHead` and `ParsedHead`. Both stay, as decision 35 allows.

#### Part b: core, store, transactions

**Scope.** The 19 crates under `ultragraphdb_core/`, `ultragraphdb_store/` and `ultragraphdb_transactions/`, over the
`src/` lines `git diff -U0 1f4b67a3` gives, a line `git blame` gives to `08ff8f26` left out. The hits came from
`cargo clippy -p <crate> -- -W clippy::must_use_candidate -W clippy::missing_errors_doc` (JSON, filtered to added lines),
the 12.3 review's lists, and a scan of the added lines for `as` casts, `async fn`, `drop(` of a send, `VecDeque` and
unbounded channels; neither lint joined a table. Every change is a comment, a doc section or an attribute, except two
ranges written half-open and one compile-time assertion, none of which changes behaviour.

**Checked.** `cargo clippy -p <crate> --all-targets -- -D warnings` clean on 18 crates and on `store_apply`'s library
(its test target fails on two `shadow_unrelated` hits in `changeset_core_rebuild_tests.rs:862`, `:892`, another agent's
edit in progress, clean at the closing run). `cargo clippy --workspace --all-targets --keep-going` shows no `unused_must_use` or
`let_underscore_must_use` anywhere, so no caller needed a fix. `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps` clean on
the 16 crates changed. `bazel test //ultragraphdb_core/... //ultragraphdb_store/... //ultragraphdb_transactions/...`:
57 tests pass. The 19 `:doc` targets build. A re-run of the lint scan leaves one hit on an added line, below.

##### Per crate: what was swept

| Crate | `#[must_use]` | `# Errors` | Casts given a reason | Other |
|---|---|---|---|---|
| `core_algo`, `core_codec`, `core_keys` | 0 | 0 | 0 | none found |
| `core_catalog` | 17 | 2 | 4 | the hierarchy walk `0..=len` written `0..len + 1` |
| `core_ids` | 10 | 0 | 0 | |
| `core_mapping` | 16 | 1 | 2 | Lemire's two casts: the low half kept on purpose, the high half lossless |
| `core_terms` | 8 | 0 | 1 | `const _: () = assert!(ID_LEN <= 255)` beside the constant the `u8` cast reads |
| `core_text` | 6 | 0 | 2 | `append_next`'s recursion bound (one level); the six loops of `parse_u_attributes` and `parse_u_keys` |
| `core_tok` | 3 | 0 | 5 (1 stated already) | `transition`'s two binary searches and their sign-preserving casts |
| `core_types` | 2 | 0 | 16 | `expand_children`'s recursion (at most 3 deep); the loops of `big_float_format.rs`, `first_non_finite`, `quick_select` |
| `store_apply` | 46 | 9 | 1 (stated already) | |
| `store_hnsw` | 16 | 0 | 0 | |
| `store_kv` | 0 | 3 | 0 | |
| `store_posting` | 32 | 3 | 1 | |
| `store_quads` | 45 | 18 | 0 | |
| `store_sim` | 5 | 1 | 0 | |
| `transactions_commit` | 74 | 0 | 12 (2 stated already) | 3 `async fn` cancellations (`Replay::run`, `run_in`, `run_with_catalog`); 2 `VecDeque` bounds (`held`, `registrations`) |
| `transactions_log` | 2 | 0 | 5 | 2 `async fn` cancellations (`trim_through`, `checkpoint_at`); the chain walk `0..=segments` written `0..segments + 1` |
| `transactions_txn` | 24 | 0 | 5 | |
| **Total** | **306** | **37** | **54 (50 new)** | 5 cancellations, 2 queue bounds, 2 ranges |

No `drop(...)` of a send's result and no unbounded channel is on an added line of these crates.

- **Casts.** Each reason says why the cast cannot lose: a `usize` at most 64 bits (`usize` to `u64`), a `usize` holding
  32 bits on every target the repository builds for (`u32` to `usize`), a bound read before it (`getnum`'s two digits,
  `parse_signed_offset`'s 23 hours, a round's `bounds.bytes()` and `bounds.transactions()`, a frame's `MAX_FRAME_SIZE`),
  a value at least 0 (`select.rs`, `segment_words.rs`), or a truncation that is Go's own (`full_tag`'s `u16`, the float
  conversions of `conversion.rs`, which round or truncate as Go's `float64(int64)` and `int64(float64)` do).
- **Cancellation.** `Replay::run_with_catalog` waits only on the log's read: a future dropped there leaves every round
  replayed before it applied whole with its session entries and applied position, a build such a round started going on
  in the applier; `run` and `run_in` refer to it. `Wal::checkpoint_at` dropped at the superblock's write leaves the
  superblock in memory unchanged and on disk as a crash at that step does; dropped while segments are recycled, it
  leaves the checkpoint moved, the segment in hand on disk for the next open to recycle, and the rest for the next trim.
  `trim_through` leaves the same, the log not poisoned.
- **Recursion the review listed.** `shapeindex`'s `update_edges` (depth 30) and `geometry_wkb`'s `write_one` (members
  hold no members) already state their bounds; `expand_children` and `append_next` now do.

##### Defects fixed

1. **`core_terms` failed `cargo clippy`.** A concurrent change to `term_view/term_view_literal.rs:72` bound `datatype`
   inside the arm destructuring `datatype` (`shadow_unrelated`, deny), which stopped `cargo clippy` for every crate
   depending on `core_terms`. The inner binding is now `named`; the crate's tests pass.
2. **Two unresolved doc links** in `transactions_commit/src/types/commit_step/mod.rs`, failing `cargo doc` at
   `-D warnings`: `` [`Sequencer`] `` now names `ultragraphdb_cluster_sequencer::Sequencer`, and `` [`Input::Register`] ``
   the variant's name, `Input::Registration`.

##### Open, with what decides each

1. **The commit queue is unbounded in production.** `CommitStep::with_queue` bounds `waiting`, `deferred` and `held`
   together, and the pipeline's `registrations` with them, but defaults to `usize::MAX`, and no caller outside the
   crate's tests calls `with_queue` or `Pipeline::with_queue`; its doc says "the node sets it". The bound needs a value
   (`PROTOCOL.md` §11 names the Busy refusal for a full queue, not its size): a design decision, then the storage role
   setting it and a test at the bound and past it.
2. **`ChangesetApplier::over` returns `Result` and never fails.** Its `# Errors` says so. Returning `Self` changes 10
   call sites in 9 files across six crates, several of them under other agents' edits.
3. **`EncodedTerm::result_view`** (`core_terms/src/types/encoded_term/encoded_term_view.rs:34`), added by the
   concurrent `core_terms` change, has no `# Errors` heading; it names its refusals in its summary, as `view` does.
4. **Public fields on `store_quads`' `Registered`** (`referrer_set/mod.rs`): the owner's choice.
5. **Atomic orderings** with no stated reason: `store_sim`'s `SimClock` (`SeqCst`, each value read alone) and
   `store_kv`'s `GcSettings` (Acquire, Release, AcqRel). Whether a weaker ordering suffices is behaviour.
6. **`Copy` types above 16 bytes** in these crates: `store_apply`'s `ReadCounts` and `MarkedReference`, `store_quads`'
   `ReferrerMember`, `transactions_commit`'s `EpochStamp`, `core_types`' `CivilTime`. Dropping `Copy` changes callers.
7. **Two types in one module**, which decision 35 lets stay: `core_mapping/src/types/change_ref.rs`,
   `store_quads/src/types/referrer_set/mod.rs`, `transactions_txn/src/types/frame_changes.rs`.
8. **The 64-bit premise is a comment.** Several cast reasons rest on `usize` holding 32 or 64 bits on the targets built;
   a `const _: () = assert!(usize::BITS == 64);` in each crate would make that a compile-time check.

#### Part c: `ultragraphdb_common/`

The sweep covers the lines `git blame 1f4b67a3..` gives to the change in the `src/` of the eleven `ultragraphdb_common_*`
crates, `08ff8f26`'s lines left out, the working tree's included. Its lists are the 12.3 review's (second half), each
confirmed by a scan of those lines: `cargo clippy -p <crate> -- -W clippy::must_use_candidate -W clippy::missing_errors_doc
-W clippy::return_self_not_must_use` on the command line, no lint table changed, and a pattern scan for `as` casts,
`async fn`, `drop(…send…)`, `unbounded` and `VecDeque::new()`, each hit read in place. `accept_pace.rs` and
`inbox_turn.rs` in `common_service`, and `utils_test/owner_run.rs` and `tests/types/owner/` in `common_frontend`, are
another agent's work of this wave and were left to it.

Every change is a comment, a doc line or a `#[must_use]`; no behaviour changed, so no test was added. A `#[must_use]`
failed no caller: `cargo check --workspace --all-targets` is clean, and `bazel build` of the 43 library, binary and test
targets depending on the eight changed crates passes. Each crate's `cargo clippy --lib --bins -- -D warnings` and
`cargo doc --no-deps` under `-D warnings` are clean, rustfmt changes none of its `src/` files, and its Bazel targets pass:
`common_compute` unit; `common_config` unit and bdd; `common_frontend` unit and bdd; `common_role` unit and bdd;
`common_service` unit; `common_service_config` unit and bdd; `common_storage` unit; `common_telemetry` unit, bdd and
doc test, 14 targets in all.

A cast's reason follows the repository's wording, a trailing `// lossless: a `usize` is at most 64 bits` or the
constant's value; an `async fn`'s note says what a future dropped at each `.await` leaves. Where every async method of a
type leaves the same thing, the type's documentation says it once, as the client shells' `Client` does, and the methods
that differ say their own.

| Crate | `#[must_use]` | `# Errors` | Casts | `async fn` | Dropped sends | Queues |
|---|---|---|---|---|---|---|
| `common_compute` | 0 | 0 | 8 | 43 | 6, all with reasons | 5 |
| `common_config` | 0 | 0 | 1 | 16 | 7, one reason for the owner's match | 0 |
| `common_config_check` | 0 | 0 | 0 | 0 | 0 | 0 |
| `common_frontend` | 3 | 0 | 18 | 72 | 48, 21 given reasons | 6 |
| `common_role` | 0 | 0 | 4 | 21 | 0 | 0 |
| `common_service` | 7 | 0 | 0 | 9 | 0 | 0 |
| `common_service_config` | 54 | 0 | 0 | 0 | 0 | 0 |
| `common_storage` | 6 | 0 | 7 | 10 | 19, 1 given a reason | 6 |
| `common_telemetry` | 5 | 0 | 0 | 1 | 0 | 0 |
| `common_utils` | 0 | 0 | 1, its reason stated already | 0 | 0 | 0 |
| `common_version` | 0 | 0 | 0 | 0 | 0 | 0 |

Every exported `pub fn` the change added that returns `Result` holds a `# Errors` section already; `clippy::
missing_errors_doc` finds none missing on the added lines.

##### Per crate

**`ultragraphdb_common_compute`.** Casts: `members.address(… as usize)` (`compute_core/mod.rs`), the three `u32`
offsets of `Page::row_at`, `remaining as usize` (`partition_rows_fill.rs`), `left as usize`
(`partition_rows_terms.rs`), and `ResultSink`'s `max_frame` both ways. Cancellation: a read is one task, which a Cancel or
its connection's end drops; every Scan it holds is cancelled as its `ScanStream` drops and its pins with them, which
`run_read` says and each function of the read repeats for what it holds. Three say more: `ScanStream::next` dropped at
its wait leaves the frame to the next call, which grants one more credit, so the partition may send one frame ahead;
`PartitionRows::fill` and `next_frame` dropped at a frame drop the Scan under way, taken out to be read, so a partition
whose fill was dropped is read no further and `merge` ends the read; `ComputeCore::link` and `dial_for_all` leave a
dropped dial to its `DialUnderWay`, whose waiters look again. Queues: `Link`'s `out` (per stream a request, a Credit per
frame taken and a Cancel), `OpenStream::frames` (Entries frames by `credit`, any other command a stream's one answer or
end, as the protocol has the peer send it), `ComputeSession`'s pushes (one batch per read at a time, the connection's
`max_streams`) and each read's `room` (at most one waits), and `PartitionRows::pending` (the graph's one range, or one per
predicate its listing names).

**`ultragraphdb_common_config`.** Cast: `TargetState::latest`. Dropped sends: `store_owner`'s seven replies take one
comment over the match, beside its doc's "a caller gone before the answer loses nothing but the answer". Cancellation:
`ConfigApi` states it once for its nine handlers, `ConfigHandle::ask` for itself; `ConfigStore`'s writes state on their
`impl` that a write dropped at its sync leaves the round appended and the state without it, so the store is not used
again and its next start recovers the round if durable, and `store_owner`'s doc that the owner awaits each write to its
end, which every caller does today.

**`ultragraphdb_common_config_check`.** Nothing in scope: no cast, `async fn` or send on the added lines, and every
exported function holds its `#[must_use]` and `# Errors`.

**`ultragraphdb_common_frontend`.** `#[must_use]`: `AuthThrottle::name_refused`, `RoleShared::new`, `RoleSession::new`.
Casts, 18: the two `detail() >> 32` (a `u64`'s high half), the session spread's `len() as u64` and its index below
`len()`, a pattern's column count, the `usize`-to-`u64` of frame lengths, indexes and `CATALOG_CAP`, three frame
constants by their value, and `OwnerClock::read`'s two `as_micros() as u64`, whose `u128` of microseconds fits a `u64`
for 584,000 years. Dropped sends: the owner's replies in `asked_lease`,
`asked_register`, `next_close`, `declare`, `drop_catalog`, `record_default` and `report` take their function's doc, and
`answer_closes`' error and `start_owner`'s failed recovery a comment; `Owner::take`'s doc and the other comments covered
the rest. Cancellation: `ClientSession` states it once for its seventeen request functions, each awaiting the owner alone
and awaited to its end by its connection, so only the runtime's end drops one; `OwnerHandle` once for its 22 calls, beside
`ask`'s own; `RoleSession` once for its nine handlers. The owner's `recover`, `create_*_once`, `first_start`, `run`,
`turn`, `answer_durable`, `learn_missed` and `store_snapshots`, `OwnerThread::stopped`, `start_owner` and `open`, the role
link's four tasks, `connect_back`, `authenticating` and the test doubles' four each state their own. Queues: the
session's `events` and `out` (a read's frames metered by the client's credit, every other stream's its answers and
end), `ClientTransaction::queue` and `RoleLink`'s `outgoing` (each frame counted in its connection's bytes on their way),
the owner's inbox (as `OwnerHandle` says) and `RoleDouble`'s pushes.

**`ultragraphdb_common_role`.** Casts: `ConnectionBound::enter`, `RoleClient::dial` and `dial_taking`, `Server`'s
`with_max_frame`. Cancellation: `RoleClient` states it once for its nine, a future dropped at an `.await` cutting the frame
in hand so the client is given up; `RoleReader` and `RoleWriter` likewise; `ParkedBytes::room` leaves its waker, which the
next release wakes for nothing; `finish_open`, `retry_until`, `serve_connection` and its four steps state their own.

**`ultragraphdb_common_service`.** `#[must_use]`: `Readiness::is_ready` and `is_listening`, `RoleStart`'s four getters,
`exit_status`. Cancellation: `AdminService::call` (the load is set only once its fetch answered), `HealthService::call`,
`fetch_settings`, `fetch_once`, `bind_tcp`, `listen`, `serve_service`, `accept`, `serve_admin`. `cpu_list.rs`'s
`first..=last` says why it is closed: the kernel's `first-last` names both ends.

**`ultragraphdb_common_service_config`.** `#[must_use]` on the 54 getters of the setting tables and the error's two
constructors.

**`ultragraphdb_common_storage`.** `#[must_use]`: `PartitionCore::new`, five `ScanBoard` getters. Casts: three in
`partition_core_reads.rs`, `StorageSession`'s `max_frame`, three `*limit as u64` in `error_reply.rs`. A dropped send's
reason: the `Answer::Open` of a request behind a parked one. Cancellation: `PartitionCore::run` dropped drops the core
without its stop (a request waiting for its first answer is refused by its session, a stream already open hears nothing
until its connection closes, a round under sync is neither applied nor handed out, as `PartitionHost::turn` says, and the
next start recovers from the log); `turn`, `yield_once`, `CoreEnds::finish`, the session's `answer` and `next_push`, and
the four of `partition_core_remote.rs` state their own. Queues: `StorageSession`'s pushes (one frame a stream before it
left) and what it sends its core (one request at a time and a note per stream), stated on the session and on
`CoreChannels`; `PartitionCore::versions`, `Parked::behind`, `Retention::closes`, `ScanBoard::turns` and the session's
`looked_at`. The four `..=` to `u64::MAX` in `held_catalogs.rs` and `scan_board/mod.rs` say a half-open range cannot
name that end.

**`ultragraphdb_common_telemetry`.** `#[must_use]`: `TelemetryError`'s three constructors, `OtlpHttpClient::new` and
`with_tls`. `send_bytes` holds no `.await`: its exchange blocks within the first poll.

##### Defects found

- Two public docs linked a private item, which `cargo doc` under `-D warnings` refuses: `BIND_RETRIES`
  (`common_service/src/utils/listen.rs:21`) to `BIND_RETRY`, and `AuthThrottle` (`common_frontend/src/types/
  auth_throttle/mod.rs:36`) to `NAME_FAILURES_MAX`. Each names the constant in code font now.
- No defect of behaviour. Two preconditions the cancellation reading found are now stated where they live: a
  `ConfigStore` write dropped at its sync leaves the round appended and not applied, and a `PartitionRows` whose fill was
  dropped has lost its Scan under way; every caller today runs each to its end or ends the read with it.

##### Open, with why

- `ReportAsked`'s four `pub` fields (`common_frontend/src/types/report_asked.rs:8`): private fields need a constructor
  of two `u64`s, which the rule on swappable arguments refuses; the owner's choice.
- `Server`'s `welcome: fn(Welcome) -> Welcome` (`common_role/src/utils/serve.rs`), a function pointer: replacing it
  changes `Server`'s signature for its callers in the binaries; the owner's choice of shape.
- `cross_checks.rs`' `1..=PARTITIONS_MAX` (`common_config_check`): the owner holds the review's ranges open for a decision.
- The `"{cpu}"` messages of `owner_thread.rs` and `run_service.rs`: a scenario reads each by its text.
- `common_storage`'s other channels, `asks` (`FrontendAsk`), `RefillBlocks`' `wanted`, and the `std::sync::mpsc`
  `own`, `remote` and `closes`, are created in the storage binary; what bounds `asks` while the frontend link does not
  read is that link task's, so the statement belongs with the binary's sweep.
- `Link`'s frames of commands other than Entries are bounded by the peer keeping the protocol (one answer or end a
  stream); a count on the compute side is a design change.
- Multi-type modules stay (`client_session/mod.rs`, `partition_core/mod.rs`, `link.rs`, `run_read.rs` among them), and
  the files without a `_tests.rs` of their name are outside this sweep.
- One `bazel test //ultragraphdb_common/ultragraphdb_common_storage:ultragraphdb_common_storage_tests` failed while
  another agent was writing that crate's test files (`partition_core_frames_tests.rs` changed 20:20:35, the run 20:20);
  its log was overwritten by another run before it was read, and the two runs since pass. The cause is not established.

#### Part d: `ultragraphdb_cluster/`, `ultragraphdb_tool/`, `ultragraphdb/`

Owner decision 35. The sweep covers the lines `git diff -U0 1f4b67a3` adds to the `src/` of the twenty crates under the
three folders, less the lines `git blame` gives to the tenant rename (`08ff8f26`). The lists of the 12.3 review were the
start; each item was then found again by a script over those lines: `clippy::must_use_candidate` and
`clippy::missing_errors_doc` passed to `cargo clippy -p` on the command line, no lint table touched, their hits kept where
they fall on an added line; `as` into a numeric type; `async fn` whose doc names no drop or cancel; `drop(` of a send;
`unbounded`, `channel()` and `VecDeque`. The five `_tests` crates of the role binaries add nothing under `src/`.

No behaviour changed: every edit is an attribute or a comment. No `as` became a `try_from`, so no test was added.

##### Per crate

| Crate | `#[must_use]` | `# Errors` | `as` reasons | Cancellation | Queue bounds |
|---|---|---|---|---|---|
| `cluster_placement` | 0 | 2 | 11 | 0 | 0 |
| `cluster_sequencer` | 0 | 0 | 8 | 5 | 0, stated already |
| `cluster_metadata` | 0 | 0 | 14 | 4 | 5 |
| `cluster_partition` | 41 | 0 | 0 | 1 | 1 |
| `tool_bdd` | 0 | 0 | 0 | 0 | 0 |
| `tool_toml` | 3 | 0 | 0 | 0 | 0 |
| `tool_bench` | 18 | 0 | 14 | 1 | 0 |
| `tool_test` | 7 | 0 | 1 | 0 | 0 |
| `ultragraphdb_compute` | 0 | 0 | 1 | 7 | 0 |
| `ultragraphdb_config` | 0 | 0 | 0 | 1 | 0 |
| `ultragraphdb_config_cli` | 0 | 0 | 0 | 1 | 0 |
| `ultragraphdb_frontend` | 0 | 0 | 0 | 3 | 0 |
| `ultragraphdb_storage` | 0 | 0 | 1 | 17 | 7 |
| `ultragraphdb_supervisor` | 0 | 0 | 0 | 8 | 0 |
| `ultragraphdb_tests` | 0 | 10 | 1 | 0 | 0 |
| **Total** | **69** | **12** | **51** | **48** | **13** |

- **`#[must_use]`.** Every hit of `must_use_candidate` on an added line: `cluster_partition`'s error constructors and the
  getters of `Answer`, `BoundRead`, `Fanout`, `Home`, `Partition`, `PartitionMailbox`, `PartitionOutcome`, `Reference`
  and `ReverseQuery` (38, and the three `tenant` getters of `BoundRead`, `Reference` and `ReverseQuery`, which the rename
  touched last in files the change added); `tool_toml`'s and `tool_bench`'s error constructors; `tool_bench`'s
  `ApplyArgs`, `PartitionArgs` and `PartitionShape` getters; `tool_test`'s `ChangesetWorkload`, `PartitionWorkload`,
  `Run` and `TestDir`. No caller left a value unused, so no call site changed. The binaries' items are not exported and
  draw no hit.
- **`# Errors`.** `ultragraphdb_tests`' harness, the ten functions the review listed (`config_folder`, `config_folder_at`,
  `start_config_service`, `start_config_service_at`, `storage_holding`, `frontend_holding`, `holding`, `until_answering`,
  `members_at`, `start_double`), and `cluster_placement`'s crate-private `Layout::from_assignments` and
  `Layout::check_limits`.
- **`as`.** Each says why it loses nothing: a `u32` into a `usize`, which every target the node builds for holds whole; a
  `usize` length into a `u64`; a constant cast at its value (256, 1,024, 2^18, 2^20). `tool_bench`'s `as f64` say a
  report's figure is exact below 2^53 and rounded past it; `Summary::of`'s float-to-`usize` rank that `q` lies in 0 to 1;
  storage's `first_ticket` that microseconds since 1970 fit a `u64` until the year 586,912.
- **Cancellation.** Every added `async fn` says what a future dropped at each await leaves. Those a thread's `block_on`
  runs to their end (`ultragraphdb_config`'s `serve`, the supervisor's `supervise`, storage's `serve_core`,
  `tool_bench`'s `write_loop`) say so, and what a drop would leave.
- **Dropped sends.** Each `drop(….send(…))` in the added lines already said why the receiver gone is fine: 3 in
  `cluster_partition`, 1 in `tool_bench`, 8 in `ultragraphdb_storage`.
- **Queues.** `cluster_metadata`'s state names each `VecDeque`'s bound (`PLACEMENT_PENDING_MAX`, the settings'
  `sessions_max`, `EVICTIONS_PENDING_MAX`, `RETAINED_MAX`) and `LeasedBlocks` its `HELD_MAX`; `cluster_sequencer`'s two
  were stated already ("bounded by the settings"). `PartitionMailbox::mesh` and storage's seven channels say what bounds
  each: a mailbox's inbox by the writes parked at its peers, a batch a second each until their limit, the answers and the
  closes; storage's connection inbox by each connection's credit and `held_bytes`; its asks by epochs and catalog events;
  its closes by one per epoch; its answers by the registrations asked; its discards by one per raising turn.
- **Other review items.** `cluster_partition` states its plane in `lib.rs` (data plane: it writes the partition's log and
  store); `placement_map/mod.rs`'s `0..=u8::MAX` says a half-open range of `u8` cannot name 255.

##### Open, for the lead

1. **Storage's lease wants have no bound** (`ultragraphdb_storage/src/service.rs`, the `wanted` channel, now commented).
   `RefillBlocks::lease` (`common_storage/src/types/refill_blocks.rs:31`) sends a want on every call, and
   `Sequences::next` (`store_quads/src/types/sequences/mod.rs:61`) calls it at each allocation while no block is held
   ahead, so while `frontend_requests` waits on an unreachable frontend the queue grows by one per allocation, a refused
   write's included. Duplicates are harmless (`lease_ahead` leases until `BLOCKS_AHEAD` are held), so one pending want
   per type would bound it at the number of types; that is a change to `common_storage` and a decision on whether a want
   is a flag or a message.
2. **`PartitionMailbox::mesh` and storage's remote channel** are bounded by what the partition does, not by a capacity:
   writes parked at a peer each resend a batch a second until their read-write limit. A capacity with a refusal past it
   needs a decision on what a full inbox does with a close, which must not be lost, as against a registration or an
   answer, which a resend repeats.
3. **`Pipeline::sync` is not safe to drop** (`transactions_commit/src/types/pipeline/mod.rs:288`): it clears `unsynced`
   before awaiting the log's sync, so a future dropped there leaves a round neither applied nor counted unsynced.
   `PartitionHost::turn`'s new note says so; every caller today runs the turn in the task that owns the host, so nothing
   survives such a drop. Setting the flag after the await would make it safe; that is the commit step's crate.
4. **`tool_test`'s `unfaulted`** (`partition_workload/mod.rs:902`) recurses once per store (rule 1). `SimStore` offers
   only a closure, `unfaulted_reads`, so an iterative form needs a guard on `SimStore`.
5. **`ChangesetWorkload`'s seven `pub` counters** (`tool_test`), like `ReportAsked`'s fields, wait on the owner's choice.
6. **`..=` ranges** (`1..=PARTITIONS_MAX` and kin: `metadata_group_access.rs:73`, `tool_bench`'s `partitions_run.rs:121`,
   `tool_test`'s `partition_workload/mod.rs:138,153` and `crash_harness.rs:164,186`) wait on the owner's decision on those
   ranges.
7. **Storage's `"{cpu}"` message** stays formatted, its text read by the supervisor's and storage's scenarios.

The accept loops the review listed as spinning now take `AcceptPace`, an edit of another agent in this wave.

## 12.4 Coverage

The owner closed 12.4 on 2026-11-08 with the lines below listed as they stand: coverage is high, and a further pass would return little for its cost.

`scripts/coverage.sh` ran over every crate the change adds `src/` files to, 54 crates in four parts, each with a target directory of its own (`target/llvm-cov-target-0` to `-3`). The lines that count are those the change adds to a crate's `src/` in the working tree, files added since `1f4b67a3` included, less those `git blame` gives to `08ff8f26`; "instrumented" is the part llvm-cov counts. Every added line is covered or listed with its reason. The reasons are of five kinds: unreachable by construction, each with its argument; an OS or platform failure; a cost past a test's (a 60 s deadline, 4 GiB, a probability below 2^-128); a binary whose lines run only in the process scenarios of its test crate, which cargo-llvm-cov does not count, each function mapped to its scenarios; and reachable in process by a test not written, which is open rather than justified. Other agents edited comments in these files during the wave, so each part measured again at the end and its line numbers are the final tree's.

**Wave 3b** wrote the tests wave 3 listed as not written in `common_compute`, `client_core`, `client_tokio`, `common_frontend`, the config CLI, `cluster_partition`, `common_storage`, `store_apply`, `transactions_commit` and `tool_test`, four agents each with a target directory of its own (`target/llvm-cov-target-b0` to `-b3`), measured as wave 3 measured, at its start and on the final tree, by wave 3's `added.py` and `intersect.py`. Other agents' doc lines and `#[must_use]` grew the added counts meanwhile, so these crates' line numbers below are the final tree's after wave 3b, and each one's listing below is wave 3b's. Every suite each changed passes under Bazel, and clippy at `-D warnings` over its targets is clean.

### Part 0: wire, config check, role, TOML, codec, placement, keys, the cluster tests' harness, telemetry, the compio client, the block and index traits

**How it was measured.** `scripts/coverage.sh <crate>` per crate, `CARGO_LLVM_COV_TARGET_DIR=target/llvm-cov-target-0`. The lines that count are those `git diff -U0 1f4b67a3` adds to the crate's `src/` in the working tree, files added since included, less those `git blame` gives to 08ff8f26; "instrumented" is the share of them llvm-cov counts, the rest being comments, blank lines and declarations. Every test added or changed passes under Bazel: `ultragraphdb_common_config_check:all`, `ultragraphdb_common_role:all`, `ultragraphdb_protocol_wire:all`, `ultragraphdb_tests:unit`; clippy at `-D warnings` over the four crates' targets is clean.

| Crate | Added | Instrumented | Covered | Listed |
|---|---|---|---|---|
| `ultragraphdb_protocol_wire` | 10,413 | 5,030 | 5,020 | 10 |
| `ultragraphdb_common_config_check` | 1,746 | 1,248 | 1,237 | 11 |
| `ultragraphdb_common_role` | 1,426 | 652 | 643 | 9 |
| `ultragraphdb_tool_toml` | 1,228 | 514 | 514 | 0 |
| `ultragraphdb_core_codec` | 1,104 | 533 | 532 | 1 |
| `ultragraphdb_cluster_placement` | 836 | 443 | 443 | 0 |
| `ultragraphdb_tests` | 661 | 374 | 358 | 16 |
| `ultragraphdb_common_telemetry` | 478 | 192 | 191 | 1 |
| `ultragraphdb_client_compio` | 451 | 210 | 197 | 13 |
| `ultragraphdb_traits_blocks` | 230 | 75 | 75 | 0 |
| `ultragraphdb_core_keys` | 130 | 37 | 37 | 0 |
| `ultragraphdb_traits_index` | 95 | 24 | 24 | 0 |

Before the tests below, 282 instrumented added lines were uncovered: 160 in the config check, 45 in the harness, 40 in role, 22 in the wire, 13 in the compio client and one each in the codec and telemetry. 61 stay listed.

**The telemetry suite needs Bazel's environment under Cargo.** `test_init_refuses_without_installing_then_installs_once` expects the refusal naming `/nonexistent/ultragraphdb/from-environment.pem`, which `BUILD.bazel` sets as `SSL_CERT_FILE`; under a plain `cargo test -p` on macOS it names `/etc/ssl/certs/ca-certificates.crt` and fails. It was measured with `SSL_CERT_FILE` set as Bazel sets it. On a host holding that bundle the https endpoint would install the process's one subscriber, so the test depends on the variable rather than on the host.

#### A defect fixed: a refusal's message lost from the log

`serve.rs`'s warning for a refused CloseRequest and `role_session.rs`'s for a subscription ended named the refusal's text `message`, the field tracing already gives the event's own text. The event carried two fields named `message`: a JSON formatter keeps the last, the event's text, and the refusal's reason was lost. Both now name it `reason` (`ultragraphdb_common_role/src/utils/serve.rs:380`, `ultragraphdb_common_frontend/src/types/role_session.rs:234`). `test_a_refused_close_request_is_answered_by_nothing` records the warning through a subscriber of its own and fails on the old name, seen failing by a temporary edit.

#### `ultragraphdb_common_config_check`

Tests added: `tests/utils/check_central_tests.rs`, new, four tests (a central file that is not TOML, a key outside any target by its line, a declared file that is no string or an absolute path, a folder that can be read but not listed, which names no stray file); in `check_target_tests.rs`, the config service's file without `plaintext` and with an `ftp://` address, a target name and a key naming no role, and each refusal of a file (a value out of its field's range, an unknown table, a root key, a value of another kind, a line past `LINE_BYTES_MAX`); in `cross_checks_tests.rs`, each metadata bound at 0, a partition count of 0 with the storage partition outside it, an observability address naming a host, a storage instance with no role address, and instances the active revision lacks or whose file is refused; in `merge_tests.rs`, `\n` and `\r` by their short escapes and a layer that is not TOML; in `defect_tests.rs`, a defect's revision; in `write_central_tests.rs`, a folder that cannot be made.

Listed, each unreachable by construction:
- `src/utils/check_central.rs:166`: `stray_files` runs only after `read_central` read the central file, and a readable file's path has a parent, `""` for a bare name.
- `src/utils/check_target.rs:103`: `ServiceConfig::problems` collects every refusal `ServiceConfig::parse` can raise, so with none collected the parse succeeds.
- `src/utils/check_target.rs:182-188`: `ServiceConfig::problems` yields `Toml`, `UnknownTable` and `OutOfRange` alone; `Missing` and the endpoint's security refusals come from a role's typed reads, which the check does not make.
- `src/utils/cross_checks.rs:65`: every report names an instance of the target, and every instance is a key of its files, so its label is found.
- `src/utils/cross_checks.rs:532`: the implicit `else` of a storage instance without settings; `partitions` returned at line 524 when any storage instance has none.

#### `ultragraphdb_common_role`

Tests added: in `tests/utils/serve_tests.rs`, a client endpoint announcing the Welcome its server makes and a role endpoint the largest frame (`with_welcome`); an unknown name refused under the metadata group's secret (`with_unknown_secret`); a session at its default hooks (`Defaulted`), over an initial credit of 1, whose open stream leaves, is credited and cancelled, a push to the cancelled stream going nowhere and a Scan answered `Done` sending nothing; the CloseRequest test now captures its warning. `tests/types/role_client_tests.rs`, new: a password SASLprep refuses, and a name whose escaped client-first message passes the largest frame, refused as an Authenticate not built. `tests/types/role_reader_tests.rs`, new: `next_frame_in` holding two replies in the caller's pool. `role_secret_tests.rs`: a secret SASLprep refuses has no credentials. `serve_tests`' helpers are `pub(crate)` for the two new files.

Listed:
- `src/types/role_client/mod.rs:185`: the SCRAM exchange ends in its two steps or is refused by `ClientAuthentication::receive`, so `EXCHANGE_STEPS` never runs out.
- `src/utils/serve.rs:168-169`: the operating system gives no random bytes for a nonce.
- `src/utils/serve.rs:224`, `265`, `288-291`: a connection's deadline passing. A role endpoint's only deadline is the authentication timeout, 60 s (its idle limit is `Duration::MAX`); a client endpoint's is that or the idle limit, 600 s. The server reads the clock, so a test would wait out 60 s.

#### `ultragraphdb_protocol_wire`

Tests added: `conditional_write_tests.rs`, a relayed write carrying its principal through its frame; `resume_key_tests.rs`, a term table whose first kind byte no term has; `reference_view_tests.rs`, a torn term record. `test_a_last_row_of_another_length_is_refused` shortened section 0 by 4 bytes, which the section offsets refused before the row's rule; it now lengthens it by 4 into the padding before section 1, and asserts the `CountMismatch` of `"resume row"`.

Listed, each unreachable by construction:
- `src/types/commit/mod.rs:103-104`, `src/types/commit_view/mod.rs:65`: a reply's node terms are IRIs; `push_node_result` interns an IRI, and `CommitView` refuses any other node term.
- `src/types/drop_command_view/drop_command_view_body_view.rs:67`: the IRI arm inside the arm the outer match gives every non-IRI term.
- `src/types/drop_command_view/drop_command_view_body_view.rs:71`: a triple term needs its three terms in the table, which a drop's count of one refuses first.
- `src/types/drop_command_view/drop_command_view_body_view.rs:99`: `from_frame` gives a predicate and a graph drop their one term.
- `src/types/entries_sequence.rs:157`: `check_range` refuses a frame whose range and `through` form no run of the Scan, before `check_entries`.
- `src/types/reference_view/mod.rs:64`, `src/types/report_view/mod.rs:50`: `from_frame` refuses every term that is not an IRI (a report's first).
- `src/types/report_view/report_view_body_view.rs:80`: `from_frame` gives a refused and a build-failed report at least one term, so `predicate()` and `at_fault()` find one.

#### `ultragraphdb_tests`

Tests added in `tests/utils/harness_tests.rs`: `members_at` against an in-process frontend endpoint serving a compute and a storage member; the double started twice, refused naming a taken port; the double answering an instance it does not serve with 404, and letting a connection closed before its head go; `until_ports_free` ending refused past `START_SECONDS`, 20 s, run beside the suite's other 20 s test. The two refusal tests name whichever declared address is taken first, since another run's process may hold one; that happened once here, on port 17090.

Listed:
- `src/utils/harness.rs:203`: writing the config service's own file fails, an OS failure in a folder just made.
- `src/utils/harness.rs:222-227`, `230-238`: `start_config_service` and `start_config_service_at` start the config binary, which Bazel copies into the `bdd` target alone (`test = false` under Cargo). The `bdd` target reaches them in every scenario: `role_binaries.feature` through `start_in_order`, every whole-cluster scenario through `cluster.rs`' `start`; `ultragraphdb_storage_tests:bdd` reaches the first too.

#### `ultragraphdb_client_compio`

No test added; each line is unreachable by construction or a timing path of the driver.
- `src/types/client/client_requests.rs:20-21`, `28`, `44`, `86`, `175`, `197`, `211`: `mismatch()` and its arms; the core answers each pending request with the reply of its own kind.
- `src/types/client/mod.rs:216`, `359`: a write's or a read's task ending without its answer, which only a panic in it gives; the handles awaited are never dropped while awaited.
- `src/types/client/mod.rs:269`: no deadline and no failure held with `done` false. `next_deadline` gives every request awaited on a connection a deadline (`wait_timeout`, `read_timeout`), and a lost connection stores its failure first; no test found a way here.
- `src/types/client/mod.rs:291`, `314`: a connection lost while the deadline passed during a read is taken (`shut` after `take_arrived`), and the reader gone midway through `take_arrived`. Both need a frame to arrive and a deadline to pass within the same turn; the core's handling of that deadline is covered by its own simulated suite.

#### `ultragraphdb_core_codec`

- `src/types/decoder/mod.rs:91`: a block whose deltas do not walk. A block exists only once its deltas were read in full, and the `debug_assert!` before it fires first in every test build.

#### `ultragraphdb_common_telemetry`

- `src/telemetry/init.rs:60`: an https exporter with a readable trust store. A process installs one subscriber, which the init test spends on an http endpoint after the https refusal, and Bazel's `SSL_CERT_FILE` names no file; `TlsConnector::new`'s error arm needs a ring provider refusing TLS 1.3 and 1.2, which it does not.

### Part 1: traits, store simulation, posting, transport, HTTP server, transactions, tokio client, partition, frontend, config CLI, storage, bench

**How it was measured.** `CARGO_LLVM_COV_TARGET_DIR=target/llvm-cov-target-1 scripts/coverage.sh <crate>` per crate. The lines that count are those `git blame` of the working tree gives to a commit after 1f4b67a3 other than 08ff8f26, or to no commit, under the crate's `src/`; "added" below is that count, "instrumented" the part of it llvm-cov counts. Every crate measured at the end was measured after its last test was added, and every changed suite passes under Bazel. Other agents edited comments in some of these files during the run (`serve_http.rs`, `role_link_tasks.rs`, `restore.rs` and others); line numbers are the working tree's at the end. One frontend run failed "A role connection's dial past its bound closes it" because `role_link_tasks.rs`'s warning text was being edited at that moment; the reruns passed, 119 of 119.

| Crate | Added | Instrumented | Uncovered before | Uncovered after | Tests added | After wave 3b |
|---|---|---|---|---|---|---|
| `ultragraphdb_traits_store` | 49 | 12 | 0 | 0 | — | |
| `ultragraphdb_traits_log` | 111 | 17 | 0 | 0 | — | |
| `ultragraphdb_traits_catalog` | 280 | 78 | 0 | 0 | — | |
| `ultragraphdb_store_sim` | 809 | 480 | 9 | 0 | 3 | |
| `ultragraphdb_store_posting` | 1,172 | 611 | 4 | 2 | 1 | |
| `ultragraphdb_protocol_transport` | 1,410 | 695 | 2 | 2 | — | |
| `ultragraphdb_transport_http_server` | 718 | 330 | 14 | 4 | 2, and an assertion | |
| `ultragraphdb_transactions_txn` | 1,765 | 999 | 93 | 11 | 10 | |
| `ultragraphdb_client_tokio` | 622 | 317 | 26 | 16 | 10 | 15 of 325, all unreachable; 2 tests more |
| `ultragraphdb_cluster_partition` | 3,091 | 1,647 | 135 | 50 | 6 | 14 of 1,647; 17 tests more |
| `ultragraphdb_common_frontend` | 8,930 | 5,348 | 684 | 578 | 8 | 232 of 5,359; 23 tests and 15 scenarios more |
| `ultragraphdb_config_cli` | 274 | 171 | 171 | 171 | — | 171, every path but an OS failure run by a scenario; 8 scenarios more |
| `ultragraphdb_storage` | 1,017 | 725 | 725 | 725 | — | |
| `ultragraphdb_tool_bench` | 1,619 | 961 | 35 | 35 | — | |

#### `ultragraphdb_store_sim`

Added `tests/types/store_model/store_model_tests.rs`: a scan ending in a failure that names no key breaks the read invariant; a scan and a many-key read through a snapshot the model does not hold are compared with nothing. `tests/types/simulation/simulation_tests.rs`: a store each recovery leaves down stops the run once `RECOVERIES_MAX` recoveries in a row run out, the count read from the run, and one crash fewer lets the last recovery bring it back. Nothing listed.

#### `ultragraphdb_store_posting`

Added `tests/utils/walk_lists_tests.rs`: a snapshot whose scan fails ends the walk with the store's error before any key is visited. Listed:

- `src/types/lists/lists_rollup.rs:105`: a turn that took `BATCHES_PER_TURN_MAX` batches and found more. The bound is every full batch both queues hold and the high-priority batch being filled, so only a writer queuing during the turn, on another thread, reaches it; a test would race the turn.
- `src/utils/walk_lists.rs:103`: `held.next()` after `held.peek()` gave `Some(Err)` returns that error, so its `Some(Ok(_)) | None` arm is unreachable by construction.

#### `ultragraphdb_protocol_transport`

- `src/types/server_connection/mod.rs:487`: the guard `if open` of the Begin, Open, ComputeRead and Read arm. Both of its outcomes run (489: 10, 492: 26, 496: 25, 499: 2, 500: 14); llvm-cov gives the guard's own region 0, an artifact of a guard on an or-pattern.
- `src/types/server_connection/mod.rs:623`: an opening refused other than by `StreamLimit` or `UnknownStream`. `Outbound::open_as` refuses only stream 0, which `server_connection/mod.rs:150` closes the connection on first, and an open stream, which line 489 answers before any opening: unreachable by construction.

#### `ultragraphdb_transport_http_server`

Added `tests/types/http_response_tests.rs`: a status with no reason is named `Status`. `tests/types/bearer_key_tests.rs`: a malformed digest's text names its 64 digits, read from the fixture's length. `tests/utils/serve_http_tests.rs`: a refused peer taking no write is left undrained; a TLS listener answers a verified client after a clear-text client's failed handshake and a silent one closed at the connection's time. `Cargo.toml` takes `ultragraphdb_transport_tls` with `test-utils` as a dev-dependency for `TestPki`. Listed:

- `src/utils/serve_http.rs:88`: `read_request`'s loop exhausted. Each turn reads at least a byte, and `parse_head` answers complete or refused once a head and its declared body, each within the limits, are in: unreachable by construction.
- `src/utils/serve_http.rs:123-125`: a failed `accept`, an OS failure of the listening socket.

#### `ultragraphdb_transactions_txn`

Added `tests/types/frame_changes_tests.rs`: a lowercase language tag borrowed with its direction, an uppercase one lowered; a triple term's blank subject held, a literal subject or predicate refused; a triple term `FrameTerms` did not hold refused; DeletePattern, SetProperty and DeleteProperty read by reifier and by quad, a blank graph; a declaration refused. `tests/types/transaction/transaction_partition_tests.rs`: a store failing under a reverse read, a mark's read or a node's ID ends the transaction; a lookup by an undeclared index ends it; a blank node keeps its ID; a bound transaction writes an edge with properties and reads its blank node back. `tests/types/transaction/transaction_scan_tests.rs`: a store failing under an IRI's resolution or a dictionary read ends the transaction. Listed:

- `src/types/prepared/mod.rs:167-168`, `214-215`: the record's refusal of a build-end round and of a catalog round. Both are written with no limit (`usize::MAX`), so only the encoder's refusal of a record past the format's bounds remains, which a round carrying no changeset of its own does not reach.
- `src/types/transaction/mod.rs:329`: `commit` taking an execution already gone; line 319 returned for that case before: unreachable by construction.
- `src/types/transaction/transaction_partition.rs:171-172`: `Execution::mark` (`ultragraphdb_store_apply/src/types/execution/execution_partition.rs:115`) returns `Ok` on every path, so its refusal is unreachable.
- `src/types/transaction/transaction_scan.rs:118`: `span_fits` with no execution; it runs only after a page succeeded, which leaves the execution in place.
- `src/types/write_set_builder/mod.rs:280-282`: a conflict point for a versionless write outside the dictionary. Every versionless write the applier makes is a dictionary entry (`execution/mod.rs:339`, `changeset_core_operate.rs:66`, `changeset_core_logical.rs:49`), so the filter keeps none.

#### `ultragraphdb_client_tokio`

Added `tests/types/tokio_tls_stream/tokio_tls_stream_tests.rs`, its `Probe` taking a failing or empty write, a write of at most some bytes and reads interrupted without end: a handshake whose write fails, over a stream taking no byte, and one interrupted past `HANDSHAKE_TURNS_MAX` turns; a read interrupted past `READ_TURNS_MAX`; a read into no room; a peer gone without `close_notify` is `UnexpectedEof`; a failed write is the caller's; a write after a shutdown is refused; records a stream takes a byte at a time go out whole. `tests/types/client/client_shell_tests.rs`: a stream taking no byte of the Hello fails the connection with `WriteZero`.

Wave 3b added, in `tests/types/client/client_shell_tests.rs`, `test_a_connection_failing_while_arrived_frames_are_read_is_given_up`:
a `Breaking` stream whose reads, once armed, fail where they would wait. The deadline due after a frame is taken, the
frames already arrived are read, and the read past them fails the connection: the read waiting fails with that I/O error,
and the shell shuts the connection (`take_arrived`'s failure, and the shut after the deadline). And
`test_a_timeout_past_an_instant_waits_for_the_server`, the defect below, also in `client_compio`'s `client_shell_tests.rs`.
Measured in wave 3b (`target/llvm-cov-target-b0`): 621 → 671 added, 316 → 325 instrumented, 17 uncovered at its start
and 15 at its end, the tokio suite passing whole, 55 tests and its scenarios. Listed, each unreachable by construction:

- `src/types/client/client_requests.rs:20-21, 28, 44, 98, 207, 229, 243`: `mismatch()` and the arms returning it. `ClientCore::decode` builds
  the `Reply` from the pending request's `Expect`, which the request fixes (`Register` → `Registered`, `Commit`/`Write` →
  `Committed`, `Drop` → `Dropped`, and so on), so a call's `settle` never meets another kind.
- `src/types/client/mod.rs:79`: the incoming frame's buffer missing after a fill; nothing awaits between taking and restoring it.
- `src/types/client/mod.rs:272`: `take_arrived` finding no stream. It is called just after a frame was read on the stream; `take_frame` leaves
  the stream, and the failure arm returns.
- `src/types/client/mod.rs:330-334`: the drive loop with no stream and no deadline. The stream is gone only once the core has given the
  connection up (`flush`'s and the read's failures call `disconnected`, `shut` runs when the core is not connected). A lost
  connection answers every request and read but a commit waiting to be sent, which `needs_connection` handles at the loop's
  top, so `done` holds before the loop gets here. The branch (`handle_timeout(deadline)` beside it, unchanged) was never
  entered before the fix either.

**A defect fixed in wave 3b: a timeout past an `Instant` made both shells refuse to connect.** `ClientConfig::with_timeout(Duration::MAX)` is the core's "never time out": the core sets no deadline for it
(`test_a_timeout_or_an_expiry_past_an_instant_sets_no_deadline`). Both shells took a core with no deadline for a core with
nothing to wait for. The tokio client's `connect` returned `not_connected` at once against a server that answers. The compio
client panicked first, in compio's `timeout` (`overflow when adding duration to instant`), on the connection's own dial. Tests
first: `client_shell_tests.rs`'s `test_a_timeout_past_an_instant_waits_for_the_server`, in both shells, failed as described.

- `ultragraphdb_client_tokio/src/types/client/mod.rs`, `drive`: a connection with no deadline reads with no timeout. The
  `not_connected` answer stays for a core with no connection and no deadline.
- `ultragraphdb_client_compio/src/types/client/mod.rs`: `within(timeout, future)` bounds the dial, the shutdown and each write
  by `timeout_at(now + timeout)`, and awaits with no bound where `now + timeout` overflows. `drive` reads with no bound where the
  core holds no deadline, as the tokio shell does.

`client_compio`'s `src/types/client/mod.rs` changed by that fix and was not measured again; its suite passes under Bazel.

#### `ultragraphdb_cluster_partition`

Added `tests/errors/partition_error/partition_error_tests.rs`: `TransactionHeld` names its identity as 32 hexadecimal digits. `tests/types/partition_host/partition_host_bound_tests.rs`: a bound counter is done at once and again, refused for another partition's subject, refused past its limit, and waits for a predicate homed elsewhere, then ready. `tests/types/partition_host/partition_host_read_tests.rs`: a host pages lists and values with no transaction; a refused bound page, values page and dictionary read each end their transaction. `tests/types/partition/partition_referrer_tests.rs`: every change kind's first subject, a statement's by its triple, routes, and a blank subject or an operation naming none does not. `tests/types/fanout/fanout_tests.rs`: a reverse-count lookup over no partition finds nothing.

Wave 3b (`target/llvm-cov-target-b2`): 3,091 → 3,139 added, 1,647 instrumented, 50 uncovered at its start and 14 at its
end; `ultragraphdb_cluster_partition_tests` 97 (80 before), `bdd`, `workload_faults` and `workload_seeds` pass under
Bazel. It added:

- `tests/types/fanout/fanout_tests.rs`: a store failing under the home's read of the referrer set, and one failing under a
  named partition's own reverse entries (the partition restarted, so its lists come from the store), each fails a reverse
  read and a reverse count with the store's error and no rows; a lookup by count leaves out an object of another count; a
  count index holding more counts than one scan reads is read page after page, the object of count 257 found (33,153
  edges applied as set-up, 3.4 s in a debug build).
- `tests/types/home/home_tests.rs`: a registration by ID at a partition holding that ID as its copy of another partition's
  entity is refused naming the owner; a registration whose entry the log cannot make durable is answered with the
  pipeline's refusal under its ticket; one at a home whose latest close is `Epoch::MAX` is refused busy.
- `tests/types/partition/partition_tests.rs`: a begin above the watermark is refused naming both.
- `tests/types/partition/partition_referrer_tests.rs`: a Declare among a write's changes registers nothing; an upsert's link
  to a blank node registers only its predicate; a store failing under a foreign predicate's dictionary read ends an edge's
  and a compare-and-set's references, asked at `rdf:type`'s home, which reads nothing for it.
- `tests/types/partition_mailbox_tests.rs`: a registration for a home outside the mesh goes to the shell as
  `RemoteMessage::Register`, naming the home and the write's ticket.
- `tests/types/partition_host/partition_host_tests.rs`: a home whose pipeline stopped under a registration's round takes
  the next registration and answers neither; the writes asking wait.
- `tests/types/partition_host/partition_host_bound_tests.rs`: a Drop whose predicate the store fails to read is refused,
  taking no ticket; a refusal answering an index past a bound transaction's references still ends it; a bound operation
  past its limit when its references are answered is refused then.
- `tests/types/partition_host/partition_host_read_tests.rs`: a store failing under a bound read of a foreign term ends the
  transaction.

Listed, 14. Unreachable by construction:

- `src/types/fanout/mod.rs:204`: a row of the count-index scan that is no count-index key. The range runs from the
  tenant's, graph's, predicate's reverse count key of 0 to that of `u32::MAX`, and every key in it parses as
  `QuadKey::CountIndex`; one that parses as nothing fails the read on the line before.
- `src/types/partition_host/mod.rs:193-196`: `submit`'s begin refused. It begins at read version 0, the watermark, and
  `Transaction::begin` refuses only a version above it; loading the catalog and beginning the execution return `Ok` on
  every path (`ultragraphdb_store_apply/src/types/changeset_core/mod.rs:159-166`, `214-224`).
- `src/types/partition_host/mod.rs:375`: `finish_if_answered` removes the parked write it found two lines before, under the
  same `&mut self`.
- `src/types/partition_host/mod.rs:384`: a reference with no answer, which the check above it rules out.
- `src/types/partition_host/mod.rs:394`, `src/types/partition_host/partition_host_bound.rs:206`, `237`:
  `Commit::ReadOnly`, which only a `Mode::ReadOnly` transaction commits to
  (`ultragraphdb_transactions_txn/src/types/transaction/mod.rs:325`); `submit` and `bind` begin `Mode::ReadWrite`.
- `src/types/partition_host/mod.rs:413`: `resend` walks the tickets it collected and removes only the one it is on.
- `src/types/partition_host/partition_host_bound.rs:461`, `480`: `note_unheld` and `finish_bound` take the key
  `bound_answered` found in `self.bound` just before; nothing removes it between.

Past any frame:

- `src/types/partition/partition_referrer.rs:359`: a named subject with no bucket. `iri_bucket` answers `None` only for an
  IRI past `u32::MAX` bytes, and a blank subject's ID always has a bucket. No frame holds such an IRI (1 MiB at most), and a
  test would allocate 4 GiB: a cost past a test's, as `core_catalog`'s lengths past `u32::MAX` are.

#### `ultragraphdb_common_frontend`

`src/lib.rs` now exports `check_change` and `check_operation` beside `check_table`; they were `pub` in a module the crate root keeps private. Added `tests/utils/term_rules_tests.rs`: each datatype the XSD parser knows takes its lexical form and refuses another; a datatype without a parser takes any form; the vector datatype, its form from `format_vector`; a malformed language tag, IRI and datatype refused; each change kind and each commit-time operation passes and is refused naming the first term in a slot that does not take it; an IRI no one can hold refused. `tests/utils/iri_place_tests.rs`: a tenant's IRI outside its skolem path is ordinary. Those reach 104 of term_rules.rs's 106 lines, the checks the group 9 mutants record left unobserved.

Wave 3b (`target/llvm-cov-target-b1`): 8,930 → 9,057 added, 5,348 → 5,359 instrumented, 578 uncovered at its start and
232 at its end: about 55 unreachable by construction, about 15 OS failures, about 150 open. Under Bazel
`ultragraphdb_common_frontend_tests` passes 82 tests (59 before), `:bdd` 134 scenarios (119) and
`//ultragraphdb/ultragraphdb_config_tests:bdd` 44 (36); `cargo clippy -p ultragraphdb_common_frontend -p
ultragraphdb_config_tests --all-targets -- -D warnings` is clean.

**A rig added.** `src/utils_test/owner_run.rs`, behind `test-utils`: `owner_run(owner, clock)`, an `Owner` over logs the test chose, run on the test's runtime by a clock the test moves, and its `OwnerHandle`. Asks sent before the run is next polled are taken in one turn, so a test orders a turn exactly; over `SimStorage` a test fails the log's next write. `tests/types/owner/mod.rs` holds the helpers the owner suites share (`TestClock`, `recovered`, `running`, `ticking`, `closed_after`).

Tests added:

- `tests/types/owner/owner_answer_tests.rs`: a Lease of type prefix 0, a close request and a cursor past `Epoch::MAX`, a report of partition 0 refused with code 1, a session of a principal the group does not hold with code 7, and the owner goes on; a registration evicting a session and that session's commit check taken in one turn refuse the commit with code 12.
- `tests/types/owner/owner_catalog_tests.rs`: a version built on one not current is a Catalog conflict, whether replaced earlier or by an offer of the same turn; an offer past `MetadataState::PENDING_MAX` pending versions refused with code 7 naming the bound; a Drop of a predicate leaves an entity type, a Drop of everything builds the initial catalog and a second in the same turn is refused; a default entry recorded once, a predicate past `PREDICATE_BYTES_MAX` refused; every kind of declaration reaches Describe (a predicate's kind, datatype and vector index, entity types numbered in preorder, a relation and its roles, a relation replaced), and a declaration naming no IRI refused naming it.
- `tests/types/owner/owner_check_tests.rs`: a tightening a partition refuses is withdrawn, its Commit refused naming the predicate and the term at fault, a declaration it left as it was kept, both log events emitted; a restarted owner withdraws a tightening no Commit waits on; a withdrawal refused while the pending versions are full waits, says so, and is offered again after a close.
- `tests/types/owner/owner_turn_tests.rs`: the sequencer stores a snapshot after its closes and a restart recovers from it; a failed write of the sequencer's log stops the owner with an I/O failure.
- `tests/utils/owner_thread_tests.rs`: a data directory under a file, a snapshot that is a directory and one that does not decode, and an unreadable bootstrap secret file each stop the start with their kind of error; a tenant the settings name is created once; the parts of a running owner stop it apart.
- `tests/types/role_session_tests.rs`: Lease, Placement and Catalog answered from the owner, a CloseRequest taken, a Join at an address that does not parse held, a Members request holding members, a second subscription and a command the session does not host refused; a report of each kind answered; a subscription's closes carry announcements and evictions; Members after the owner stopped is unavailable.
- `tests/types/auth_throttle_tests.rs`: a new name goes uncounted while every refusal in the full table runs. `tests/types/role_links_tests.rs`: links without a map hold no run and no partition. `tests/utils_test/role_double_tests.rs`: a double serves its cluster; an Error frame's refusal reads back whole.
- `features/schema-changes/declaring.feature`, `tests/declaring_steps.rs`: a transaction holding declarations refuses an Operation, a change of data and a read, one that changed data refuses a declaration, a Write mixing both is refused with `detail` the index of the first change of data; a Commit of declarations through a session is refused.
- `features/client-endpoint/operations.feature`, `tests/operation_steps.rs`: every kind of change routed by its subject; Operations by subject, compare-and-set included, a blank subject's in turn, and a first read naming no subject binding the tenant's one partition; a claim of a typed, language-tagged and directional literal asked of its home, an IRI and a datatype the store names by ID in turn; a home refusing the predicate, a home no process joined and a home's connection closing under the claim each refuse the write and end the transaction; a first Write finding no partition in turn, one naming an unjoined partition, and a first read reaching two partitions refused.
- `features/client-endpoint/relays.feature`, `tests/relay_steps.rs`: a retried Commit answered with code 14, with code 18 naming the last request, and unavailable with a partition unjoined; a DirectWrite refused naming another owner resent there once; a DirectWrite and a Drop whose partition closes under them unavailable; Drops of the tenant, a graph and the tenant's data.

`BUILD.bazel`'s `bdd` target lists the three step files.

**Listed: unreachable by construction.**

- `src/types/owner/owner_answer.rs:287-288, 291`: no placement held. `Owner::recover` forms the map and syncs it before the owner serves (`owner/mod.rs:124-129`), and no command removes it.
- `src/types/owner/owner_answer.rs:351`, `src/types/owner/owner_check.rs:70`: a log refusal of an append. The single-node log refuses an append only once a failed sync poisoned it, and every sync failure stops the owner at that sync through `stopped`; `report_check` refuses nothing else (`metadata_state_apply.rs:63`).
- `src/types/owner/owner_check.rs:88, 94`: a version in `checking()` with no record, or a record not `Checking`. The state inserts into the set with the record at an offer (`metadata_state_apply.rs:198`) and removes it as it sets the record `Withdrawn` or `Settled` (`:220-222`, `:260-261`); the release keeps the version before one being checked (`:624-629`).
- `src/types/owner/owner_catalog.rs:165-167, 231-234`: the current version's bytes absent or not decoding. The release keeps the latest version in effect and every later one (`metadata_state_apply.rs:619-633`), and every version's bytes are `version_bytes`' own encoding. `:184-186`: a Drop's version past the cap; it holds a subset of the current version's declarations, whose bytes passed the same cap.
- `src/types/owner/owner_turn.rs:180, 182-183`: a waiting subscriber's `close_after` failing. A subscriber waits only at or past the latest close, each turn closes at most one epoch, and the newest close is always held (`sequencer_deliver.rs:49-66`).
- `src/utils/owner_thread.rs:59-62`: the owner's thread sends its outcome on every path of its closure.
- `src/utils/snapshot_file.rs:34`: the fall-through of `path.parent()` being `None`; every snapshot path is joined under the data directory.
- `src/types/role_session.rs:195`, `src/utils_test/double_session.rs:38`: a frame failing its view. `ServerConnection::take` views every frame first and closes the connection on a failure (`server_connection/mod.rs:144-147`). `role_session.rs:245-247`: framing a subscription's refusal fails only for code 26 with a high `detail` (`error_reply_body_builder.rs:32`); its refusals are code 1 or unavailable. `:258-260`: an EpochClose holds at most 4,352 records of 16 bytes, as its comment says. `:285`: a triple at fault; `ReportView::to_body`, called first, refuses one (`Report::refused`).
- `src/utils_test/double_session.rs:53`: the session holds a sender of its own channel. `src/utils_test/role_double.rs:228, 242, 249, 256, 263, 266`: a body view failing on a frame the frontend built and the transport validated.
- `src/utils/term_rules.rs:243`, `src/utils/iri_place.rs:51`: an IRI past `u32::MAX` bytes, which no frame holds.
- `src/types/client_session/client_session_writes.rs:82, 380`: a subject or a claim's predicate nobody can hold, or too long; `check_change` and `check_operation` refuse both first (`term_rules.rs:233-245`). `:90`: a literal, triple or absent subject; `Slots::node` refuses each. `:124`: a claim's predicate that is no IRI; `Slots::predicate` refuses it. `:178-181`: a Write or Operation on a stream with no transaction; the transport refuses it naming the command first (the scenario "a Write on a new stream is refused"). `:264`: `write` found the transaction one line before.

**Listed: an OS failure.** `src/utils/owner_thread.rs:152-154` (a runtime the OS refuses to build, the one path that drops the recovery's sender unsent); `src/types/client_session/client_session_requests.rs:132-134, 136, 314-316, 318` (the random source failing) and `:138-141` (128 random bits all zero, below 2^-128); `src/types/role_link/role_link_tasks.rs:70-72` (a role connection's failed write); `src/utils_test/role_double.rs:96` (a double's failed accept).

**Open: reachable in process, no test written.** These stay open: none has one of the kinds of reason this part accepts, and the lead decides.

- `client_session_writes.rs:59` (a declaration among a one-frame write's changes; believed refused by `DirectWriteView::from_client_frame`, not verified), `:341, 347, 358-359, 391-393, 427`.
- `client_session_one_frame.rs:130-135` (a one-frame write's term refused, its session ended), `232-235` (a one-frame write of the whole run), `243-246, 249, 312, 355, 358, 429-433, 440, 470-471, 500, 509`.
- `client_session_reads.rs:60, 64, 96, 155, 168-173, 184, 187, 230, 246, 254, 264-265, 268-270, 295, 308, 319, 329, 336, 354, 372, 383-385, 388, 418, 438, 447, 460, 492, 495, 518, 525, 534, 551, 585`.
- `client_session_commit.rs:45-46, 76, 79, 103-108, 122, 136, 143, 165, 209-212, 224, 274, 307, 337, 356`.
- `client_session_bind.rs:123, 141, 167, 201, 219, 306-308, 340, 378, 381, 395, 425`.
- `client_session_requests.rs:23-25, 120-124` (a client's command the session does not dispatch: an answer's command sent as a request closes the connection at the transport, so whether any command reaches this arm is not settled), `182, 188, 261`.
- `client_session_session.rs:60, 137, 144-148, 150, 216, 225, 227-231, 235-236, 250, 254-255`; `client_session_prepared.rs:39, 62, 65`; `client_session/mod.rs:199-200`.
- `role_link/mod.rs:47, 135-137` (a frame queued or a stream opened on a link already closed), `role_link_tasks.rs:124-126` (a frame on a stream the link holds no request on), `route_sink.rs:47`, `role_double.rs:276-277` (a command `partition_answer` leaves unanswered).

Most of the open client-session lines are guards of the form `let Some(..) = self.<table>.get(&stream) else { return }`: an event for a stream already gone. Each is either unreachable by construction or reached only by a role's frame arriving after its stream ended; neither was argued line by line.

#### `ultragraphdb_config_cli`

A binary with no library and no test target (`BUILD.bazel` declares the binary alone; Cargo runs `src/main.rs`'s 0 unit tests): 277 added lines, 171 instrumented. Wave 3b:

A binary with no library and no test target, so `-p ultragraphdb_config_cli` counts none of its 171 instrumented added lines; every path runs as a process under `ultragraphdb_config_tests`. Eight scenarios were added (`features/config-service/service.feature`, `tests/bdd.rs`), five against a stand-in server the test thread answers while the CLI runs (`restore_from_stand_in`). Each path and its scenario:

- `src/main.rs:30-60`, the restore's dispatch: "A restore rebuilds a folder a service can begin from"; `:36-37`, `--ca-file`: "A restore over TLS reads the trust file it names"; `:61-67`, the usage: "The CLI states its usage"; `:69-102`, the check: "The check finds what the service would refuse", "The check passes what the service would take", "The profiles ship and pass".
- `src/restore.rs:48-62`, `listed`: a retired target, "A restore leaves out a retired target"; a revision that is no number, "A restore refuses a listing naming a revision that is no number"; a line naming no target, "A restore refuses a listing line that names no target".
- `:69-81`, `client`: plain text, every restore of the service; `:75`, "A restore over TLS reads the trust file it names"; `:76`, "A restore over TLS reads the system's trust file".
- `:84-108`, `asked`: unreachable, "A restore of a service that is down says so"; 401 and 403, "A restore with the read key writes nothing"; another status, "A restore refuses a status the service answers with no success".
- `:111-174`, `run_restore`: the listing and its revisions, "A restore rebuilds a folder a service can begin from" and "A restore leaves out a retired target"; no target but retired ones, "A restore of a service holding retired targets alone writes nothing"; a folder holding files, "A restore adds to no folder"; settings this release refuses, "A restore refuses settings this release refuses".

Listed: `src/restore.rs:113-114`, a compio runtime the OS refuses to build: an OS failure.

No defect was found in `src/` by the frontend's or the CLI's new tests. Two of the tests first written were wrong and were fixed: a role session test held a reply frame while it sent the next request (a frame holds the connection's one buffer until it is dropped), and a scenario expected a home that ends a Reference's stream with no frame to fail the claim at once; the frontend learns nothing of such an end, so the scenario closes the home's connection instead.

#### `ultragraphdb_storage`

A binary with no library and no test target. All 725 instrumented added lines, `src/main.rs` and `src/service.rs`, run as the storage process under `ultragraphdb_storage_tests`, which `-p ultragraphdb_storage` does not count: the start, readiness and stop (`storage_role`, `spawn_core`, `serve_core`, `wait_or_end`, `stop_the_rest`) by "Storage flushes on shutdown", "Storage is not ready before recovery", "Storage stopped before it joined is never ready", "A configuration error exits with its code" and "An invariant violation exits with its code"; replay and the catalogs held (`replayed`, `load_catalogs`, `versions_held`, `held_versions`, `announced_catalog`) by the restart scenarios and `hosted_drops.feature`; `spawn_discards` by the retention scenarios of `transactions`; partitions and connections (`serve_partition`, `core_parts`, `open_partition`, `accept_connections`, `until_lost`) by "Two storage processes share the partitions" and "A reference crosses storage processes"; the subscription (`placement_of`, `joined_subscription`, `deliver_closes`, `subscription_end`, `close_of`) by "A subscription past the closes held exits with its code" and "An invariant violation exits with its code"; `forward_answers` and `frontend_requests` by "A registration is a role command" and the parked-write scenarios; `top_up` and `lease_ahead` by "A partition leases its blocks ahead of need"; `send_report` by "A pinned role reports what it holds".

#### `ultragraphdb_tool_bench`

A benchmark tool, measured and listed. 35 lines:

- `src/bin/wal_bench.rs:28-30`: the binary's `main`, run by hand on the VM (12.5).
- `src/utils/apply_run.rs:186-189, 233-236`, `src/utils/pipeline_run.rs:187, 189`: back-pressure and draining that depend on the threads' scheduling.
- `src/utils/partitions_run.rs:258-259, 344-345, 387-389, 408-410, 445-447`, `src/utils/pipeline_run.rs:217-219`: a partition's loop failing, a refused commit counted, and the checks that fail only on a wrong run: an object with no ID at its home, a reverse read differing from one partition's, an IRI with no partition, a read-write transaction ending read-only.
- `src/errors/bench_error/bench_error_from.rs:38-40`: no bench path raises a `QuadError` of its own.
- `src/types/partition_shape/mod.rs:136`: a map giving a partition none of the tenant's run, which no layout the bench draws gives.
- `src/utils/setup.rs:63`: the applier refusing to open over a fresh store.

### Part 2: compute, cluster, client core, config, transport, stores, binaries

`scripts/coverage.sh <crate>`, each run with `CARGO_LLVM_COV_TARGET_DIR=target/llvm-cov-target-2`. The lines that count are
the instrumented lines of `git diff -U0 1f4b67a3 -- <crate>/src` on the working tree, less those `git blame` gives to
08ff8f26; comments and blank lines carry no count. Other agents added comment lines to several of these files during the
wave, so each crate was measured again on the tree as it stood at the end, and the line numbers below are that tree's.

| Crate | Added lines (instrumented) | Covered | Uncovered | Tests added | After wave 3b |
|---|---|---|---|---|---|
| `common_compute` | 2,910 | 2,699 | 211 | 7 | 42 of 2,911: 23 unreachable, 5 a corrupt store, 14 open; 23 tests more |
| `cluster_metadata` | 2,022 | 2,012 | 10 | 4 | |
| `store_quads` | 1,519 | 1,505 | 14 | 3 | |
| `client_core` | 1,293 | 1,255 | 38 | 0 | 19 of 1,293, all unreachable; 13 tests more |
| `cluster_sequencer` | 788 | 784 | 4 | 0 | |
| `common_config` | 658 | 658 | 0 | 7 | |
| `common_service_config` | 463 | 463 | 0 | 1 | |
| `protocol_transport_compio` | 327 | 320 | 7 | 9 | |
| `store_hnsw` | 288 | 288 | 0 | 0 | |
| `error_cluster` | 261 | 255 | 6 | 0 | |
| `core_algo` | 237 | 236 | 1 | 0 | |
| `error_role` | 34 | 34 | 0 | 0 | |
| `supervisor` | 418 | 0 | 418 | 0 | |
| `frontend` | 207 | 0 | 207 | 0 | |

`common_compute`'s suite fails under Cargo with the default thread count: 32 of its 128 tests time out on the 2 s and 5 s
bounds of the played frontend and partitions ("the frontend's placement map did not come within 2s") while the
instrumented build runs every test at once beside three other coverage runs. With `-- --test-threads=2` every test passes,
135 with the ones added here, so it was measured by `cargo llvm-cov -p ultragraphdb_common_compute --all-targets --lcov
--ignore-run-fail -- --test-threads=2`. Bazel passes it unchanged.

#### Tests added

- `common_config`: `config_api_tests.rs`, every refusal of the store answered with its status and the error's text (an
  unknown target, no active revision, an unknown revision or instance, a stale base, a refused bundle, a retired target),
  the 400s and the 404 of a request the API cannot read, a change the log fails to make durable answered 503 (a fault at
  the next storage operation), and every route answered 503 once the store's owner has ended; `config_store_tests.rs`, a
  record not following its state (a skipped revision, an activation of a revision not stored, a retirement of a target
  that does not exist) diverging at its position, `active_defects` checking only an active revision and naming a stored
  bundle that no longer reads as one defect, an activation changing what the active revision fixes and a retirement of
  an unknown target refused; `utils/store_owner_tests.rs`, the owner ending with its handle. The API tests' key files
  moved to a folder per test: two tests writing one key file at once raced, one reading it while the other truncated it.
- `common_service_config`: `service_config_tests.rs`, a value out of range and one of the wrong type recorded by
  `problems` and the reading going on, each the refusal `parse` stops at.
- `protocol_transport_compio`: `carrier_tests.rs`, a split carrier over TCP and over a Unix socket;
  `tls_read_half_tests.rs`, a read with no room, `close_notify` read as the end, a stream closed without it read as
  `UnexpectedEof`, a failed stream read, bytes that are no record; `tls_write_half_tests.rs`, a write after the half's
  shutdown refused, a failed stream write; `compio_tls_stream_tests.rs`, a read dropped while it waits leaving the stream
  readable (the branch of `read_stream` that reserves the buffer again). `tests/utils/probe.rs` gained switches for failed
  reads and writes and for bytes that are no TLS record.
- `cluster_metadata`: a tenant's creation and a drawn secret round trip, a byte short refused (`every_kind` and the fuzz
  seeds leave both out); a tenant held and a secret drawn are appended once; at most 2^20 tenants (1,048,576 inserts, 1.6 s
  for the whole target under Bazel); a report on a version not held and on one in effect that was never a tightening
  change nothing.
- `store_quads`: the catalog's declarations described by kind (an edge, a predicate of either kind, an entity type, a
  property with no datatype on its domain); `snapshot_objects` leaving out the write's own writes, `discard_lists`, and
  `quads`; a value of a page's own past the store's last key.
- `common_compute`: `core_listener_tests.rs`, a TCP and a Unix listener every core takes a handle of, a stale socket
  replaced and the new one at mode 0600, a file standing at a Unix address refused and left; `literals_tests.rs`, a
  language-tagged object with no direction and with each; `access_tests.rs`, a bound object alone reading its predicates'
  list and each reverse list at the partition its referrer set names (`played_data.rs` gained `object_predicates`);
  `read_request_tests.rs`, a slot bound to a term it cannot hold refused with that term's index as detail;
  `partition_rows_plan_tests.rs`, a bound predicate or object the partition does not name answering no row.

#### Listed, with reasons

**`supervisor`, all 418 lines** (`main.rs:19-25`, `service.rs:87-650`) and **`frontend`, all 207 lines** (`main.rs:20-31`,
`service.rs:50-293`): each is a binary of `main.rs` and `service.rs` with no library and no test target of its own, so
cargo-llvm-cov counts nothing there. Its lines run in the process scenarios of its sister crate, which start the built
binary: `ultragraphdb_supervisor_tests` (`features/role_binaries/supervisor.feature`: the start order, the exit codes of a
failing or signalled role, SIGTERM, SIGINT, SIGHUP and SIGQUIT, a signal during the start, the secret drawn once, two
role secret files, a bootstrap password removed, the config service's refusals, a killed supervisor orphaning no role;
`io_uring.feature`) for `read_start`, `draw_secret`, `supervise`, `one_role_secret`, `tell`, `until_stopped`,
`stopped_code`, `code_of`, `observability`, `active_target`, `not_one_target` and `asked`; `ultragraphdb_frontend_tests`
(`features/role_binaries/role_binaries.feature`: a frontend without its role address, a port held a moment and past the
bound, any other bind error, `/health` once bound, epoch closes reaching storage, a partition's lease and placement;
`client-endpoint/tls.feature`) for `frontend_role`, `endpoint_limits`, `client_server`, `owner_start`, `accept_roles` and
`accept_clients`.

**`error_cluster`, `cluster_error_display.rs:118, 171, 246, 305, 374, 435`**: the fallback arm of each of the six private
text helpers. `fmt` passes each helper only the variants it writes, so the arm naming the others never runs.

**`core_algo`, `packed.rs:204`**: `merge_sorted_packed`'s pop of a heap element naming no decoder; each element is pushed
together with its decoder and position (as group 2 recorded).

**`cluster_sequencer`, `sequencer/mod.rs:152, 171, 179`**: the `?` of three `invariant` checks (a round appended after the
tail, a close one epoch after the last, close times never falling, a transaction in the epoch after the last close),
each holding by construction of `append` and `hold`. `sequencer_snapshot/mod.rs:119`: the end of the `if let [before,
after]` over `windows(2)`, which always yields two.

**`cluster_metadata`**:
- `metadata_command_codec.rs:159, 212`: the arms of the commands `encode` writes itself and never passes to the helper.
- `metadata_group/mod.rs:96, 99`: `sync`'s breaks for a staged command past the durable position and for an empty
  queue after `front` found one; the log's `sync` makes every appended round durable, and `front` and `pop_front` read one
  queue.
- `metadata_snapshot_codec.rs:461`: `shifted`'s arm for a placement-map refusal other than `Malformed`;
  `PlacementMap::decode` refuses every byte string as `Malformed`.
- `metadata_state_apply.rs:245`: `report_check`'s return for a record `record_mut` lacks after `version_record` found it;
  both read one map by one key.
- `metadata_state_apply.rs:312-315`: the bound of 2^20 default-entry records. Reaching it holds 2^20 records each with a
  predicate string and an entry, some 100 MB, cloned once more by the check that a refusal changes nothing; the bound is
  the `len() >= MAX` of the tenants' bound tested above.

**`store_quads`**:
- `owned_buckets.rs:73`: `draw`'s trailing `None`; the drawn index lies below the owned count (as group 3 recorded).
- `quad_store_partition.rs:222, 328`: a store read failing during `page`'s walk and during `values_page_over`'s scan;
  only a failing store reaches them, which no test of this crate injects.
- `quad_write/mod.rs:396-401, 407`: `ids` meeting a posting that names no system ID; only a corrupt list holds one.
- `list_objects.rs:44-46, 52`: the posting crate's error while a list is iterated, the same corrupt or failing store.

**`protocol_transport_compio`**:
- `compio_tls_stream_async_read.rs:45-46`, `tls_read_half.rs:77-78`: the bound of 65,536 reads with no plaintext.
  Reaching it takes a peer sending 65,536 records that each decrypt to nothing, which `TlsSession` offers the tests no
  way to send; a record never complete ends at its 16,389th byte, well short of the bound.
- `connection/mod.rs:120-121`: commented "Not reached": after its header the framer asks for the frame's size once.
- `listener/mod.rs:76`: a Unix listener bound to no path; `Listener::bind` binds a Unix socket only at its address's
  path, which an `Address` holds as UTF-8.

**`client_core`, 38 lines in wave 3; 19 after wave 3b, each unreachable by construction.** Wave 3b
(`target/llvm-cov-target-b0`: 2,246 → 2,287 added, 1,293 instrumented, 199 tests and the scenarios passing) added:

- `client_core_requests_tests.rs`: a transaction of another client, naming a stream of this one that holds no transaction,
  refused as finished.
- `client_core_results_tests.rs`: a read cancelled once read to its end sends no Cancel; a Result of more columns than a
  pattern binds gives the connection up with the rule.
- `client_core_tests.rs`: a ConditionalWrite answered busy sent again whole (`Resend::Conditional`); a one-frame write lost
  with its connection, too large for the next connection's Welcome, in doubt with `FrameTooLarge` as its cause (the harness's
  server replaced by one of a smaller `max_frame` between connections); a commit not yet written when its connection times
  out answered `connection_lost(timed_out)`, its session's number kept.
- `utils_test/test_server/`: a commit through no session lost before the commit step; a one-frame write of a session the
  server does not hold refused with code 12; a ConditionalWrite holding a declaration refused with code 1; a Prepare of a text
  refused; a Begin with a read version on a read-write transaction left unanswered, the client's wait timing out; a Welcome
  whose name passes the largest frame dropping the connection with no frame; a Read naming a transaction not open refused on
  its own stream.

Listed:

- `client_core_receive.rs:95`: a Read-placed frame of a stream not held; `place` returns `Read` only for a stream held, and
  nothing between removes it.
- `client_core_receive.rs:235`: `authenticate` on a link down or in its handshake; `receive` returns at `Down` and
  `connection_frame` takes the handshake's frame before it reaches `authenticate`.
- `client_core_receive.rs:332`: `settle_register`'s pending gone; `answer` found it at its start and nothing between removes it.
- `client_core_receive.rs:420`: a commit through a session whose state is gone. A session leaves the table only by code 12's
  answer to its request in flight, and `next_number` keeps one request in flight per session.
- `client_core_receive.rs:521`: `refused_detail` of an error other than `Refused`; it is asked for code 13 alone, which only a
  `Refused` carries.
- `client_core_requests.rs:131`: the end of `send_counted`'s `debug_assert_eq!` block for a request with no session number;
  its four callers send commits, one-frame writes and drops alone.
- `mod.rs:246`: `connect_failed` meeting an unanswered request with no stream and no resend time that is no commit; `lose`
  answers every request but a commit, and a Register waiting to be sent again holds a resend time.
- `mod.rs:571, 590`: `frame` and `open_stream` while the link is not up. Streams exist only while it is up (`lose` takes them),
  `new_stream_frame` checks `is_ready` first, and `send_commit` runs from `handle_timeout` under `is_ready` and from
  `resend_commits` once the link is up.
- `mod.rs:629`: `resend_frame`'s pending gone; `send_commit` read it just before.
- `utils_test/test_server/mod.rs:354, 375`: the closing Error not built at a timeout or at a header's refusal. Each is a fixed
  short text or a rule's text, which always builds.
- `utils_test/test_server/mod.rs:454`: a framer or pool error other than a rule's. `receive` holds one of the pool's 4 buffers
  at a time, and the framer refuses by a protocol rule alone.
- `test_server_commit.rs:220`: a commit result naming no node; `apply` makes every result for a node.
- `test_server_requests.rs:127`: the role commands; the client endpoint's connection refuses commands 512 to 767 before they
  reach the server.
- `test_server_requests.rs:242`: an empty two-column Result past credit not built; it always builds.
- `test_server_requests.rs:321-323`: a row of a term and no reifier not fitting a two-column batch, as the comment says.

**`common_compute`, 211 lines in wave 3; 42 after wave 3b.** Wave 3b (`target/llvm-cov-target-b0`, `--test-threads=2`
as above: 5,113 → 5,182 added, 2,910 → 2,911 instrumented, 212 uncovered at its start, 158 tests passing) wrote 23 tests. The harnesses gained: the played frontend's `delay_placement` and `list_compute`; the played-reads
partition's `ClosedAnswer` (silent, a refusal, a frame of another command, a Closed whose body breaks its checksum) and
`answer_closed_with`; `run_read_tests.rs`'s `spread_by`.

- `compute_core_tests.rs`: a frontend answering Placement or Members past the answer bound; a partition the Members does
  not list, lists at an address that does not parse, or lists where nothing listens, each refusing the read and the next read
  asking the Members again; two reads arriving during one dial sharing it; a Members listing compute processes beside the
  partitions. `dial_under_way_tests.rs`: a read waiting on a dial its dialling read gave up dials for itself.
- `compute_core_lookups_tests.rs`: a partition not naming the lookup's predicate skipped, its list answered a key per frame
  read to the last, for both lookups; a term's entry holding no ID, an ID's entry holding no text, and a value index listing a
  literal each refusing the lookup with code 17.
- `access_tests.rs`: an unbound graph walked in the keyspace of each access (subject alone, predicate and object, object
  alone, reifier).
- `partition_rows_plan_tests.rs`: a bound graph the partition does not name answering no row; a read stopped by its limit
  in a named graph resumed there, the graph bound and walked.
- `partition_rows_tests.rs`: what a partition holds that does not read refusing the read with code 17 (a term's entry, an
  ID's entry, a list, a key in a list's range, a referrer set, a list naming an ID the dictionary lacks, an object's encoding,
  a key under a walked keyspace); a referrer set answered over two frames read to its last.
- `partition_rows_fill_tests.rs`: a Scan its limit stopped at keys giving no row goes on from the key past its last.
- `row_terms_tests.rs`: a language-tagged literal held by hash comes back by its texts, and a read resumed at a string, a
  language string of no direction or either, a known datatype's literal not in canonical form, or a literal of a datatype held
  by ID goes on after that row.
- `run_read_tests.rs`: a resume key carried to another pattern or another version refused before any partition is asked;
  a monotonic-fast read refused when a partition fails its Closed (silent, refusing with its own code and detail, answering
  with another command, a Closed that does not read); a reverse read at a named version reading the home's set at the latest
  version, one at monotonic-fast level reading the member at the home's epoch, and an object the home does not name answering
  no row.
- `resume_terms_tests.rs`: a resume key naming its graph by a literal, or its object by a blank node, refused.
- `literals_tests.rs`: a literal of a datatype the partition does not name answering no row, asking no list.
- `scan_stream_tests.rs`: a partition silent past a Scan's wait refusing the read, the next read dialling anew; a Scan
  waiting when another read's frame closes its connection refused, "closed under the read".
- `result_sink_tests.rs`: a row past the protocol's largest frame refused with code 6; a read waiting for credit ending
  with its client's connection.
- `read_pins_tests.rs`: a partition refusing the Scan holding a read's version: the read goes on and answers.

One test of these first failed only under the coverage run's load: the Closed test with a 100 ms answer bound, where the dial
itself outlasted the bound. Each case now dials with a first read and runs at a 1 s bound. The frontend and silent-Scan tests
were widened the same way, to a 1 s bound.

By reason:

- Unreachable by construction (23):
  - `access.rs:140`: `listed_key` is asked only of the accesses whose lists name predicates.
  - `compute_core_lookups.rs:267`: a key of the scanned range that is not a count-index key; the range runs from the
    predicate's count 0 to `u32::MAX` within the count-index keyspace.
  - `compute_core_lookups.rs:286`: `parts.get(at_part)` with an index `enumerate` gave over `parts`.
  - `compute_session.rs:104`: the session holds a sender, so its channel never ends.
  - `frontier.rs:75`: `less` is asked only of indices in the heap.
  - `partition_rows/mod.rs:225`: a found key not among the gate's terms; each range is one key, `past_alone` of it.
  - `partition_rows_fill.rs:68, 302`: `next_graph` and `expand` with no access. `plan` sets one, or ends the partition, before
    any fill.
  - `partition_rows_fill.rs:184`: `pop_front` finding nothing just after `front` found a range.
  - `partition_rows_fill.rs:209`: `next_frame` with no Scan open; `fill` calls it only while one is.
  - `partition_rows_fill.rs:307`: `listed_key` naming nothing; `expand` runs only for the accesses that list predicates.
  - `partition_rows_terms.rs:247`: a row with no subject and the subject unbound. Only a statement's row has no subject, and
    a statement is read with its reifier bound.
  - `read_route.rs:117`: a subject naming no bucket; `iri_bucket` names none only for an IRI past `u32::MAX` bytes, which no
    frame holds.
  - `result_sink.rs:109`: `finish` meeting a resume key past the frame with no row held. A key comes only after a row, which
    `push_row` leaves in the batch.
  - `result_sink.rs:139-142`: the room's channel ending under a read. `OpenRead` declares the read's task handle before the
    room's sender, so dropping it drops the task first. `test_a_read_waiting_for_credit_ends_with_its_connection` ends the
    connection under a read waiting for room and covers no line here.
  - `row_terms.rs:73`: a row of a keyspace no read scans for rows.
  - `row_terms.rs:81`: `text_of` given an inline text; `request_term` passes it hashed texts alone.
  - `run_read.rs:285`: a reverse route's object that is no IRI; `ReadRoute::of` routes in reverse only for an IRI object.
  - `run_read.rs:462`: the merge's order of a part with no row; the frontier holds only parts whose page holds one.
  - `run_read.rs:487`: `parts.get_mut` of an index the frontier gave over `parts`.
- A stored value that does not decode, which only a corrupt store holds (5): `names.rs:76` (an object's encoding that does
  not read names nothing; the read is refused at its row, as `partition_rows_tests.rs` shows, without this line);
  `partition_rows_terms.rs:245` (a reverse list naming a subject by no IRI ID); `row_terms.rs:147` (a stored literal naming
  its datatype by hash); `row_terms.rs:196-197` (a resume key's literal whose datatype ID the dictionary lacks).
- Open, reachable in process and not staged here (14); none has one of the kinds of reason this part accepts.
  - `link.rs:232, 238, 242, 245`: the connection's reader failing, a frame out of sequence, a frame on stream 0, an Entries
    frame past its stream's credit. The played partitions answer through `serve_connection`, which numbers each frame, meters
    it by credit and sends nothing on stream 0, so staging these takes a partition writing raw frames.
  - `link.rs:210`: a write failing; the reader sees a dropped connection first in every scenario played here.
  - `link.rs:157`: a second `close` of a closed link. A read would have to take a refused frame delivered before another
    read's frame closed the link.
  - `link.rs:109`, `read_pins.rs:101, 116`: a Scan opened on a link that has closed. A read would have to sit between two of
    its own Scans, its rows held at its client's credit, while another read's frame closes the link. The attempt
    (`link_tests.rs`, removed) failed in two ways. The listed keys of a subject's predicates go out as one Scan. The apart
    partition sends its frames in one queue, so a frame waiting for credit holds back every frame behind it.
  - `partition_rows_fill.rs:152`: a walk's Scan answering a frame with no key ahead of its last. No played partition sends one.
  - `partition_rows_fill.rs:286`: a stall after one range of a Scan ended and before the next gave a key; the apart
    partition's stall scripts stall on the first range's first key.
  - `partition_rows_frames.rs:93`: a read resumed after a stall inside a list whose records hold statement IDs. The apart
    partition, the one that stalls, drops statement IDs from its records.
  - `partition_rows_terms.rs:253`: a statement row with no property and the predicate unbound; not shown unreachable here.
  - `run_read.rs:55`: a refusal whose frame does not build. Reachable by reading, not tested: a resume key naming an unheld IRI
    near 1 MiB refuses with a message holding that IRI. The Error frame would then pass 1 MiB, and the read would end with
    no frame on its stream. Open for the lead as a possible defect.

#### Defect found

None in `src/`. One in the tests: `config_api_tests.rs`'s `key` wrote both key files into one folder per process, so two
API tests running at once could read a key file the other had just truncated ("holds 0 bytes"); each test now has a folder
of its own.

### Part 3: storage, apply, commit, the role binaries' common crates, core, transport and the test tools

`scripts/coverage.sh <crate>` per crate, each run with `CARGO_LLVM_COV_TARGET_DIR=target/llvm-cov-target-3`. The lines
that count are those `git diff -U0 1f4b67a3` adds under the crate's `src/` in the working tree, less those `git blame`
gives to 08ff8f26; "instrumented" is the share of them llvm-cov counts at all (a comment, a signature or a lone `}` it
does not). The scripts are `cov3/added.py` and `cov3/intersect.py` in this wave's scratch directory. The last pass ran over the tree as
it stood once this wave's other agents had edited it, so the line numbers below are that tree's.

| Crate | Added | Instrumented | Covered | Listed | Before this pass | After wave 3b |
|---|---|---|---|---|---|---|
| `tool_bdd` | 171 | 62 | 62 | 0 | 13 |  |
| `core_ids` | 857 | 271 | 271 | 0 | 0 |  |
| `core_mapping` | 695 | 273 | 272 | 1 | 58 |  |
| `core_types` | 1,094 | 785 | 771 | 14 | 66 |  |
| `core_catalog` | 660 | 368 | 355 | 13 | 13 |  |
| `transport_http_client` | 574 | 264 | 263 | 1 | 13 |  |
| `transport_tls` | 1,533 | 612 | 603 | 9 | 12 |  |
| `common_service` | 1,325 | 596 | 575 | 21 | 44 |  |
| `config` (binary) | 254 | 156 | 0 | 156 | 156 |  |
| `compute` (binary) | 319 | 199 | 0 | 199 | 199 |  |
| `store_apply` | 3,541 | 1,880 | 1,826 | 54 | 61 | 18 of 1,880 (3,622 added), all unreachable |
| `transactions_commit` | 2,879 | 1,580 | 1,552 | 28 | 28 | 13 of 1,586 (2,968 added), 12 unreachable and 1 an open defect's |
| `common_storage` | 4,435 | 2,616 | 2,397 | 219 | 292 | 48 of 2,619 (4,487 added): 41 unreachable, 7 OS failures |
| `tool_test` | 3,199 | 2,087 | 1,352 own, 1,848 with its users' suites | 239 | 735 | 151 of 2,090 (3,214 added); 1,749 own, 1,939 with its users' suites |

Every suite passes under Cargo: `tool_bdd` 27 tests and 1 scenario, `core_ids` 72 and 10, `core_mapping` 66 and 14,
`core_types` 345 and 22, `core_catalog` 57 and 5, `transport_http_client` 21, `transport_tls` 62 and 10, `common_service`
49, `store_apply` 313 and 93, `transactions_commit` 260 and 99, `common_storage` 144, `tool_test` 6; the two binaries
hold none. Under Bazel every touched crate's targets pass, the four `store_apply` workload targets included.

#### Tests added

- `tool_bdd`: `tests/traits/context_tests.rs` (a present value, an absent one naming the caller's line, a successful and
  a failed result), `tests/types/test_error_tests.rs` (conversion from an error, through `?`, `Display`), and
  `test_an_unlistable_directory_fails_naming_it` in `fuzz_regressions_tests.rs`. Neither `Context` for `Result` nor
  `TestError`'s `From` and `Display` had a test.
- `core_mapping`: six tests in `change_ref_tests.rs`, one per `Change` variant `as_ref` borrows (only `InsertEdge` had
  one), and `test_a_triple_term_object_is_no_edge_and_its_subject_is_a_node` in `property_graph_view_tests.rs`, the
  statement built directly since a dataset refuses a triple-term object.
- `core_types`: `tests/utils/xy_tests.rs`, the planar ray test of a point query's `contains` against exact integer
  geometry: the point at the origin, whose degrees round-trip exactly, in each of the 6,561 triangles whose first two
  vertices lie on the 9 × 9 grid around it and whose third is (0, 5), compared with integer orientation tests. A Python
  model of `sign_of_det2x2` showed that family reaches every branch the ray test can call. Also five inputs added to
  `INVALID_DATES` (a day, an hour, a second that is no number, an offset that is none, an offset without a sign); two
  geofilter tests (a geometry with no area cannot be queried with, and is neither within nor intersecting); two GeoJSON
  tests (a geometry without coordinates is the empty geometry of its type, coordinates that are no array are refused
  naming the Go type); four conversion tests (`null` GeoJSON to nil, a non-finite geometry to a string is nil, a
  mismatched value refused naming both types, a marshal to a type without that form refused).
- `transport_http_client`: `tests/errors/http_client_error/http_client_error_tests.rs` (each failure at a URL names it
  and its reason, and only what waiting mends is unreachable); `test_a_client_names_its_url`;
  `test_a_silent_peer_is_timed_out`, which waits out the exchange's 5 s; a head ending past its 16 KiB bound, and a
  status line without a status, refused.
- `transport_tls`: `test_records_past_the_unread_plaintext_a_session_holds_are_refused` (rustls holds 16 KiB of unread
  plaintext; four records' worth taken in one `receive` is refused as a record past the handshake, which is why every
  carrier hands a session at most `RECORD_SIZE_MAX` at a time) and `test_a_read_into_no_room_is_pending`.
- `common_service`: `tests/types/admin_service_tests.rs` (404 for what is no load, 401 without the key, 400 for a body
  naming no revision, 409 while another loads, 422 for a revision the config service refuses, 503 for one it cannot be
  asked for); `test_a_unix_socket_is_private_and_a_stale_one_replaced` (mode 0600, the socket a dropped listener left
  replaced); `test_an_admin_listener_s_refused_settings_stop_the_start` (no key, a key that is no digest, TLS files that
  do not load: each 78, naming the setting); `test_a_start_naming_no_admin_listener_serves_none`;
  `test_no_parent_named_is_no_parent_watched`.
- `store_apply`: `test_a_node_s_id_is_the_store_s_or_a_new_one_kept_for_the_execution` (`Execution::node_id`, which
  only `transactions_txn` called) and `test_reads_of_what_the_store_does_not_hold_find_nothing` (an unknown graph or
  predicate, a value that is no literal, a value of two tokens intersecting their keys).
- `common_storage`: `test_every_other_refusal_takes_its_code` (17 refusals `error_reply` maps and no test named),
  `test_a_changeset_s_failed_compare_and_set_carries_the_value_found` (a blank node, both base directions, a triple term
  naming nothing), `test_a_scans_refusal_names_the_key_through_every_carrier`;
  `test_a_transactions_operation_is_held_and_its_result_committed` (an Operation frame in a bound transaction: Held, then
  the Commit carrying its one result; no test sent one); `test_a_range_naming_no_tenant_or_keyspace_or_a_wrong_after_is_refused`.

No defect was found. The TLS session's refusal of more than 16 KiB of unread plaintext is rustls's bound and no caller
meets it, since each passes one record's bytes at most; the test pins why.

#### Listed

**`core_mapping`, 1.** `types/random_source.rs:61`, the draw after 128 rejected ones: probability below 2^-128.

**`core_types`, 14.**
- `utils/xy.rs:156-158`, `order_by_y`'s branch for both y coordinates at or below zero: the ray test calls
  `sign_of_det2x2` only for a segment straddling the point's horizontal, so one y is positive.
- `types/date_time/date_time_parse.rs:387`, `parse_nanoseconds` failing: its caller passes a `.` or `,` followed by
  digits, at most nine of which are read, which `atoi` always takes.
- `date_time_parse.rs:489-493`, a time with a zone abbreviation and no numeric offset: Dgraph's one layout naming `MST`
  names `-0700` before it, so a parsed abbreviation always comes with its offset, which `finish` returns first.
- `utils/conversion.rs:372-373`, a `BigFloat` quotient of zero by zero: the divisor is 1e9.
- `s2/loop/mod.rs:408`, `445`: an index position holding no clipped shape 0. Both are reached only after the iterator
  located an indexed cell, and the index holds one shape, whose clipped shape every cell it creates carries.
- `s2/regioncoverer/mod.rs:437`: no best index while the best level is at or above the minimum. The best level is -1
  exactly when no index was found, and the coverer's minimum level is clamped to at least 0, so the loop breaks first.
- `s2/shapeindex/tracker.rs:137`: removing a shape found past the first position. The index tracks one shape, ID 0, so
  the set holds at most that one.

**`core_catalog`, 13.**
- `utils/catalog_codec.rs:59`, `68`, `101-102` and `types/catalog_contents/catalog_contents_codec.rs:52-58`, `69-70`:
  a string or an entry past `u32::MAX` bytes, or more than `u32::MAX` declarations. A test would allocate past 4 GiB.
- `catalog_contents_codec.rs:39`, `reason_of`'s other arm: every refusal the entry decoder raises is the store's
  `Corrupted`.
- `errors/catalog_error/catalog_error_display.rs:115`: `encoding` is called for `CorruptedVersion` and `TooLarge`
  alone, each displayed by its own arm.

**`transport_http_client`, 1.** `types/http_client.rs:189`, a reply ending with no byte after a failed write. On macOS
the read after the failed write returns the connection's reset, which line 193 reports; the end-of-stream case depends
on the kernel's ordering, so no test pins it.

**`transport_tls`, 9.**
- `types/tls_stream/tls_stream_read.rs:29-31`, no plaintext after `READ_TURNS_MAX` (65,536) reads: the bound of rule 2,
  reached by a peer sending 65,536 records that carry no plaintext.
- `utils_test/test_pipe.rs:46`, `63`: a lane past the two `pair` builds.
- `utils_test/test_pki.rs:87`, `188-190`: rcgen or rustls refusing the authority, key and configuration the test PKI
  generates itself, the TLS 1.2 configuration from the chain and key the first one already took.

**`common_service`, 21.**
- `utils/run_service.rs:96`, `run_service` with no `ULTRAGRAPHDB_PARENT_PID`: past `Bootstrap::read` a role then serves
  until a signal, which an in-process test cannot send; every role binary started outside the supervisor takes it, e.g.
  `ultragraphdb_storage_tests`' `features/config-service/bootstrap.feature`.
- `run_service.rs:111`, `138-151` (`restart_at`): the role executes itself again, which replaces the process;
  `ultragraphdb_storage_tests`' "A load restarts the node at the revision" (`features/config-service/load.feature`).
- `run_service.rs:222-224`, a role not ending within `SHUTDOWN_POLLS`: a test would wait out its 60 s.
- `run_service.rs:312`, `338`: a binding to a CPU, which reads `/proc/thread-self/status`; Linux only, and this pass ran
  on macOS.
- `run_service.rs:320`, `driver_name` inside `tracing::info!`: evaluated only with a subscriber, which `run_service`
  installs once per process and no in-process test does.
- `run_service.rs:367`, a failed accept on the health listener: an operating-system failure.

**`config`, 156, and `compute`, 199: binaries, reached by process scenarios alone.** Neither has a library; cargo-llvm-cov
counts no process a scenario starts.
- `config` `main.rs:17-20` and `service.rs` `run_config_service` 51-60, `listing` 62-68, `read_start` 70-124,
  `read_targets` 127-137: `ultragraphdb_config_tests`' `features/config-service/service.feature`, its start refusals
  ("A broken declared file stops the start", "No central file stops the start", "A key too short stops the start", "A bad
  config-service file stops its start", "Every absent file is named at once"). `lock` 140-163: "A second process over
  the directory is refused"; its 160 (`TryLockError::Error`) is an operating-system failure. `serve` 165-234: every
  scenario that starts the service, "A restart holds what was acknowledged" for recovery and seeding, and the TLS arm
  200-212 `ultragraphdb_storage_tests`' "A role reads its settings over TLS"; 57-58, the runtime refused, is an
  operating-system failure.
- `compute` `main.rs:20-31` and `service.rs` `compute_role` 41-62, `required` 64-66 ("A compute process without its
  address stops the start"), `join_as` 68-119 and `attempt` 263-288 ("Compute is ready once a partition has joined",
  "Compute refuses a role with another secret"), `start_cores` 133-158 and `accept_on_core` 161-185 ("Compute serves
  reads alone" and `features/quad-reads/compute_reads.feature`), `hold_peers` 188-212, `hold` 217-223, `answers`
  227-252, `now` 258-260 ("A lost dependency exits with its code", "A peer that stops answering without closing is a
  lost dependency"), all in `ultragraphdb_compute_tests`. Of these, 155 (a core's thread not spawned), 165-166 (a core
  that does not listen) and 182 (a failed accept) are operating-system failures.

**`store_apply`, 54 in wave 3; 18 after wave 3b, every one unreachable by construction.** Wave 3b
(`target/llvm-cov-target-b3`; 333 unit tests, the four workload targets and the scenarios passing under Cargo, the
crate's 7 Bazel targets passing) added:

- `tests/types/changeset_write/changeset_write_read_tests.rs` (new): a blank graph the write did not name and a skolem
  predicate the store does not hold find nothing, for `subjects_into` and `lookup_into`; an integer gives `term` no token
  and finds nothing; a stored blank subject reads back as its blank node; a list the write changed answers a lookup by its
  literals, its IRI passed over.
- `tests/types/changeset_write/changeset_write_logical_tests.rs` (new): a logged predicate ID naming a blank node is
  refused as the store's corruption naming the ID.
- `tests/types/changeset_core/changeset_core_logical_tests.rs` (new): a replay whose inherited entries lack `rdf:type`
  resolves it itself and writes its own entries.
- `changeset_core_catalog_tests.rs`: a version taking effect leaves out another partition's entity type and relation;
  a write naming a held predicate by its given ID follows the held declaration (its `exact` index); a statistics key that
  does not parse is passed over by `reconcile_defaults`.
- `changeset_core_rebuild_tests.rs`: a count index built over literals; a count build passing over a reverse key that
  does not parse; a swap passing over a value-index key whose graph field names no graph; a vector index rebuilt over one
  the indexer wrote deletes the dead list it does not write; a build failing on a lost dictionary entry is undone; a
  vector build passes over a value that is no vector.
- `changeset_core_record_tests.rs`: a crashed build of a predicate with a datatype is built again at open, datatype kept.
- `changeset_core_operate_tests.rs`: an operation linking to a node with a damaged referrer set is refused with the set's
  corruption; a node an unapplied operation created is found through the overlay without its dictionary entries.
- `changeset_write_statements_tests.rs`: a triple a named graph does not hold is refused naming its graph; deleting a
  statement whose record names a blank predicate removes the record.
- `execution_tests.rs`: a page of the referrer sets reads the stored sets and records its span.
- `tests/utils/referrer_candidates_tests.rs` (new): a replayed edge joins its object's referrer set; a list under a key
  that is no forward list's, and a reference posting naming no system ID, name no candidate.

Listed:
- `changeset_core/changeset_core_rebuild.rs:259`: a forward object that is no ID decodes to a literal: an object that is a
  triple term is refused at the write (`MappingError::unsupported_object`).
- `changeset_core_rebuild.rs:450`: `swap_vectors` runs only under `plan.build_vector`, which `IndexPlan::of` sets only
  when the new declaration has a vector index.
- `changeset_core/changeset_core_record.rs:148-149`: `build_again` of a predicate the catalog no longer declares:
  `finish_records` names a predicate to build again only when the same catalog declares it, and nothing between changes it.
- `changeset_core/changeset_core_referrers.rs:72`: every entity pushed to `changed` was first held in `sets` by `held_set`.
- `changeset_core/mod.rs:235`: the invariant that a round of one execution gives one commit.
- `changeset_write/changeset_write_read.rs:196`: a key the write wrote under a forward list's prefix is a forward key it
  built, which parses.
- `changeset_write_read.rs:221-225`: an ID decodes to an IRI or a blank node alone (`ChangesetWrite::decode`,
  `QuadStore::iri_of`).
- `types/logical_quad.rs:87-89`: the second "shorter" arm: the first takes every key of 48 bytes or fewer, so the splits at
  16 and 32 bytes always hold.
- `types/overlay_txn/mod.rs:65`: nothing writes an empty value to the overlay: the index writes encoded edges, entries and
  dead lists.
- `overlay_txn/mod.rs:109`: no node passing: the overlay's `find` runs within the insert of a node whose vector
  `insert_built` read through the same overlay at the same timestamp, so that node passes.
- `utils/catalog_keys.rs:25`: `declaration_key` is called for entity types and relations; `declare` returns for a predicate
  before it.

**`transactions_commit`, 28 in wave 3; 13 after wave 3b, every one unreachable by construction but the last.**
Wave 3b (266 unit tests and the scenarios passing under Cargo, the crate's 2 Bazel targets passing) added:

- `commit_step_epoch_tests.rs`: a registration behind the epoch's last commit waits for the close and takes the next
  epoch's first version.
- `commit_step_operations_tests.rs`: operations leaving a record that does not read are refused malformed, naming why,
  what they left let go, the next transaction committed (the test operator `Script` gained `garbled`).
- `tests/types/no_operations/no_operations_operator_tests.rs` (new): `NoOperations` refuses every operation and stages
  and keeps nothing.
- `replay_tests.rs`: an undone build replays as its undo, the replica's catalog the first apply's and no index key left;
  a log reading past the position asked refuses recovery naming the gap (`ShiftedLog`, a log double).

Listed:
- `applier_operator/applier_operator_operator.rs:104-105`: `Prepared::operated` raises the changeset's refusal and
  `WriteSetTooLarge` alone, never `CrossPartition`.
- `commit_step/commit_step_round.rs:422`: the queue is non-empty at the loop's head and nothing takes from it before the pop.
- `types/no_versions.rs:19-20`: replay asks its catalog source for `version` alone, never `version_at`.
- `pipeline/mod.rs:317`: `Prepared::end_build` is refused by the store or the record's limit: the end's execution reads no
  store, and its record is written under no limit.
- `replay/mod.rs:356-361` and `455`: `WriteSetRecordView`'s kind check refuses a build record holding a drop or other than
  one operation, and a transaction record holding an operation that is no drop, so such a round is refused as malformed
  before it is prepared (`test_a_build_record_naming_a_drop_refuses_recovery_naming_its_operation`).
- `replay/mod.rs:395`: `round` sends a build's record to `prepare_build`, never to `prepare`.
- `session_entry/mod.rs:166`: a stored reply whose results' term table holds an entry that is no IRI. `PROTOCOL.md` §6.3
  makes each entry an IRI and every writer interns IRIs alone (`push_node_result`); see the open defect.

**An open defect, found by reading in wave 3b.** `SessionEntry::encode` (`ultragraphdb_transactions_commit/src/types/session_entry/mod.rs`) writes the IRI entries of the
reply's term table and drops any other, so a reply whose table holds a literal or a blank node before an IRI is stored
with its results naming shifted indexes, and `SessionEntry::decode` refuses the entry as corrupted at the next recovery.
Replay builds the reply from a logged record's results' term table (`record.result_terms().to_table()`), and
`WriteSetRecordView` does not check that table's entries are IRIs, which `PROTOCOL.md` §6.3 requires of the Commit reply's.
No writer produces such a table. The fix is a validation rule, in the record view or in `StoredReply`, which this pass left
to the lead rather than change a fuzzed decoder another agent owns.

**`common_storage`, 219 in wave 3; 48 after wave 3b.** Wave 3b (`target/llvm-cov-target-b2`; 4,435 → 4,487 added,
2,616 → 2,619 instrumented; `ultragraphdb_common_storage_tests` 184 under Bazel, 144 before) added:

- `tests/types/scan_board_tests.rs`: a Scan opened again under its name leaves the waits before it; an epoch wait set again
  waits on the latest alone; a time or an epoch set for a Scan not open is no wait.
- `tests/types/storage_session_tests.rs`: a request to a core whose inbox is gone, and one its core drops unanswered, are
  each refused as unavailable, "the partition stopped".
- `tests/utils/operation_of_tests.rs`: an upsert's links to a blank node and from an IRI are written with the node it
  creates; a literal predicate or subject is refused naming the place and the term's index; a counter in a graph named by a
  blank node commits.
- `tests/utils/scan_check_tests.rs`: a Scan of a transaction reading another tenant's range is refused naming both.
- `tests/types/partition_core/partition_core_frames_tests.rs`: a request failing its body checksum is refused at once, and
  one waiting behind a parked Write is refused in its turn; a refused Write ends its transaction; an Operation needs its
  Bind, and a refused one ends it; an Operation waiting on its home is held once answered; a Write behind a read parks
  again on its own home; a parked Write refused once answered ends its transaction; a Drop on a transaction's stream ends
  it; a Commit its round refuses (a request below the session's last) is told on its stream.
- `tests/types/partition_core/partition_core_one_frame_tests.rs`: a resent DirectWrite and a resent Drop are answered from
  the session table at the version they committed at; a DirectWrite whose first subject another partition owns is refused
  as a stale route; a parked DirectWrite refused once answered is told so; a Drop of a predicate, a graph and the whole
  tenant each commits, through no session with no token, and one naming no IRI is refused; a one-frame write run again
  after a conflict that would wait on its home again leaves the conflict to its client, and one run again refused, its
  predicate declared unique since, is told on its stream.
- `tests/types/partition_core/partition_core_reads_tests.rs`: a list larger than a frame continues in the next, its key in
  each; an object record (a vector's) and a term read by its ID, each larger than the reader's frames, are refused as too
  large; a Scan ahead of the watermark is refused once its epoch closes; more Scans than one turn's budget are answered at
  the next (the count read from `TurnBudget`); a Scan of a transaction whose Write waits is refused; a Scan behind another's
  asking asks its own terms in turn; a dictionary range past a Scan's terms is read as its transaction sees it; a page past
  the read set's room ends the transaction on the Scan's stream and the Bind's (some 43,000 new terms over eight Scans,
  1.1 s).
- `tests/types/partition_core/partition_core_remote_tests.rs`: a Reference reply sent as a request is refused; a
  registration here of another partition's entity is refused with outcome 26; a held entity registers by its ID over the
  connection and the address the first registration found; a home the frontend names no address for leaves the write
  waiting.
- `tests/types/partition_core/partition_core_versions_tests.rs`: a contradicted version names the subject at fault, an IRI
  or a blank node; a second build of a version is reported by its predicate; a version is reported by a first entity type
  or relation.
- `tests/types/partition_core/partition_core_tests.rs`: a full epoch asks the frontend for its close.

The harnesses grew with them: `core_rig`'s `RigSetup` takes `full`, the store having applied the last sequence of epoch 1
before the core starts; the remote tests' played frontend lists the home only when `listed`, and the played home accepts a
registration by ID with that ID; `partition_core_tests::owned_by` is `pub(crate)`.

Listed, 48. Unreachable by construction:

- `src/types/one_frame_view.rs:99`: `operation` is asked only below `operation_count`, 0 for a DirectWrite.
- `src/types/partition_core/mod.rs:534`: an outcome of a ticket the core holds in neither `installing` nor `committing`.
  Every ticket the core takes from its host is recorded in one of them (Commit, one-frame write, Drop, catalog version);
  the core never calls `PartitionHost::submit`, and the home settles its registrations' replies before the host hands
  outcomes out.
- `src/types/partition_core/mod.rs:572`: a commit's reply past a frame. The commit step refuses a reply body past
  `MAX_FRAME_FLOOR` (`ultragraphdb_transactions_commit/src/types/commit_step/commit_step_round.rs:690`); a Drop's reply is
  its request with the version and token in its fixed command header; an Error carries its message and at most a found
  value no larger than a write set, 1,040,384 bytes.
- `src/types/partition_core/partition_core_one_frame.rs:274`, `277`: a retry's frame failing `FrameView::validate` or
  `OneFrameView::of`. It is the frame the connection viewed with `validate_client`, which checks all `validate` does and
  more, and whose `OneFrameView` the first run read.
- `src/types/partition_core/partition_core_reads.rs:215`, `256`, `274`: a Scan not open found by its timer, its epoch wait
  or `wake_references`. Waits are set only on an open Scan and forgotten when it leaves the board (`ScanBoard::remove`,
  `take_turn`, `open`); `wake_references` walks keys it filtered by `get`, and starting one Scan touches no other.
- `src/types/partition_core/partition_core_reads.rs:221`: a timer in state References or Queued; it is set only in Epoch and
  Parked and cleared on leaving them, as its comment says.
- `src/types/partition_core/partition_core_reads.rs:349`: an empty turn while turns wait. A key is queued only while open
  and holding no turn, and removal purges its turns, so each of the turns counted at the loop's start is taken.
- `src/types/partition_core/partition_core_reads.rs:372-374`: an Entries frame refused. `EntriesFill` builds within the
  Scan's `max_frame`, at most `MAX_FRAME_SIZE`, and `BodyBuilder::frame` refuses only past 1 MiB.
- `src/types/partition_core/partition_core_reads.rs:401-404`: a Scan holding no range. `ScanView::from_frame` refuses one
  (`ultragraphdb_protocol_wire/src/types/scan_view/scan_view_body_view.rs:180`), and a Scan past its last range is done.
- `src/types/partition_core/partition_core_remote.rs:82`: a registration's term that is no IRI; `ReferenceView::from_frame`
  refuses any (`reference_view_body_view.rs:42`).
- `src/types/partition_core/partition_core_remote.rs:130`: an answer for an asking no longer open. Each registration of an
  asking is answered once, by the host's accept or its home's settle, and the asking ends with its last.
- `src/types/partition_core/partition_core_remote.rs:146`: removes what `get_mut` found three lines before.
- `src/types/partition_core/partition_core_remote.rs:154`: a Reference reply past a frame. The reply is its request with
  each fixed 48-byte registration filled in, so it has the request's size.
- `src/types/partition_core/partition_core_remote.rs:210`: an accepted registration naming no entity or no epoch;
  `ReferenceView::from_frame` refuses both, a close being at most `MAX_EPOCH`, which is `Epoch::MAX`
  (`reference_view_body_view.rs:116-121`).
- `src/types/partition_core/partition_core_versions.rs:124-128`: a contradiction naming a literal. Its subject is a subject
  ID decoded (`changeset_write_data.rs:132`): an IRI or a blank node.
- `src/types/storage_session.rs:113`: the session holds a sender, so its channel never ends while it waits.
- `src/utils/error_reply.rs:258-259`, `271-272`: `failed_key` runs only for code 17, which a changeset refusal takes only
  through a quad, list or store error and a quad refusal only through a list, store or corrupted-key error, each of which
  returns before.
- `src/utils/scan_check.rs:157`: a scan term that is no IRI; `ScanView::from_frame` refuses any
  (`scan_view_body_view.rs:106-115`).

An OS failure no test stages in process (the core runs over fjall and the compio log):

- `src/types/partition_core/partition_core_reads.rs:134-137`, `279-283`: a bound Scan's terms refused, at its opening and
  on waking. The transaction is held there, so the one refusal left is its dictionary read of a term homed elsewhere
  failing.
- `src/types/partition_core/partition_core_reads.rs:559`: an unbound Scan's read refused: the store failing, its pin keeping
  the version readable.
- `src/types/partition_core/partition_core_versions.rs:36-41`: a version the pipeline cannot install. `Prepared::catalog`
  fails only on the store's failure and the round's run only on the log's or the store's; `next_version` returns while a
  build runs, so the rebuilding arm needs such a failure in the run that starts one.

Wave 3 listed as reachable and unwritten what proved unreachable: `scan_check.rs`'s scan term that is no IRI, the
Reference's registration term that is no IRI and its acceptance naming no epoch, a Scan holding no range, and the one-frame
retry's frame failing to view. Each is refused by the wire view before the code is reached, as listed above.

**`tool_test`, 239 in wave 3; 151 after wave 3b.** Measured as wave 3 measured it, its own suite and then with
the suites of the crates that run it (`cargo llvm-cov -p ultragraphdb_tool_test -p ultragraphdb_store_kv -p
ultragraphdb_store_apply -p ultragraphdb_cluster_partition -p ultragraphdb_common_storage`): its own suite, 6 tests in
wave 3 and 22 after wave 3b, covers 1,749 of 2,090 (1,352 of 2,087 before), and with its users' suites 1,939 (1,848).
Wave 3b added:

- `tests/types/changeset_workload/changeset_workload_checks_tests.rs` (new): the changeset workload driven step by step
  over a store damaged behind the applier's back after 40 steps, each damage reported by its rule: deleted value-index,
  reverse, predicate-list and count keys (`INDEXES_MATCH_DATA`), deleted statistics and a statistics key that does not
  parse (`STATISTICS_MATCH_DATA`), deleted records, a record copied under an ID no posting carries and a record
  naming another statement (`RECORDS_MATCH_POSTINGS`), a forward list that does not decode (`NO_UNEXPECTED_ERROR`), and,
  in a replayed run, a store differing from its replica in a key, a key without versions and its metadata
  (`REPLAY_AGREES`). `ChangesetWorkload::replaying()` is new in `src/`, the workload `run_replayed` builds, so a caller
  stepping the workload itself replays it.
- `tests/utils/fjall_workload_tests.rs` (new): `run_both` replays every record on fjall, and reopens fjall across crashes
  between steps; the crate's own suite now reaches `Mirror`, `Run` and the store workload.
- `tests/utils/crash_harness_tests.rs` (new): `acknowledged_rounds` counts the whole lines, refuses a round out of order
  naming both, and a log that does not read.

Listed, 151:
- Unreachable by construction, 12: `changeset_workload/mod.rs:898`, `994`, `997`, `1065`, `1068` (a key of the forward
  lists that is no forward key, or not of the default graph: `lists_of` and `live_keys` read the default graph's forward
  keys alone), `1141`, `1161`, `1189`, `1206`, `1236`, `1258` (a key of a keyspace that is not that keyspace's kind:
  `lists_of(keyspace)` holds that keyspace's keys alone), `257` (the replica's open failing: a fresh simulated store with no
  faults).
- Reports of a rule only the store or the partitions under test can break, and no damage a test does reaches, 97:
  - `mirror/mod.rs` `replay` 132, `compare` 310-329, `same` 340-342, `compare_metadata` 412-427: fjall answering otherwise
    than the simulated store; `run_both` holds both stores, and only a defect in either reaches them.
  - `partition_workload/mod.rs` `ended` 354-363, `check_held` 479-512, `check_mark` 540-543, `check_reverse_reads`
    559-564, `check_scans` 617-623, `check_pages` 645-668, `edges_of` 681, `failure` 758-763, `of` 796: a partition
    breaking a rule; `PartitionWorkload::run` holds its partitions' stores.
  - `changeset_workload/mod.rs` 279, 480-482 (the store and the model disagreeing on a refusal), 540, 558, 578-581 (the
    reverse statements, the statement reads and the model, each checked after the index and record rules, which every
    damage tried breaks first), 712-715 (the replica differing in its applied timestamp alone: a store's applied timestamp
    is its newest commit's, which both replay).
- Arms of a fault or a seed the measured runs did not draw, 35: `changeset_workload/mod.rs` 298-299, 421, 433, 436, 512,
  552, `injected` 772-821; `mirror/mod.rs` 213, 234, 253; `partition_workload/mod.rs` 184, 285, 289-290, 321, 580, 589,
  857, 889, 981-987, 1009-1011, 1018, 1034. Not traced line by line in this pass.
- Process scenarios, 7: `crash_harness.rs` 105-109, 142, 150, the crash writer stopping before its stage or a mode it has
  no stage for: the writer is `ultragraphdb_store_kv`'s `kv_crash_writer` binary, run as a process by that crate's crash
  suites.

The 97 reports and the 35 arms stay open: neither "only a defect in the code under test reaches it" nor an arm left
untraced is one of the kinds this part accepts. Reaching them takes a damage hook in `src/`, a closure `run_both` and `PartitionWorkload::run`
call between steps with the stores, as `ChangesetWorkload::step` already gives a caller; whether the test tools take one
is the lead's call.

## 12.7 Documentation

The four documents now record what the change built, checked against the code and the Cargo manifests
(`cargo metadata --no-deps --format-version 1`, 75 workspace crates). Since 1f4b67a3 the workspace gained 33 crates and
lost or moved none; every crate keeps the group folder it had.

**`AGENTS.md`.** The crate table lists all 75 crates, one row each, and every row's dependency column matches its
manifest's normal dependencies exactly; a test crate's row lists its dev-dependencies, marked "for tests only". Before
the pass 25 crates' rows disagreed with their manifests: 24 lacked edges, among them `ultragraphdb_transactions_txn` and
`_commit` on `ultragraphdb_cluster_placement` (and `_commit` on `_cluster_sequencer`), `ultragraphdb_cluster_partition`
on `_cluster_sequencer` and `_core_catalog`, `ultragraphdb_protocol_transport_compio` on `_transport_tls`,
`ultragraphdb_common_service` on the two protocol transport crates, and `ultragraphdb_store_apply` on
`_error_invariant` and `_traits_catalog`; 2 named edges no manifest holds (`ultragraphdb_cluster_metadata` on
`_error_invariant`, `ultragraphdb_store_hnsw` on `_common_utils`); the three role binaries and their three test crates
shared two rows, now six with their own dependencies (AGENTS.md:232-234, 243-245). Rows whose text described stage 5 or
an earlier task now describe the code: `ultragraphdb_transactions_commit` (`EpochStamp` and `CloseRequest` in place of
the clock sequencer, `Proposal`, `SessionEntry`, `StoredReply`, `SettingError`), `ultragraphdb_transactions_txn`
(`Transaction::bind`, `FrameChanges`, `FrameTerms`, `Commit`, the record's sections, `read_set_bench`),
`ultragraphdb_tool_bench` (`wal_bench --partitions`), `ultragraphdb_tool_test` (`PartitionWorkload`, its five
dependents), `ultragraphdb_cluster_partition` (`RemoteMessage`, `BoundKey`, `BoundStep`), `ultragraphdb_common_role`
(`frontend_address` and `retry_until`, the join; `storage_role` and `compute_role` live in the binaries),
`ultragraphdb_error_cluster` (raised by the placement map, the sequencer and the metadata group, carried by the
frontend's owner; the partitions raise `PartitionError`) and `ultragraphdb_supervisor_tests` (its `image_bdd` target,
tagged `image_test`). The group-folder list names `ultragraphdb/` (AGENTS.md:159-162). The external table gains
`oxilangtag` (AGENTS.md:260) and `mimalloc` (AGENTS.md:276), and its "Used by" columns for `oxrdf`, `oxsdatatypes`,
`fjall`, `futures`, `compio`, `xxhash-rust`, `sha2`, `subtle` and `getrandom` name every crate using each.

**`ROADMAP.md`.** Stage 5.5's paragraph (ROADMAP.md:29-45) states what the stage built: the 128-bit system ID,
epoch versions and the logical record; the sequencer and the metadata group with one replica; partitions as the one
owner of their facts, registration at a home and reverse reads through referrer sets, proved in `lean/PlacementModel`;
the three role binaries joined by role commands 512–767, their images, the supervisor and the config service; the
client against the binaries; `wal_bench`'s N partitions. Its row stays `in progress`, the stage closing with 12.5,
12.5b, 12.6 and 12.8.

**The Shard-per-Core note.** *What stage 5.5 built* (line 1012) maps each decision P20 and P24 give the stage to what
runs and its crate and files, fifteen rows from D1/P1/P7/P15/P16 to P23; the introduction points to it and to backlog
item 11 for what stage 8 builds.

**The backlog.** Items 2, 3, 4, 9 and 10 were specified and built by the change and have left; the introduction names
them and the note's section (backlog.md:14-18). Their open questions were answered in drafting (design, *Decisions taken
in drafting*): principals in the metadata group, the client's automatic credit, a commit in doubt once sent, the
Welcome's write-set bound from the round size. Item 1 keeps what remains for later stages: the `dev-ha` profile, the
data layout per replica, metrics and the operator requirements. Item 11 becomes stage 8's: flush sharing (P10),
sequenced cross-partition work and the epoch agent (P3, P12, P14), the node's runtime (P4, P6, P8, P11) and
`wal_bench`'s second and third steps. Items 5 to 8 and 12 to 17 are unchanged: item 7's checkpoints driven by the
rounds' bytes have no driver in a role yet, since the storage role calls neither `checkpoint` nor `trim_through`.

**A broken link.** `ultragraphdb_common_role/src/lib.rs:8-9` linked `` [`compute_role`] ``, a private function of the compute binary, as what joins the frontend. It now says storage and compute both join the frontend their settings name, `frontend_address`, through `retry_until`, the crate's own exports; `cargo doc -p ultragraphdb_common_role` with `-D rustdoc::broken_intra_doc_links` passes.
