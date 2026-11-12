Every task group follows `openspec/protocols/tdd-protocol.md`'s five phases, in order, as stage 6.6's groups do: (1) the
group's public surface compiling with bare `unimplemented!()` bodies; (2) the full suite written against it and
observed failing for that reason, its run and test count recorded in `notes/group-N.md`: the group's scenarios verbatim
as Gherkin in the owning crate's `features/<capability>/`, bound by its `bdd` target with `fail_on_skipped`, unit tests
for the corner cases enumerated in advance, every error variant constructed through the public API, and every expected
value from a source independent of the code, the conformance vectors of `specification/conformance/storage/` first;
(3) the suite's inputs reviewed against the corner-case enumeration; (4) the implementation, judged by the suite alone;
(5) as the group's last step, `scripts/mutants.sh <crate> --diff <base>` once per crate the group changed, and for a
group decoding bytes from a peer or the disk, its fuzz targets and seeds written and built. Before an existing crate's
behaviour changes, its group lifts it into `openspec/protocols/ULTRA_STYLE.md` in a commit of its own; every crate
this change creates conforms from its first commit, is created in full by the group that builds it, and gets its row
in `AGENTS.md`'s crate table. Tests return `TestResult`. Bazel is the gate: a group is done when `make test` passes,
cached; the user commits after each group. Removing files waits for the user's approval at the task that removes them.
A weakness found in a specification is fixed in that specification first.

Waves: 0; then 1 and 3 in parallel; 2; 4; 5; 6; 7; 8; 9; 10; 11; 12. A wave's agents work on disjoint
crates, and the whole repository is gated by `make build` and `make test` when a wave returns.

## 0. Specifications first

- [x] 0.1 `STORAGE.md`, `PROTOCOL.md` sections 5.2, 6.6, 6.8, 9.4, 11 to 13, `SCHEMA.md` and `CATALOG.md` consistent by the review of `openspec/notes/UltraGraphDB Spec Consistency Review.md` (T48, 2026-11-10); the conformance vectors regenerated and deterministic
- [x] 0.2 This change's specifications: the six new capabilities and the deltas of the modified and removed ones
- [x] 0.3 Stage 6.6's tokenizers delta (`openspec/changes/stage-6-6-ugs-data-definition/specs/tokenizers/spec.md`) aligned with design D8: the `measure` token the value in the quantity kind's canonical unit as a `Float64` comparison form, exact; the `decimal` token the sortable encoding; identifier bytes the registry's; no Dgraph equality
- [ ] 0.4 The corner cases of every group enumerated in `notes/corner-cases.md` before its phase 2: per decoder its refusals in `STORAGE.md` section 19.4's order, per state machine its boundary conditions, per lane its version rules
- [ ] 0.5 The specification step reopened on 2026-11-12 (`openspec/notes/UltraGraphDB Specification Update.md`): `specification/conformance/storage/generate_vectors.py` and its vectors brought to `STORAGE.md` as it now stands: extent leaves and the extent root (kinds 21 and 22) in `kinds.json`; the superblock's `extents` reference at offset 176 and the 58-byte checkpoint identity in `grid.json`; checkpoint step 2 placing the extent leaves and root, and the reserve counting them; `refusals.json` for both kinds; the vectors regenerated and deterministic
- [ ] 0.6 Vectors for the object binding (`STORAGE.md` section 12.5): an extent's record from its placed blocks, an extent object's key and bytes, a checkpoint record's key and bytes with the records of the extents step 2 closed, and a punched extent repaired by fetch (section 17.4)
- [ ] 0.7 This change's specifications and design rebased on stages 5.8 and 5.9 and on the two bindings: the deltas of `quad-reads`, `wire-protocol`, `role-binaries`, `ingest`, `transactions` and `storage-lanes` naming compute and the system group; tile-grid's requirements for extent records, publication, absent extents, retention and restore; the narrowed slot rule of section 14.6; the object store's interface and its simulated store placed in the task groups

## 1. Ownership and the logical seam → `ultragraphdb_traits_engine` (new), `ultragraphdb_error_tile` (new), `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_commit`, `ultragraphdb_cluster_partition`, `ultragraphdb_store_quads`, `ultragraphdb_core_keys`

- [ ] 1.1 Ultra Style lift of each existing crate the group changes, each in its own commit
- [ ] 1.2 Scenarios: local-commit's ownership, overlay and restart requirements; transactions' lane X recording; key-encoding's logical conflict keys, as Gherkin
- [ ] 1.3 Phases 1 to 3: `traits_engine`'s logical change types, lane requests and answers, and the `Layout` interface (design D6); `error_tile`'s `TileError(pub TileErrorEnum)` with every refusal of `STORAGE.md` section 19.4 and the commit refusals; the KV store behind `traits_engine` as an engine of its own
- [ ] 1.4 Phase 4: the engine owned by the partition's core; transactions, Scans and Frontier requests holding handles; an attempt undone and rerun from its request on a miss (T33); the transaction's write overlay and the admitted-proposal overlay; conflict keys in the eight logical families of `PROTOCOL.md` section 12.2, a drop one range; every suite green on the KV engine
- [ ] 1.5 Phase 5: `scripts/mutants.sh` over each changed crate

## 2. The log's record → `ultragraphdb_transactions_log`, `ultragraphdb_transactions_commit`, `ultragraphdb_cluster_partition`, `ultragraphdb_store_apply`, `lean/`

- [ ] 2.1 Ultra Style lift of each existing crate the group changes that group 1 did not lift
- [ ] 2.2 Scenarios: commit-rounds' and write-ahead-log's requirements of this change, as Gherkin
- [ ] 2.3 Phases 1 to 3: the Round's `flags`, derivations (section 6) and evictions (section 7), `first_version` of an entry-less round, the u64 derivation version over the `[tiles]` settings; the write-set record's `entry_kind` and conflict families; the session table keyed by session alone with the share, command, command header and body in its value
- [ ] 2.4 Phase 4: execution derives and the round carries (count degrees, `@limit` and statistics deltas); evictions applied at the round that logs them; the golden `round.frame` regenerated by its generator; replay refusing a falling horizon and a changed derivation version
- [ ] 2.5 `RoundModel` in `lean/` extended with checkpoints, the logged fold horizon and the lanes, `lake build` green with no `sorry` (design D10)
- [ ] 2.6 Phase 5: fuzz seeds for the Round and the write-set record; `scripts/mutants.sh` over each changed crate

## 3. The durable seam → `ultragraphdb_traits_durable` (new), `ultragraphdb_store_durable` (new), `ultragraphdb_transactions_log`

- [ ] 3.1 Scenarios: storage-simulation's fault classes of `STORAGE.md` section 19.1 for files (torn writes at 512-byte boundaries, lost and misdirected writes, latent sector errors, failed syncs, refused punches), as Gherkin
- [ ] 3.2 Phases 1 to 3: `DurableStorage`, `DurableFile` (`write_vectored_at`, `allocate`, `punch_hole`) and `AlignedBuf` moved into `traits_durable`; `CompioStorage` and `SimStorage` moved into `store_durable`; the log depending on them
- [ ] 3.3 Phase 4: allocation and punching through rustix on compio's blocking pool (T10); the simulator's new fault classes; the log's suites green unchanged
- [ ] 3.4 Phase 5: `scripts/mutants.sh` over the three crates

## 4. The format → `ultragraphdb_store_tile_format` (new)

- [ ] 4.1 Scenarios: tile-format, as Gherkin, each bound to its vector file
- [ ] 4.2 Phases 1 to 3: FOR, DELTA, BITMAP, BYTES, FIXED and FILTER with the choice rule; sortable encodings and comparison forms of every `CATALOG.md` tag; term encodings; tokens; `esc`; canonical keys with (type prefix, slot) pairs; the tile header, directory and reference; adjacency blocks of kinds 1 to 5 with sections 1 to 14 and inline columns; column chunks of kinds 1 to 3 with overlays and halving; record runs of kinds 1 to 20; digests and the checkpoint identity
- [ ] 4.3 Phase 4: every vector of `encodings.json`, `sortable.json`, `tiles.json`, `kinds.json`, `compaction.json` and `refusals.json` encoded byte for byte, decoded back, and every refused case refused with its refusal in section 19.4's order
- [ ] 4.4 Phase 5: fuzz targets for every decoder, seeded with the vectors; `scripts/mutants.sh`

## 5. The grid → `ultragraphdb_store_grid` (new)

- [ ] 5.1 Scenarios: tile-grid, as Gherkin
- [ ] 5.2 Phases 1 to 3: the sans-I/O grid: extents and the three streams, the lowest-free-extent rule, release and reuse, the free set, the manifest's leaves and root, the directory, the superblock's four copies and its selection, the reserve, `GridFull` and its logged exit, checkpoints with the drain and the frees at the next round's boundary, recovery, repair, scrubbing, the cleaner once per boundary
- [ ] 5.3 Phase 4: the compio shell; `grid.json` and `decisions.json` reproduced; recovery's refusals (`SuperblockDiverged`, `SuperblockLost`, `DerivationMismatch`, `Identity`) under the simulator's faults
- [ ] 5.4 Phase 5: fuzz targets for the superblock and the manifest; `scripts/mutants.sh`

## 6. The engine on the generic layout → `ultragraphdb_store_tile` (new), `ultragraphdb_tool_test`, `ultragraphdb_tool_bench`

- [ ] 6.1 Scenarios: tile-engine and storage-lanes, as Gherkin
- [ ] 6.2 Phases 1 to 3: slots per (bucket group, type) and their maps; the delta and its pool; visibility, the logged fold horizon with builds as a minimum, storage classes, folding; apply from the round alone with the slot-map pieces execution pins; strict admission; due reasons, the queue, units, rewrites with merge, fold, cut, halving and the next-tile merge; statistics, `@limit` and referrer records additive; drops, purge markers and builds; lanes L, S and X with logical resume keys; the generic layout
- [ ] 6.3 Phase 4: `engine.json` reproduced: every rewrite, checkpoint and the final grid, and the crash at round 30 recovered and replayed to the same bytes
- [ ] 6.4 Phase 4: the differential test, every workload of `tool_test` answering equally on the tile engine and the KV engine (design D4)
- [ ] 6.5 `wal_bench` ported with an engine switch; experiments E1, E2, E3 and E6 run and recorded in the design note's section 7
- [ ] 6.6 Phase 5: `scripts/mutants.sh` over `store_tile`

## 7. The simulator → `ultragraphdb_store_tile_sim` (new), `ultragraphdb_store_sim`

- [ ] 7.1 Scenarios: tile-simulation, as Gherkin
- [ ] 7.2 Phases 1 to 3: the shadow model; two replicas fed one log; every fault class of section 19.1; the invariants of section 19.3 checked at every boundary
- [ ] 7.3 Phase 4: replicas byte-identical at every checkpoint under every fault class; `GridFull` reached and left by its logged checkpoint round; the KV parts of `store_sim` retargeted
- [ ] 7.4 Phase 5: `scripts/mutants.sh` over both crates

## 8. The runtime → `ultragraphdb_cluster_partition`, `ultragraphdb_common_storage`, `ultragraphdb/ultragraphdb_storage`, `ultragraphdb_common_config`

- [ ] 8.1 Ultra Style lift of each existing crate the group changes that earlier groups did not lift
- [ ] 8.2 Scenarios: partitions' and role-binaries' requirements of this change, as Gherkin
- [ ] 8.3 Phases 1 to 3: the turn of intake, commit batch, lanes, one beat, one checkpoint step and one I/O submission; the sync split into issue and completion; bounded channels that wake the core; `single_issuer` and `defer_taskrun` with their fallback logged
- [ ] 8.4 Phase 4: checkpoint, trim and recovery from the grid's superblock; fjall's workers, the discard thread and `ThreadExecutor` retired; the `[tiles]` settings fixed cluster-wide and checked equal across storage instances; experiments E9 and E10 recorded
- [ ] 8.5 Phase 5: `scripts/mutants.sh` over each changed crate

## 9. The wire and the read kernels → `ultragraphdb_protocol_wire`, `ultragraphdb_common_compute`, `ultragraphdb_common_frontend`, `ultragraphdb_cluster_partition`, `ultragraphdb_client`, `ultragraphdb/ultragraphdb_compute`, `ultragraphdb/ultragraphdb_frontend`

- [ ] 9.1 Ultra Style lift of each existing crate the group changes that earlier groups did not lift
- [ ] 9.2 Scenarios: wire-protocol's, quad-reads' and ingest's requirements of this change, as Gherkin
- [ ] 9.3 Phases 1 to 3: Welcome's `value_bytes_max`; Scan's spaces 1 and 2 and Entries' inline widths; Frontier and Neighbours with kinds 1 to 6, every-predicate requests, 48-byte filters, request-order rows; Ingest (26) with its outcomes; code 5 and 6 details; the level-to-lane table; results partition by partition with logical resume keys; retention by the logged fold horizon, `read_retain_ms` removed
- [ ] 9.4 Phase 4: compute and the frontend on the new kernels; Ingest split, blank-node groups, key and `@idempotency` claims; both clients; every golden frame of a changed body regenerated by its generator; experiment E12 recorded
- [ ] 9.5 Phase 5: fuzz seeds for every changed body; `scripts/mutants.sh` over each changed crate

## 10. Tokenizers → `ultragraphdb_core_tok`

- [ ] 10.1 Scenarios: tokenizers' requirements of this change, as Gherkin, with vectors from `STORAGE.md` section 5.3
- [ ] 10.2 Phases 1 to 4: the registry's identifier bytes; range, interval and count tokens as the identifier byte and the comparison form; the `datetime` tokenizer replacing `hour` and `day`; the Dgraph golden comparisons replaced, after the user's approval of the files removed
- [ ] 10.3 Phase 5: `scripts/mutants.sh`

## 11. The KV store removed → the workspace

- [ ] 11.1 The differential test of task 6.4 passing on every workload, recorded
- [ ] 11.2 After the user's approval: `ultragraphdb_store_kv`, `ultragraphdb_traits_store`, `ultragraphdb_store_posting`, `ultragraphdb_core_codec` and `ultragraphdb_store_hnsw` deleted with their specifications (`kv-store`, `posting-lists`, `posting-cache`, `uid-lists`, `vector-index` retired); fjall and moka out of `[workspace.dependencies]`; `ultragraphdb_core_algo` kept only if compute uses its sorted-set operations
- [ ] 11.3 The four decoder lints (`indexing_slicing`, `arithmetic_side_effects`, `cast_possible_truncation`, `cast_sign_loss`) enabled globally (T41)
- [ ] 11.4 Every suite green on the tile store alone; two simulated replicas byte-identical under every fault class; `RoundModel` proved

## 12. The benchmark and the net impact → `ultragraphdb_tool_bench`, `openspec/benches/`

- [ ] 12.1 The benchmark of `openspec/benches/benches-2026-11-09.md` rerun on the tile store exactly as that file states it: the same commands, configuration files, hosts, cores, data sizes and repetitions, in process and through the binaries, each figure with its spread
- [ ] 12.2 The run recorded as a dated baseline `openspec/benches/benches-YYYY-MM-DD.md` in the 2026-11-09 file's structure: what ran, every table in full, the commit, the commands and the environment
- [ ] 12.3 The comparison table of the net impact: every measurement of 2026-11-09 beside its tile store figure, with the change in absolute and relative terms and whether it lies outside both runs' spread; added to the dated file and summarised in the design note's section 7 against the targets of its section 3
- [ ] 12.4 Each regression outside the spread root-caused and fixed, or recorded with its cause and the user's decision (a big gap from the in-process ceiling is a software defect, never the network's or the virtual machine's); an optimisation kept only where throughput rises outside the spread
- [ ] 12.5 The exit gate: `ROADMAP.md`'s stage 6.5 row marked done with the comparison's headline figures, and stage 6.6 resumed at group 2
