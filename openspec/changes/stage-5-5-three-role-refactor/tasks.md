Every task group follows `openspec/protocols/tdd-protocol.md`'s five phases, in order: (1) the group's public surface
compiling with `unimplemented!()` bodies under `#[expect(clippy::unimplemented, reason = "phase 1 surface")]`; (2) the
full suite written against it and observed failing for that reason, its run and test count recorded in the group's
notes (`notes/group-N.md`): the group's scenarios verbatim as Gherkin in the crate's `features/<capability>/` bound by
its `bdd` target with `fail_on_skipped`, unit tests for the corner cases enumerated in advance, and every error variant
constructed through the public API, each expected value from a source independent of the code; (3) the suite audited
against defects planted in a throwaway implementation; (4) the implementation, judged by the audited suite alone;
(5) a group that decodes bytes from a client, a peer or the disk writes its fuzz targets and seeds and builds them,
while the runs of `scripts/mutants.sh` and `scripts/fuzz.sh` happen once, in group 12. Before any of its crates'
behaviour changes, a group lifts each existing crate it touches into `openspec/protocols/ULTRA_STYLE.md`: the Ultra
lint set in the crate's manifest, zero warnings under Clippy with `-D warnings`, the rules no lint checks reviewed and
recorded, the suite unchanged and green with the same count under Cargo and Bazel, all in a commit of its own; every
crate this change creates starts with the lint set. A group is done when its suites pass under Cargo and Bazel with
equal counts and `make test` passes; the user commits after each group. Removing files waits for the user's approval
at the task that removes them.

## 0. Workspace and protocol

- [ ] 0.1 Pin `toml` (pure Rust, serde) in `[workspace.dependencies]` and record it in `AGENTS.md`'s dependency table; `clippy.toml`: `too-many-lines-threshold = 70`; record the Ultra lint set's manifest block once in `ULTRA_STYLE.md` for crates to copy, with `unsafe_code = "forbid"` repeated
- [ ] 0.2 Create the folders `ultragraphdb_cluster/`, `ultragraphdb_roles/` and the crates of the design's table with the repository's layout, headers, the Ultra lint set and Bazel targets, hermetic and sandboxed, each with an empty `features/` folder its `bdd` target reads
- [ ] 0.3 `PROTOCOL.md` first: the role-command range 512–767 and its commands, the session token in a commit reply, a read's level and token, result batches naming IRIs by text, the client's automatic credit and in-doubt rules; the replica range 256–511 kept for stage 8
- [ ] 0.4 Record the decisions taken in drafting (design, *Decisions taken in drafting*) in the client's and endpoint's notes for the user's review before groups 9 and 10 begin

## 1. The system ID → `ultragraphdb_error_ids`, `ultragraphdb_core_ids`

- [ ] 1.1 Scenarios: system-identity, all requirements
- [ ] 1.2 Phases 1–3: `SystemId`, `TypePrefix`, `Bucket`, `Sequence`, `Namespace`, the canonical key encoding, bucket derivation, `Version`, `Epoch`, `SessionToken`, `IdError` with one variant per refusal; golden buckets from the xxHash reference implementation, committed with their source; the corner cases (prefix 0 and 65,535, bucket 0 and 2^24 − 1, sequence 0, 1 and u64::MAX, reserved bits set one at a time, an empty IRI, a 4 GiB IRI refused)
- [ ] 1.3 Phase 4: the ID's packing, ordering and checks; the derivation; versions and tokens
- [ ] 1.4 Phase 5: a fuzz target for decoding an ID and a version, seeded with the golden values

## 2. Keys and ID lists → `ultragraphdb_core_keys`, `ultragraphdb_core_codec`, `ultragraphdb_core_algo`

- [ ] 2.1 Ultra Style lift of `core_keys`, `core_codec` and `core_algo`, each in its own commit
- [ ] 2.2 Scenarios: key-encoding (all requirements, the Dgraph-byte scenarios rewritten), uid-lists ("ID lists pack into the repository's own format", "Packed lists decode, seek and round-trip"), posting-lists ("Posting lists are stored in the repository's own format")
- [ ] 2.3 Ask the user, then remove `tools/golden/codec.go`'s `codec_uidpack` and `codec_seek` generators, `ultragraphdb_core_codec/testdata/golden/codec_uidpack.json` and `codec_seek.json`, and the tests and feature steps reading them (approved 2026-11-01); retire `algo_setops` and the HNSW graph vectors where the 128-bit formats invalidate them, recording which
- [ ] 2.4 Phases 1–3: `QuadKey` over `SystemId` with the reverse key's part, split keys with 16-byte starts; the packed ID list with its version byte and 104-bit LEB128 deltas, golden values from an independent implementation of the spec committed with their source; the posting format's version byte 2; set operations over 128-bit IDs against a plain sorted-vector oracle
- [ ] 2.5 Phase 4: the keys, the packed list, its decoder and seeks, the set operations, the posting format
- [ ] 2.6 Phase 5: fuzz targets for the key parser, the packed list's decoder and the posting format v2, seeded with the golden values

## 3. Dictionary, statements and vectors → `ultragraphdb_core_terms`, `ultragraphdb_store_quads`, `ultragraphdb_store_posting`, `ultragraphdb_store_apply`, `ultragraphdb_store_hnsw`

- [ ] 3.1 Ultra Style lift of `core_terms`, `store_quads`, `store_posting`, `store_apply` and `store_hnsw`, each in its own commit
- [ ] 3.2 Scenarios: term-dictionary ("IRIs and blank nodes get system IDs", "Concurrent creation of one IRI yields one ID"), quad-storage ("Statement IDs"), vector-index ("HNSW vectors live in posting lists")
- [ ] 3.3 Phases 1–3: IDs from computed buckets and leased sequence blocks (a block source trait the metadata group implements in group 6 and a test double behind `test-utils`); statement IDs of the `Statement` type in the subject's bucket; HNSW over `SystemId`, the dead list as hexadecimal strings
- [ ] 3.4 Phase 4: the dictionary, statements and HNSW on 128-bit IDs; every suite of the store crates green on the new types
- [ ] 3.5 Phase 5: fuzz target for the HNSW lists' decoders on 128-bit IDs

## 4. Versions and the logical log → `ultragraphdb_traits_log`, `ultragraphdb_transactions_log`, `ultragraphdb_transactions_commit`, `ultragraphdb_transactions_txn`, `ultragraphdb_store_kv`

- [ ] 4.1 Ultra Style lift of `traits_log`, `transactions_log`, `transactions_commit`, `transactions_txn` and `store_kv`, each in its own commit
- [ ] 4.2 Scenarios: commit-rounds ("Commit versions are built from the epoch"), write-ahead-log ("The ordered-log interface", "Checkpoints trim the log", "A round's record is logical", "The log is the only synced stream"), transactions ("Transactions are strictly serializable", "Interactive transactions", "Commits answer a session token")
- [ ] 4.3 Phases 1–3: the epoch stamp replacing the clock sequencer; durability tickets on `append`; the logical record (resolved quads, new dictionary entries, version, catalog version, derivation version) and its derivation into keys; fjall with manual journal persistence and the store's explicit sync at checkpoints; session tokens in commit replies
- [ ] 4.4 Phase 4: the commit step, the WAL and the transaction on the new versions and record; replay derives the keys a first apply wrote, checked differentially against the stage-5 apply path on the simulator's workload
- [ ] 4.5 Phase 5: fuzz targets for the logical record's decoder and the WAL scan on the new record, seeded from the suite's records

## 5. Placement and the sequencer → `ultragraphdb_error_cluster`, `ultragraphdb_cluster_placement`, `ultragraphdb_cluster_sequencer`

- [ ] 5.1 Scenarios: sequencer (all requirements); metadata-group ("The placement map assigns bucket ranges to partitions") for the map's own checks
- [ ] 5.2 Phases 1–3: `PlacementMap` with its ranges, namespace runs, split nodes and preferred leaders, checked against a plain bucket-by-bucket reference model; the sequencer's state machine over the ordered log with an injected clock: closes, bounds, `as of`, subscriptions and catch-up, the multi-partition log; `ClusterError` with one variant per refusal
- [ ] 5.3 Phase 4: the map and the sequencer; recovery replaying the sequencer's log, run in the simulator with crashes at every append
- [ ] 5.4 Phase 5: fuzz target for the sequencer's log records and the placement map's encoding

## 6. The metadata group → `ultragraphdb_cluster_metadata`, `ultragraphdb_core_catalog`

- [ ] 6.1 Ultra Style lift of `core_catalog`
- [ ] 6.2 Scenarios: metadata-group (all requirements), schema-storage ("The schema persists in the metadata group"), schema-changes ("A schema change takes effect at an epoch boundary")
- [ ] 6.3 Phases 1–3: the metadata group's state machine: catalog versions with their effective epochs, block leases from per-type high-water marks, the placement map, principals with SCRAM credentials and the bootstrap from a secret file, snapshots and trimming; the catalog's loading from the group in place of catalog keys
- [ ] 6.4 Phase 4: the group, its recovery from snapshot and log, the block source of group 3 implemented by it; schema changes switching at their epoch in the simulator over two partitions
- [ ] 6.5 Phase 5: fuzz targets for the group's command records and snapshots

## 7. Partitions → `ultragraphdb_cluster_partition`, `ultragraphdb_tool_bench`, `ultragraphdb_tool_test`

- [ ] 7.1 Ultra Style lift of `tool_bench` and `tool_test`
- [ ] 7.2 Scenarios: partitions (all requirements)
- [ ] 7.3 Phases 1–3: a partition over its bucket ranges with its own log, commit step, fjall instance, dictionary cache and sequence blocks; routing checks naming the owner; `CrossPartition` aborts; reverse keys' parts from the map; absence by predicate lists; the read kernel (a keyspace range at a version, bounded by a row limit)
- [ ] 7.4 Phase 4: partitions in one process, each on its own compio runtime; the simulator's workload over two partitions under every fault class
- [ ] 7.5 `wal_bench`: N partitions in one process, one seeded generator each, per-partition and total relations, bytes and syncs per second

## 8. Protocol and transport → `ultragraphdb_protocol_wire`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`

- [ ] 8.1 Ultra Style lift of `protocol_wire`, `protocol_transport` and `protocol_transport_compio`, each in its own commit
- [ ] 8.2 Scenarios: wire-protocol ("Requests name terms by their text", "Roles exchange role commands", "Reads carry their level and session token")
- [ ] 8.3 Phases 1–3: the role commands' bodies, views and builders in the 512–767 range, refused on client connections; the session token and read level in their bodies; result batches naming IRIs by text through the dictionary; the Unix-domain-socket carrier chosen by the address scheme; the conformance suite extended with the new frames, valid and malformed
- [ ] 8.4 Phase 4: the bodies and the carrier
- [ ] 8.5 Phase 5: fuzz targets for every new body's decoder, one structure-aware through `arbitrary`

## 9. The roles → `ultragraphdb_roles_storage`, `ultragraphdb_roles_compute`, `ultragraphdb_roles_frontend`

- [ ] 9.1 Scenarios: client-endpoint (all requirements), quad-reads (all requirements)
- [ ] 9.2 Phases 1–3: storage's host (role commands, epoch delivery, block leases, the read kernel's service); compute's reads (fan-out by bound subject, merge in key order, access paths, resume keys as checksummed tokens, levels and tokens); the frontend's endpoint (connection and per-connection bounds, SCRAM against the metadata group, sessions, routing with one resend on a stale map, statement caches, the error table with `CrossPartition` and `Unavailable`, vector-index declarations refused); the metadata group and sequencer hosted
- [ ] 9.3 Phase 4: the three role crates against each other in one test process over the in-memory transport of `test-utils`, then over Unix sockets
- [ ] 9.4 Phase 5: fuzz target for the resume token's decoder

## 10. The client against the cluster → `ultragraphdb_client_core`, `ultragraphdb_client_tokio`, `ultragraphdb_client_compio`

- [ ] 10.1 Ultra Style lift of `client_core`, `client_tokio` and `client_compio`, each in its own commit
- [ ] 10.2 Scenarios: wire-protocol ("The client's credit is automatic", "A commit in flight is in doubt once sent"), transactions ("Commits answer a session token") from the client's side, quad-reads ("A read names its level")
- [ ] 10.3 Phases 1–3: session tokens kept per session and sent with reads; read levels; the shells without `grant`, the core's grant taking `NonZeroU32`; `ConnectionLost` before a commit's frame is sent and `InDoubt` after, `RetriesExhausted` folded into `InDoubt`
- [ ] 10.4 Phase 4: the client against the frontend; the client's conformance suite against the role binaries of group 11

## 11. Binaries, supervisor and images → `ultragraphdb_error_role`, `ultragraphdb_bin/*`

- [ ] 11.1 Scenarios: role-binaries (all requirements)
- [ ] 11.2 Phases 1–3: settings from a standard TOML file through the `toml` crate (`deny_unknown_fields`), overridden by `ULTRAGRAPHDB_<TABLE>_<KEY>` variables, with every setting's bound checked; the profiles as TOML files; `/health` and `/ready` from the in-house HTTP/1.1 responder; start, readiness and the shutdown order; exit codes; the io_uring probe and fallback; the supervisor's start order, signal forwarding and exit code
- [ ] 11.3 Phase 4: the four binaries; the dependency check that the frontend links no storage engine
- [ ] 11.4 Images: `glibc_multiarch_img` for each role; the development image layering the four binaries; the seccomp profile in `bazel/oci_images/seccomp/`; load targets; the role-binaries scenarios run against the images in Docker, under the default seccomp profile and the shipped one
- [ ] 11.5 Phase 5: fuzz targets for the settings' validation over TOML text and the HTTP request parser

## 12. Final verification and documentation

- [ ] 12.1 `scripts/mutants.sh` over every crate the change added or edited; every survivor killed or recorded in `.cargo/mutants.toml` with its measurement
- [ ] 12.2 `scripts/fuzz.sh` over every target the change added, its report recorded, every regression case replayed under Cargo and Bazel; the fuzzers audited by planting two or three decoder defects each
- [ ] 12.3 Ultra Style conformance: every touched and created crate builds with zero warnings under the lint set; the review of the unlinted rules, each crate's exceptions and their reasons listed in the change's notes
- [ ] 12.4 Measurements on the VM disk: `wal_bench` with one partition against stage 5's figures (relations per second, bytes per relation with 128-bit IDs), and with 1 to 12 partitions in one process against the side-by-side table of the Shard-per-Core note
- [ ] 12.5 The development container end to end: the supervisor's image started, a client writing and reading through it, killed and restarted with every acknowledged commit held
- [ ] 12.6 Documentation: `AGENTS.md` (crates, dependencies, layers), `ROADMAP.md` (stage 5.5's row), the Shard-per-Core note and the backlog closed with what this change built
- [ ] 12.7 `make test` green; the final test counts recorded
