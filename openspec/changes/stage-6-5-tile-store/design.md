## Context

The design is `openspec/notes/UltraGraphDB Scalable Tile Store.md` (decisions T1 to T48), its feasibility study
`openspec/notes/UltraGraphDB Tile Store Feasibility.md` (blockers B1 to B5, the gap tables, the crate plan of its
section 5 and the order of its section 6), and the consistency review `openspec/notes/UltraGraphDB Spec Consistency
Review.md` (C1 to C48). The bytes are `specification/storage/STORAGE.md`; the wire and the log are
`specification/binary_protocol/PROTOCOL.md`; the logical obligations are `specification/schema/SCHEMA.md` section 12.
Where this file and those disagree, those win and this file is corrected.

The code today: every partition owns a fjall keyspace through `ultragraphdb_traits_store::Store`, borrowed across
turns by transactions and Scans (B2); quads, lists, postings and the dictionary are byte layouts of
`ultragraphdb_core_keys` and `ultragraphdb_core_codec`; the write-set record names physical list keys; the log's
superblock and `SimStorage` live inside `ultragraphdb_transactions_log`, above everything a grid would need; reads are
KV-shaped Scans with version-suffixed keys and a per-node retention window; Dgraph-equal tokens decide the index bytes.
Stage 6.6 has finished its group 1, the wire for UGS, and is paused until this change is done (`ROADMAP.md`).

Stakeholders: the user, who commits after each group; stage 6.6, which resumes on this engine; stage 8, which
distributes it.

## Goals / Non-Goals

**Goals:**

- The engine of `STORAGE.md` sections 2 to 19 on the generic layout, reproducing every conformance vector.
- Every crate that reads or writes storage, the log or the read and write kernels following `PROTOCOL.md` sections 5.2,
  6.6, 6.8, 9.4, 11.5, 11.8, 11.9, 11.12, 12 and 13.
- The KV store, its traits, posting lists, codec and HNSW index removed, every suite green without them.
- Two simulated replicas byte-identical under every fault class.

**Non-Goals:**

- The compiled layout and governed storage: stage 6.6's groups 3, 4, 6 to 8 and 9.2 fill the `Layout` interface this
  change defines and reads. A tenant here has the generic layout.
- Distribution (stage 8): state sync, bucket-group moves and the sequencer's shares are specified and left unbuilt;
  their rules (`STORAGE.md` sections 16.8 and 17.6) stay testable in the simulator only where a single partition can
  exercise them.
- The vector index, rebuilt on adjacency blocks in stage 8.
- The query core (stage 7) and the LDBC comparison (E7).

## Decisions

**D1. Crates.** The feasibility study's section 5, with its two open placements closed:

| Crate | Holds | Depends on |
|---|---|---|
| `ultragraphdb_error_tile` | `TileError(pub TileErrorEnum)`: every refusal of `STORAGE.md` section 19.4 and the store's commit refusals | `ultragraphdb_error_invariant` |
| `ultragraphdb_traits_durable` | `DurableStorage`, `DurableFile` (with `write_vectored_at`, `allocate`, `punch_hole`), `AlignedBuf`, moved out of `transactions_log` | none |
| `ultragraphdb_store_durable` | `CompioStorage` and `SimStorage`, moved out of `transactions_log`, the sim extended with the grid's fault classes | `traits_durable`, `compio`, `rustix` |
| `ultragraphdb_store_tile_format` | section 1 and sections 4 to 11: encodings, filters, header, directory, record runs, adjacency blocks, column chunks, digests, references, the refusals; no I/O | `core_ids`, `error_tile`, `xxhash-rust` |
| `ultragraphdb_store_grid` | sections 12, 13.1 to 13.3, 16.5, 16.6 and 17: extents, streams, free set, manifest, directory, superblock, checkpoints, recovery, repair, scrubbing, the cleaner; a sans-I/O core with a compio shell | `tile_format`, `traits_durable`, `error_tile` |
| `ultragraphdb_traits_engine` | the engine's interface in logical change types: rounds to apply, lane requests, answers; the `Layout` interface stage 6.6 fills | `core_ids`, `error_tile` |
| `ultragraphdb_store_tile` | sections 13.4, 14 to 16 and 18: slots, the delta, versions and the logged fold horizon, classes, apply, admission, compaction, statistics, drops, purges, builds, lanes | `tile_format`, `store_grid`, `traits_engine`, `core_ids`, `core_terms`, `core_mapping`, `core_tok`, `error_tile` |
| `ultragraphdb_store_tile_sim` | section 19.5: the engine under every fault class against a shadow model; the reference engine's run; replica identity | `store_tile`, `store_grid`, `store_durable`, `traits_engine` |

`traits_engine` names logical change types only, never wire views, so no trait crate sits above `protocol_wire`; the
partition converts wire views to logical changes. `CompioStorage` and `SimStorage` take a crate of their own,
`store_durable`, because the grid and the log both need them and neither may depend on the other. No LZ4: format 1
holds no general-purpose compression (`STORAGE.md` section 20). *Alternatives:* keeping the implementations in
`transactions_log` puts the grid above the transactions layer; a wire-typed engine trait puts a trait crate above the
protocol.

**D2. Ownership first, over the KV store.** The ownership refactor (B2) lands before any tile exists: the engine,
still fjall behind the logical interface of `traits_engine`, is owned by its core; transactions, Scans and Frontier
requests hold handles; reads restart their attempt on a miss (T33); the commit path writes logical conflict keys and
keeps the two overlays. Every suite stays green, so the KV engine becomes the differential oracle of D4. *Alternative:*
the refactor inside the tile engine's arrival would change ownership and storage at once, with no oracle for either.

**D3. Sans-I/O cores, compio shells.** The format is pure; the grid and the engine are state machines whose outputs
are I/O requests and whose inputs include their completions (`STORAGE.md` section 2); the compio shell performs them
on the core's ring, allocation and punching through rustix on compio's blocking pool (T10, T18: no compio fork). The
simulator performs the same requests under faults, so one code path is tested and shipped.

**D4. Differential testing, then removal.** Every workload of `ultragraphdb_tool_test` runs on both engines behind
`traits_engine` and answers equally before the KV store is removed; the removal is the last group and waits for the
user's approval (`AGENTS.md`: removing files waits for approval).

**D5. The conformance vectors are Bazel data.** `//specification/conformance/storage:vectors` is a data dependency of
the format, grid, engine and sim crates' tests; each test reads its vector file, encodes the input and compares bytes,
decodes and compares the input, and refuses the refused cases with their refusals. The generator is never imported:
the vectors are written from the specification alone, and the code is held to them.

**D6. The generic layout and the `Layout` interface.** The engine instantiates families from a `Layout` it reads per
catalog version: the generic layout is its built-in value for a tenant with no schema; a compiled layout, decoded from
the catalog version's Layout entry (`CATALOG.md` section 5.11), is stage 6.6's to supply. Record runs, identity columns
and slot maps exist for every tenant whatever the layout (T45).

**D7. The wire's breaking changes land together.** Welcome, Scan, Entries, Frontier, Neighbours, Ingest, the Round, the
write-set record's conflict families and the error details change in one group with their golden frames, fuzz seeds and
both clients, so no intermediate state of the wire is committed.

**D8. Tokenizers own their identifier bytes.** The registry assigns identifier bytes in its own order; range, interval
and count tokens are the identifier byte and the value's comparison form (`STORAGE.md` section 5.3); `datetime`
replaces `hour` and `day`; the Dgraph golden comparisons are replaced by vectors written from `STORAGE.md`. Stage 6.6's
`measure` and `decimal` tokenizers follow this rule, and its delta is aligned in this change's first group.

**D9. Determinism is tested, not argued.** Every decision of the engine reads only the rounds, the settings and state
derived from both; the simulator checks two replicas byte for byte at every checkpoint under every fault class, and
the reference engine's run is reproduced exactly. A difference is a bug, never a tolerance.

**D10. The Lean model moves with the log.** `RoundModel` gains checkpoints, the logged fold horizon and the lanes in
the group that changes the Round, proved before the group closes.

**D11. Method.** Every group follows `openspec/protocols/tdd-protocol.md`'s five phases, as stage 6.6's groups do: the
public surface with bare `unimplemented!()`; the suite written against it and observed failing; its inputs reviewed
against the corner cases; the implementation; then, as the group's last step, `scripts/mutants.sh` once per crate it
changed and its fuzz targets written. An existing crate is lifted into `openspec/protocols/ULTRA_STYLE.md` in a commit
of its own before its behaviour changes; new crates conform from their first commit. Bazel is the gate: `make test`
cached; the user commits after each group.

## Risks / Trade-offs

- [The engine does not beat the KV baseline] → experiments E1 to E3 and E6 run in group 5, before the runtime and the
  removal; a regression is root-caused and fixed, low throughput being a software defect, and group 12 measures the
  net impact against the KV baseline.
- [The C10 reserve is an estimate, not a proven bound] → `PlacementFound` stops a replica that would place without a
  free extent; the simulator drives grids to `GridFull` under every fault class and checks the logged exit.
- [Strict admission refuses an entry whose changes alone exceed the watermark's share] → that refusal is code 5
  `detail` 1 and documented; E5 and the Ingest benchmark measure how often realistic batches meet it.
- [The ownership refactor breaks suites far from storage] → it lands over the KV store with every suite green before a
  tile exists (D2).
- [Two engines behind one interface for several groups] → the KV engine is frozen after D2 except for fixes; its
  removal is a scheduled group, not open-ended.
- [Byte-level drift between generator and code] → the code is never generated from the generator; both follow
  `STORAGE.md`, and a mismatch is resolved in `STORAGE.md` first.

## Migration Plan

No outside users: logs, grids and KV directories written before this change are not readable after it . Each group lands as one commit after `make test`; rollback is the previous commit. The KV store is
deleted only in the last group, after the differential test passed on every workload and the user approved the
deletion. Stage 6.6 resumes at group 2 when this change is archived.

## Open Questions

None blocks the first group. The experiments' measurements (E1 to E14) set defaults that `STORAGE.md` section 3.1
holds today as estimates; a changed default changes the derivation version and is recorded in `STORAGE.md`.

For stage 8:

- Ingest resends after a bucket group moves: which table answers a share whose rows changed owner?
