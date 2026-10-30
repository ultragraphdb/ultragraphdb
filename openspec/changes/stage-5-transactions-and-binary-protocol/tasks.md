Every task group follows `openspec/protocols/tdd-protocol.md`'s five phases, in order: (1) the group's public surface
compiling with `unimplemented!()` bodies; (2) the full suite (the group's scenarios verbatim as Gherkin, unit tests, the
corner cases enumerated in advance, every error variant constructed) written against it and observed failing, the
failing run and its test count recorded in the group's notes; (3) the suite audited against deliberate defects in a
throwaway implementation; (4) the implementation, judged by the audited suite alone; (5) hardening: a group that decodes
bytes from a client, a peer or the disk writes its fuzz targets and their seeds, building them, while the runs of
`scripts/mutants.sh` and `scripts/fuzz.sh` happen once, at the very end, in the stage's final verification (11.7 and
11.8; decided 2026-10-27). A phase-3 throwaway implementation that passes the audited suite may be kept as the implementation (decided 2026-10-27). A group is done when its suites pass under Cargo and Bazel with equal
counts and its operations run in the simulator's workload under every fault class. Removing files or crates waits for
the user's approval at the task that removes them. Groups 6, 7 and 8 build the transaction path memory-efficient and
performance-optimized by design, before any benchmarking (decided 2026-10-27): no allocation per read or write on the
hot path, keys held once and named by u32 spans, sorted read sets merge-joined against the history with validation
exact, write-set records written straight into their wire buffer, per-transaction memory bounded and accounted; each
optimized structure tested differentially against a plain reference model, close design choices settled by a short
benchmark recorded in the group's notes, and nothing traded for clarity or safety without a measured gain.

## 0. Workspace

- [x] 0.1 Add `compio` and `xxhash-rust` (feature `xxh3`) to `[workspace.dependencies]`, pinned, pure Rust confirmed (no `cc` or `-sys` in their trees); `futures-channel` through the pinned `futures`; record them in `AGENTS.md`'s dependency table
- [x] 0.2 Create the group folders `ultragraphdb_protocol/`, `ultragraphdb_server/`, `ultragraphdb_transactions/` with their workspace globs, and the crates of the design's table with the repository's layout, headers, `[lints] workspace = true` and Bazel targets, hermetic and sandboxed
- [x] 0.3 Make `lean/` a Lake project with `lean-toolchain` pinned to MGQL's release and the library `RoundModel`, built outside Bazel
- [x] 0.4 Fuzzing: `scripts/fuzz.sh`, shaped as `scripts/mutants.sh` (installed-tool check, crate and target arguments, a time budget per target, a report of executions, coverage and failing inputs); `arbitrary` pinned in `[workspace.dependencies]` for the fuzz packages, which form one workspace in `fuzz/` with a package per fuzzed crate at `fuzz/<crate>/`, its dependencies declared `{ workspace = true }` (decided 2026-10-27); the replay helper `fuzz_regressions` in `ultragraphdb_tool_bdd`, reading `testdata/fuzz_regressions/<target>/` under Cargo and Bazel; the nightly toolchain the script uses named in it

## 1. The posting format → `ultragraphdb_core_codec`

- [x] 1.1 Scenarios: posting-lists ("Posting lists are stored in the repository's own format"); `uid-lists` unchanged and green on the moved types
- [x] 1.2 List the protobuf fields the code reads and those it does not; the new `UidPack`, `UidBlock`, `List`, `Posting`, `PostingList` and backup list carry the first
- [x] 1.3 Phases 1–3: the types and the codec's surface in `core_codec`; the suite of encoding, decoding, the checksum and every truncation and bit flip refused as corruption naming the key
- [x] 1.4 Phase 4: the format (version byte, flags, commit timestamp, pack blocks, postings, splits, XXH3-64 trailer); the codec moved out of `store_posting/src/utils/posting_codec.rs`
- [x] 1.5 `core_algo`, `store_posting`, `store_quads`, `store_apply`, `store_hnsw` and their tests on the new types; every suite green
- [x] 1.6 Remove `core_types`' `object_value`, `get_geo_tokens`, `match_geo`, `sort_with_facet` and `core_tok`'s `get_index_factory_from_spec` with their tests (approved 2026-10-27)
- [x] 1.7 Ask the user, then remove `ultragraphdb_proto_pb`, `ultragraphdb_proto_dgraph` and `ultragraphdb_proto/`, `bazel/toolchains/prost_toolchain`, `MODULE.bazel`'s `protobuf` and `rules_rust_prost`, and the pins of `prost`, `tonic`, `tonic-prost`, `tonic-prost-build`; confirm `tonic` and `prost` remain only under `opentelemetry-otlp`
- [x] 1.8 Phase 5: mutants over `core_codec`; fuzz target for the posting format's decoder, seeded with its golden values

## 2. The header and checksums → `ultragraphdb_error_protocol`, `ultragraphdb_protocol_wire`

- [x] 2.1 Scenarios: wire-protocol ("One header for every frame", "XXH3-64 checksums", "Chunked body checksums", "Frames are validated before use and never panic", "Frames are at most 1 MiB", "Commands keep their numbers")
- [x] 2.2 Phases 1–3: `ProtocolError` with one variant per validation rule; the header view and builder; chunk tables; the validation order; XXH3-64 checked against the xxHash reference implementation's outputs, committed as golden values with their source
- [x] 2.3 Phase 4: the header, checksums, chunked bodies and validation, read with `from_le_bytes`, no `unsafe`
- [x] 2.4 Phase 5: mutants over the header and checksums; fuzz targets for header validation and the chunk table, one of them structure-aware through `arbitrary`

## 3. The write-ahead log → `ultragraphdb_error_log`, `ultragraphdb_traits_log`, `ultragraphdb_transactions_log`

- [x] 3.1 Scenarios: write-ahead-log, all requirements
- [x] 3.2 Phases 1–3: `OrderedLog`, `Position`, `LogError`; `DurableStorage` and `DurableFile`; the suite including every fault mode of the simulated storage and every corruption case of the scan
- [x] 3.3 Phase 4: the simulated storage (buffered and stable contents, file and directory syncs, power loss, faults before and after, torn writes reported and silent)
- [x] 3.4 Phase 4: the compio storage: preallocated segments, aligned writes, `fdatasync`, directory syncs
- [x] 3.5 Phase 4: rounds as frames at block boundaries; self-identifying records (position, parent, cluster, incarnation); the scan and its corruption rules, reported truncation of a torn tail, refusal on interior damage naming round, chunk and transactions
- [x] 3.6 Phase 4: the ping-pong superblock with its three outcomes; checkpoints and trimming; `seal`
- [x] 3.7 Phase 4: the power-loss-protection flag's defaults for round bounds and checkpoint interval, never touching the sync
- [x] 3.8 Phase 5: mutants over the log; fuzz targets for the scan over arbitrary segment bytes (never accepting a round whose checksums fail) and for the superblock's decoding
- [x] 3.9 Segment allocation (approved 2026-10-27): `fallocate` with `FALLOC_FL_KEEP_SIZE` through `rustix`, pinned directly, for a segment that has never existed, in place of writing zeros, with zeros written instead on a file system without `fallocate` (`EOPNOTSUPP`, one warning); a checkpoint recycles trimmed segments by zeroing them and syncing them on the checkpoint path, then renaming them for reuse instead of removing them (option (a), decided 2026-10-27), so steady state allocates nothing and every round's sync is data only; no preallocation on macOS; the extra sync cost of a fresh segment's extent conversion measured in task 12.4

## 4. Read sets at statement granularity → `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_store_apply`

- [x] 4.1 Scenarios: changesets ("A changeset reports its regions"), transactions ("What a transaction's reads record and what conflicts with them", recording side)
- [x] 4.2 Phases 1–3: `Request`'s tested statements and scanned ranges beside `read_keys`; `QuadWrite::holds`; dictionary absences in `QuadStore`; ranges in the declaration checks
- [x] 4.3 Phase 4: inserts record the statement, not the list; deletes, overwrites and unique checks keep their list reads
- [x] 4.4 Phase 5: mutants

## 5. Execution leaves the commit lock → `ultragraphdb_store_apply`, `ultragraphdb_store_posting`

- [x] 5.1 Scenarios: local-commit (all modified requirements), changesets ("A changeset describes a write")
- [x] 5.2 Phases 1–3: `execute` returning `Executed`, `apply_round` taking a round's admitted `Executed`s at given versions; rollups' version rule
- [x] 5.3 Phase 4: `ChangesetCore::apply_batch` split; transactions stop using `Batch` and `StackedLayers`; the store batch written with `PersistMode::Buffer`; `Timestamps` keeps open reads and the discard bound and stops giving commit versions
- [x] 5.4 Phase 4: rollups read at the watermark and write one above the newest commit version they fold
- [x] 5.5 Phase 5: mutants

## 6. Transactions on a worker → `ultragraphdb_transactions_txn`

- [x] 6.1 Scenarios: transactions ("Transactions read a snapshot at the visibility watermark", "A transaction reads its own writes", "Transactions have bounded lifetimes", "Interactive transactions")
- [x] 6.2 Phases 1–3: `Transaction`, `ReadSet`, the write-set record builder in `protocol_wire`'s layout, the limits as settings
- [x] 6.3 Phase 4: snapshots at W, reads of own writes, the 10 s and 16 s limits with retryable errors naming them; a read version above W refused at once with an error naming it and W, never waited on (decided 2026-10-27)
- [x] 6.4 Phase 5: mutants

## 7. The commit step → `ultragraphdb_transactions_commit`

- [x] 7.1 Scenarios: commit-rounds, all requirements; transactions ("Transactions are strictly serializable", "Commit-time operations", "Exactly-once commits through sessions", "Aborts name their reason")
- [x] 7.2 Phases 1–3: the step function's inputs and outputs; the sequencer, resolver, history, commit-time operations, sessions table
- [x] 7.3 Phase 4: rounds closed by the previous sync, bounded; versions `max(previous + 2, 2 × clock µs)`
- [x] 7.4 Phase 4: the conflict history (points, statements, ranges), exact validation in round order, pruning and too-old aborts; `QuadStore::confirm`'s IRI check as one rule
- [x] 7.5 Phase 4: insert-if-absent, upsert-and-link, compare-and-set and counter add after admission, against the store at W plus every decided but unapplied round's writes (design, "The commit step"); a predicate's implicit default catalog entry as an insert-if-absent on its catalog key, not a catalog change (decided 2026-10-27); the created node's properties and the links applied through `ChangesetWrite` with the UID filled in
- [x] 7.6 Phase 4: the pipeline: log append and sync, apply, watermark, replies; a failed sync stops every later write
- [x] 7.7 Phase 4: catalog changes, drops and index builds' touched nodes in rounds; the scenario "A write after a build's start in the same round aborts" tested against the resolver, not assumed (decided 2026-10-27)
- [x] 7.8 Phase 4: sessions registered through a round, kept in the keyspace without versions, bounded, retries answered from the table
- [x] 7.9 Phase 4: recovery: replay above the store's applied version, refusal of a store ahead of the log or a gap
- [x] 7.10 Phase 5: mutants (deferred to 11.8, the stage's one run of `scripts/mutants.sh`, decided 2026-10-27)

## 8. The vector indexer → `ultragraphdb_store_hnsw`, `ultragraphdb_store_apply`, `ultragraphdb_core_mapping`

- [x] 8.1 Scenarios: vector-index (the modified and added requirements)
- [x] 8.2 Phases 1–3: the dimension in `VectorIndexSpec` and its check against the declaration; the indexer's surface; the level draw as a function of the node
- [x] 8.3 Phase 4: transactions write vectors without touching HNSW lists; the indexer in commit order with its indexed version; searches dropping nodes not visible at their snapshot
- [x] 8.4 Phase 5: mutants (deferred to 11.8, the stage's one run of `scripts/mutants.sh`, decided 2026-10-27)

## 9. Bodies, the bus and the conformance suite → `ultragraphdb_protocol_wire`, `ultragraphdb_protocol_bus`

- [x] 9.1 Scenarios: wire-protocol ("Bodies are read in place", "Handshake, streams, credit and cancellation", "Data requests, prepared statements and describe", "Result batches are columnar and dictionary-encoded", "The conformance suite")
- [x] 9.2 Phases 1–3: every body's view and builder; the bus's connection, pool and stream surface; the golden frames and the malformed corpus
- [x] 9.3 Phase 4: the bodies, including the Round frame and the write-set record
- [x] 9.4 Phase 4: the bus over compio: framing reads into pooled buffers, streams, credit, cancel; the simulated network (drop, delay, duplicate, split)
- [x] 9.5 Phase 4: prepared statements over quad patterns with schema ids; describe from the data dictionary; columnar result batches over `core_terms`' encoding
- [x] 9.6 Phase 5: mutants; fuzz targets in the bus and client crates' packages under `fuzz/` for every body's view, the round and write-set record, and quad patterns with parameters, seeded with the golden frames; a structure-aware target that builds valid frames from `arbitrary` fields and mutates them

## 9b. The Rust client → `ultragraphdb_client_core`, `ultragraphdb_client_tokio`, `ultragraphdb_client_compio`; the transport → `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio` (decided 2026-10-28)

A complete client, not a skeleton: the library a Rust application links, and what the conformance suite, Elle and every
stage-5 test drive the server with. It speaks the same protocol as the server, through `ultragraphdb_protocol_wire` and
`ultragraphdb_protocol_transport`, and adds no protocol of its own.

- [x] 9b.1 Scenarios: wire-protocol ("The Rust client"), and the client's side of "Handshake, streams, credit and cancellation" ("Credit stops a stream", "Cancel ends a stream"), "Data requests, prepared statements and describe" ("A stale statement is refused") and "Result batches are columnar and dictionary-encoded" ("A repeated term is sent once per batch"), against a scripted server over the simulated network; the end-to-end runs against the node are 10.1's
- [x] 9b.2 Phases 1–3: the client's surface: connect over TCP or a simulated stream; the handshake; sessions; read-only and read-write transactions (begin, read, write, commit, abort); data requests; prepared statements, stale after a catalog change by their schema id; describe; result streams with credit and cancel, read in place; typed errors, retryable exactly where the wire says
- [x] 9b.3 Phase 4: the implementation over the transport: request terms in the wire's term table; result batches read through their views with no copy; buffers reused per connection; a commit retried only through its session (same session and request number), with reconnects and bounded retries, so a retry is applied once
- [ ] 9b.4 The conformance suite run against the client core and both shells: every golden frame read or built as its bytes, every malformed frame refused with the connection's rule, in the simulator under drop, delay, duplicate and split (done against the core directly; the golden frames under the simulator's faults and through the shells remain)
- [x] 9b.5 Phase 5: fuzz targets in `fuzz/ultragraphdb_client_core/` for everything the client decodes from a server (replies, errors, result batches, describe), seeded with the golden frames, built and not run; mutants at 11.8
- [x] 9b.6 `AGENTS.md`: the stale `ultragraphdb_client` test-roots paragraph rewritten for the client's crates
- [x] 9b.7 `ultragraphdb_protocol_bus` renamed `ultragraphdb_protocol_transport` (the transport that carries the wire protocol), with its fuzz package and `BusError` → `TransportError` (decided 2026-10-28)
- [x] 9b.8 The transport made sans-I/O (https://sans-io.readthedocs.io/how-to-sans-io.html) with no runtime dependency: framing as a pull-style state machine telling the shell what to read into which pooled buffer, no copy, and the handshake as state machines, beside `Outbound` and `BufferPool`; `Connection` a thin compio shell in `ultragraphdb_protocol_transport_compio`; the state machines tested synchronously (decided 2026-10-28)
- [x] 9b.9 `ultragraphdb_client_core`: the client as a sans-I/O core (handshake, sessions, transactions, requests, replies and result batches, streams, credit, cancel, retries through the session, time passed in), tested synchronously against the scripted server over the simulated network under every fault class (decided 2026-10-28)
- [x] 9b.10 `ultragraphdb_client_tokio`, the primary public Rust client: the core's tokio shell with `Send` futures over TCP, `tokio` pinned; tested against the scripted server over the simulated network and a TCP loopback (decided 2026-10-28)
- [x] 9b.11 `ultragraphdb_client_compio`, the public compio client: the core's compio shell over the transport's compio `Connection`; the same tests (decided 2026-10-28)

## 9c. The protocol block → `ultragraphdb_protocol_wire`, `ultragraphdb_protocol_transport`, `ultragraphdb_client_core` and its shells, `ultragraphdb_transactions_commit`, `ultragraphdb_transactions_txn`, `ultragraphdb_store_apply`, `ultragraphdb_transactions_log`, `ultragraphdb_core_terms` (decided 2026-10-28, scoped 2026-10-30)

The specification's open points resolved before the remaining groups (decided 2026-10-28): every point of
`ultragraphdb_protocol/PROTOCOL.md` §13, as its triage §13.8 records them (category A approved row by row, category B
decided, 2026-10-30), and the security design, `openspec/notes/security.md`. Specification first: each change is written
into `PROTOCOL.md` and then built (decided 2026-10-28). Groups 8, 10, 11, 12 and 13 wait until this group closes.

- [x] 9c.1 The open points found and triaged: a critical reading of the specification (points 35–55), the points carried from the stage's notes (29–34), category A's recommendations and category B's decisions (2026-10-30)
- [x] 9c.2 The security design, checked against Dgraph's ACL and current practice: `openspec/notes/security.md` (2026-10-30)
- [x] 9c.3 The security design's open questions answered by the user (its "Open questions", 2026-10-30)
- [x] 9c.4 Specification: `PROTOCOL.md` written to every resolution of §13.8 and to the security design's wire changes, §13 removed with every point resolved (its record in `notes/group-9c.md`); `SECURITY.md`, the security architecture; the design documents written to the code (points 23–27) (2026-10-30)
- [x] 9c.5 Scenarios: the wire-protocol and transactions specs' deltas for the new behaviour, verbatim as Gherkin: authentication (SCRAM-SHA-256, a refusal, a request before it, re-authentication on an open connection, a credential's expiry), a session's owner, reads and executes on streams of their own naming their transaction (B3), the Drop command (B2), statement IDs in results (B4), the blank-node tag and the genid guard (B1), the write-set bound in Welcome (B5), the frame sequence (point 32), connection and stream errors (points 19, 45), NotCommitted (point 29), the counter's value (point 33). Written into the change's specs (2026-10-30), with the write-ahead-log's checkpoint version (point 34) and server-node's authentication (10.7) beside them; their Gherkin copies land bound, with 9c.6's phase 2 and 10.1, as an unbound step fails a `fail_on_skipped` run
- [ ] 9c.6 Phases 1–3: the surfaces: the wire's Authenticate and Drop commands, Hello's `max_frame`, Welcome's authentication and limits, the transaction field of Read and Execute, the statement-ID column, tag `0x02`, error codes 18 to 22, the write-set record's `principal` and operations 4 to 7, the OperationFailed value as a term table, the counter's value; the transport's authentication phase and re-authentication, stream rules and frame sequence; the client core's SCRAM, its reads on their own streams and its retry rules (points 29, 36, 38); the commit and store sides of drops 4 to 7, `principal`, the session's owner and the checkpoint's version (point 34)
- [ ] 9c.7 Phase 4, the frame and the connection: points 2, 3, 7, 16, 17, 19 and 45, 32, 44, 55; the handshake with authentication; reads on their own streams with credit per request (B3, points 1, 30); the golden frames regenerated
- [ ] 9c.8 Phase 4, terms and results: points 11, 12, 13, 49, 50, 51, 52, 54 in the wire and `ultragraphdb_core_terms`; tag `0x02` (B1); statement IDs (B4)
- [ ] 9c.9 Phase 4, requests, errors and sessions: points 4, 5, 8, 9, 14, 20, 21, 22, 29, 33, 35–40, 46–48; the session's owner; the Drop command with its proposals and operation codes, recovery redoing them (B2); the write-set bound (B5)
- [ ] 9c.10 Phase 4, the log: `principal` in the write-set record; the checkpoint's version and recovery's refusal of a store behind it (point 34); point 15 stated
- [ ] 9c.11 The transport's state machines audited as phase 3 audits a suite: defects planted in a throwaway copy, each found by the suite or the suite strengthened
- [ ] 9c.12 Phase 5: fuzz targets for every decoder added or changed, seeded from the regenerated golden frames, built and not run (11.7)

## 10. The server → `ultragraphdb_server_session`, `ultragraphdb_server`

- [ ] 10.1 Scenarios: server-node, all requirements; wire-protocol ("The Rust client", end to end against the node), transactions ("A read above the watermark is refused", at the session); and the two scenarios group 2 left to the server: "A refused request keeps the connection" and "A large result streams", with the cluster and unknown-command scenarios rebound to answering and closing the connection
- [ ] 10.2 Phases 1–3: the session layer's dispatch, the node's configuration and lifecycle; the session converts Write, Describe and Operation bodies and request terms to `Changeset`, declarations and oxrdf types (the wire's own layouts, decided 2026-10-27), the conversion of triple terms iterative or bounded by the wire's maximum nesting depth, never recursing past it
- [ ] 10.3 Phase 4: n−1 compio workers pinned, the commit step on one core, least-loaded accept, bounded queues answering busy; every runtime built with compio's `single_issuer` and `defer_taskrun` (decided 2026-10-27; Linux 6.6 is the baseline)
- [ ] 10.4 Phase 4: configuration read once by the binary; start with recovery before accept; graceful shutdown; telemetry through `ultragraphdb_common_telemetry`
- [ ] 10.5 Phase 4: every server scenario and the conformance suite run through `ultragraphdb_client` against the node
- [ ] 10.6 Phase 5: mutants
- [ ] 10.7 Phase 4: the server's side of security, as `openspec/notes/security.md` phases it for group 10: SCRAM-SHA-256, the password rules, throttling of failed attempts, the audit of authentication; running without authentication on loopback only; the bootstrap administrator from its secret file; the authorizer seam, holding every privilege for an authenticated principal and failing closed on what it does not know

## 11. Verification → `lean/` (`RoundModel`), `ultragraphdb_transactions_sim`

- [ ] 11.1 Scenarios: transaction-verification, all requirements
- [x] 11.2 The Lean model of rounds, validation, commit-time operations, watermark, sync, crash, recovery and sessions; the four properties proved, with the sync's durability stated as the model's assumption; no `sorry`
- [ ] 11.3 The node simulator from a seed: workers, commit step, log over simulated storage, simulated store, network and clock faults; acknowledged commits present and the store equal to the log's replay after every step and recovery; the model's theorems restated as invariants
- [ ] 11.4 The white-box checker over the log and the simulator's trace
- [ ] 11.5 Elle's list-append and read-write register workloads through the client at 8 clients, with real-time edges, outside Bazel; the runs and their results recorded
- [ ] 11.6 Kill tests on the binary during rounds, syncs and checkpoints
- [ ] 11.7 Scenarios: transaction-verification ("Every decoder of untrusted or stored bytes is fuzzed"); a full `scripts/fuzz.sh` run over every target at the stage's close, its report recorded, every regression case replayed under Cargo and Bazel; and an audit of the fuzzers themselves, as phase 3 audits a suite: for each target, two or three typical decoder defects planted in a throwaway copy of the decoder (an unchecked length that panics on a slice, an off-by-one that breaks the re-encode round trip, a missing bound behind the checksum), each of which the target must find within its time budget, the time to find each recorded; a target that misses one is strengthened or replaced (decided 2026-10-27)
- [ ] 11.8 Mutation testing at the stage's close: `scripts/mutants.sh` over every crate the stage added or changed, every surviving mutant resolved as a suite gap (a test added) or a recorded equivalence in `.cargo/mutants.toml`; the counts recorded per crate
- [ ] 11.9 The write-ahead log's compio suite and kill tests on Linux, in an OrbStack container, so the `O_DIRECT` path, which only type-checks on macOS, runs against a real Linux file system

## 12. Measurements → `ultragraphdb_tool_bench`

- [ ] 12.1 The stage-4 workload under `Durability::Filesystem` as the baseline
- [ ] 12.2 Throughput at 1, 2, 4, 8 and n−1 workers with abort rates; the commit step's cost per transaction against the execution cost divided by n−1
- [ ] 12.3 Match-then-insert and insert-if-absent over a Zipf-distributed set of entities at 8 writers; D46's batch of 1,000 member moves
- [ ] 12.4 The sync cost and the round sizes on a drive with power-loss protection and on one without; the flag's defaults set from them; the latency of the round that moves into a new segment (a spare's rename and directory sync) against a round within one, and of a fresh segment's first sync after `fallocate`
- [ ] 12.5 The write volume of the log beside fjall's journal; the vector indexer's lag on a vector-heavy workload
- [ ] 12.6 Read summaries' false-abort rates replayed offline against the log's write sets, per budget and bits per key
- [ ] 12.7 Commit latency (median and p99) and throughput at n−1 workers with the runtimes' `single_issuer` and `defer_taskrun` on and off, on Linux
- [ ] 12.8 The results in the overview's "Measured" sections, each naming its drive

## 13. Documentation and close

- [ ] 13.1 `AGENTS.md`: the new crates, folders and dependencies, the removed proto crates, and fuzzing beside mutation testing
- [ ] 13.4 `openspec/protocols/tdd-protocol.md`: phase 5 adds fuzzing for code that decodes bytes from a client, a peer or the disk, with `scripts/fuzz.sh` and committed regression cases
- [ ] 13.2 `ROADMAP.md` and the plan's "The write path on n−1 cores", "What a conflict is checked on" and stage rows 5 and 5.5 from the note's decisions
- [ ] 13.3 `make format`, `make fix` and `make test` green; Cargo and Bazel counts equal; every group's phase notes complete
