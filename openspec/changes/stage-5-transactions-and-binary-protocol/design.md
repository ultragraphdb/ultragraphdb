## Context

The design behind this change is `openspec/notes/ zero-copy-binary-protocol.md`, approved on 2026-10-27: the evidence
for each decision, from the code, the literature and Apache Iggy's and TigerBeetle's sources, is there. This document
states how the code gets there.

Today:
- `ChangesetCore::apply_batch` (`ultragraphdb_store_apply/src/types/changeset_core/mod.rs:100-150`) executes every
  changeset of a batch under one lock, each request reading at `latest_ts` with the earlier requests' deltas stacked on
  top (`StackedLayers`), and resolves terms inside that lock.
- `Lists::commit_with` (`ultragraphdb_store_posting/src/types/lists/lists_commit.rs:61-160`) runs a check hook per
  request under the writer lock, gives each a fresh timestamp from `Timestamps`, writes one store batch with the applied
  timestamp and updates the posting cache. `QuadStore::commit`
  (`ultragraphdb_store_quads/src/types/quad_store/mod.rs:90-142`) is the only validation: an IRI created twice, or a read of a key a refused request of the batch wrote.
- `Request::read_keys` records whole list keys; scans and dictionary lookups are not recorded.
- Rollups draw a fresh timestamp from the commit counter (`lists_rollup.rs:166-171`).
- The default durability is `Durability::Process`: batches reach the OS and the store syncs once a minute.
- `ultragraphdb_proto_pb`'s `PostingList`, `Posting`, `UidPack`, `UidBlock` and `List` are the posting lists' in-memory
  and on-disk form, encoded by prost in `posting_codec.rs`.
- No server, no client, no compio.

## Goals / Non-Goals

**Goals:**
- Read-write transactions executed on n−1 worker cores, strictly serializable, with one commit step.
- Every acknowledged commit durable on any drive, through a write-ahead log of the project's own.
- The ordered-log interface, the round format and the commit step shaped so stage 10 changes transport and replication,
  not semantics.
- The binary protocol, its conformance suite, the Rust client and the single-node server.
- No protobuf and no gRPC of the repository's own.
- The properties proved in Lean, checked by the simulator, Elle and a white-box checker, and by kill tests.

**Non-Goals:**
- SPARQL and GQL (stage 6): stage 5's statements are quad patterns.
- TLS, access control and namespaces as tenants (stage 7).
- The commit-log feed API (stage 7): the write-set record reserves its section, and no read API is built.
- Backups and loaders (stage 7); Bolt (stage 9).
- VSR, the ordering service across nodes, tiles (stage 10).
- Read summaries in the commit step: they are measured, not used (D10).
- Reordering within a round, unless the Zipf measurement asks for it (D6).
- Views over stored posting bytes and a zero-copy `Store::Item`: built only if a read-heavy measurement pays for them,
  step 2 of the posting format.

## Decisions

### Crates and layering

| Crate | Folder | Depends on | Holds |
|---|---|---|---|
| `ultragraphdb_error_protocol` | `ultragraphdb_error/` | — | `ProtocolError(pub ProtocolErrorEnum)` |
| `ultragraphdb_error_log` | `ultragraphdb_error/` | — | `LogError(pub LogErrorEnum)` |
| `ultragraphdb_traits_log` | `ultragraphdb_traits/` | `ultragraphdb_error_log` | `OrderedLog`, `Position`, `RoundRef`, `LogEvent` |
| `ultragraphdb_protocol_wire` | `ultragraphdb_protocol/` | `ultragraphdb_error_protocol`, `ultragraphdb_core_terms`, `xxhash-rust` | Header, commands, checksums, chunk tables, bodies with their views and builders, the round and write-set record layouts; no I/O |
| `ultragraphdb_protocol_bus` | `ultragraphdb_protocol/` | `_wire`, `compio` | Connections, framing reads, buffer pools, streams, credit; a simulated network |
| `ultragraphdb_protocol_client` | `ultragraphdb_protocol/` | `_wire`, `_bus` | The Rust client |
| `ultragraphdb_transactions_txn` | `ultragraphdb_transactions/` | `_wire`, `ultragraphdb_store_apply` | `Transaction` on a worker: snapshot, `ReadSet`, write-set builder, limits |
| `ultragraphdb_transactions_log` | `ultragraphdb_transactions/` | `ultragraphdb_traits_log`, `_wire`, `compio` | The WAL: `DurableStorage`, `DurableFile`, segments, superblock, scan, recovery; compio and simulated storage |
| `ultragraphdb_transactions_commit` | `ultragraphdb_transactions/` | `_txn`, `ultragraphdb_traits_log`, `_wire`, `ultragraphdb_store_apply` | Sequencer, resolver, conflict history, commit-time operations, apply, watermark, sessions |
| `ultragraphdb_transactions_sim` | `ultragraphdb_transactions/` | the above, `ultragraphdb_store_sim` | The node simulator, the white-box history checker |
| `ultragraphdb_server_session` | `ultragraphdb_server/` | `_wire`, `_bus`, `ultragraphdb_transactions_*` | Handshake, sessions, dispatch to workers, describe |
| `ultragraphdb_server_node` | `ultragraphdb_server/` | `_session`, `ultragraphdb_common_telemetry`, `ultragraphdb_store_kv` | The server binary |

- The root `Cargo.toml` gains the globs `ultragraphdb_protocol/*`, `ultragraphdb_server/*` and
  `ultragraphdb_transactions/*`, and loses `ultragraphdb_proto/*`.
- `compio` (pure Rust, io_uring on Linux) and `xxhash-rust` (feature `xxh3`) are added to `[workspace.dependencies]`.
  The bounded queues between workers and the commit step use `futures-channel`, part of the `futures` the workspace
  already pins.
- The Lean model is a Lake project in `tools/round_model/`, beside `tools/golden`, built outside Bazel as MGQL's Lean
  is; its toolchain is pinned in `lean-toolchain` to the release MGQL uses.

### The posting format (first, because it removes protobuf from storage)

- `UidPack`, `UidBlock`, `List`, `Posting`, `PostingList` and the backup list become plain Rust types in
  `ultragraphdb_core_codec`, with the fields the code reads; a field of the protobuf messages that no code reads is
  not carried, and the task lists them.
- The encoder and decoder move from `store_posting/src/utils/posting_codec.rs` to `core_codec`. The format is
  little-endian: a version byte, a flags byte, the commit timestamp, the pack's blocks (base, count, group-varint
  deltas as today, so `uid-lists`' byte-identity requirement still holds for the blocks), the postings, the splits,
  then an XXH3-64 checksum of everything before it. A decode that fails the checksum or a bound is
  `PostingError::Corrupted` naming the key.
- `core_algo`, `store_posting`, `store_quads`, `store_apply` and `store_hnsw` import the types from `core_codec`.
- `core_types`' `object_value`, `get_geo_tokens`, `match_geo`, `sort_with_facet` and `core_tok`'s
  `get_index_factory_from_spec` go with their tests, so neither crate names a protobuf type.
- Then `ultragraphdb_proto_pb` and `ultragraphdb_proto_dgraph` are removed (with the user's approval at that task), with
  the prost toolchain, `MODULE.bazel`'s `protobuf` and `rules_rust_prost`, and the workspace pins.

### Execution leaves the commit lock

`ChangesetCore::apply_batch` splits into two halves:

1. **`execute`, on a worker:** a `Request` at the watermark W with no stacked layers, the catalog as of W, terms resolved
   through the dictionary with provisional UIDs from `Leases` (already shared and lock-guarded), `ChangesetWrite`
   unchanged in what it writes. Its output is an `Executed`: the request's deltas and entries, the `WriteTerms`, the
   read set, catalog changes and a pending rebuild.
2. **`apply_round`, on the commit step:** the admitted `Executed`s of a round written as one store batch at their commit
   versions, with `PersistMode::Buffer`; catalogs installed; drops and rebuild starts performed in round order.

`Batch` and `StackedLayers` stop being used by transactions: a transaction reads its own writes through its `Request`,
never another's. They stay for the rebuild's internal use while it needs them.

### Read sets at statement granularity

- `Request` records three kinds of read beside `read_keys`: a tested statement (list key and object encoding), a scanned
  range, and nothing for blind writes, as today.
- `QuadWrite::objects`, used by an insert to learn whether the statement is held, is replaced for that use by a
  `holds(graph, subject, predicate, object)` that reads the list as before but records the statement, not the list.
  Deletes, overwrites and unique checks keep their list reads.
- `QuadStore` records dictionary absences; the scans in declaration checks (`predicate_prefix`) record their ranges.
- `ultragraphdb_transactions_txn::ReadSet` gathers them; the catalog range of the namespace is always in it.

### The commit step

A single thread with one compio runtime owns the sequencer, the resolver, the WAL and the store apply. It is a step
function over its inputs (commit requests, sync completions), so the simulator drives it without a runtime.

- **Rounds:** the commit requests waiting when the previous round's sync completes, bounded by bytes (the 1 MiB frame)
  and a count; the bounds' defaults follow the power-loss-protection flag.
- **Versions:** `max(previous + 2, 2 × clock µs)`, even; the round records its first version and count. `Timestamps`
  loses its role as the commit counter; it keeps open reads and the discard bound.
- **Resolver:** the conflict history is a `BTreeMap` of written keys and statements to versions and an interval map of
  written ranges, pruned below the read version of the oldest open read-write transaction. Each transaction: check its
  read set against the history above its read version, then against the round's admitted writes; admit, then add its
  writes. `QuadStore::confirm`'s new-IRI check becomes one rule of it.
- **Commit-time operations** run after a transaction is admitted, against the store at W plus the round's admitted
  writes, through the posting cache. Insert-if-absent and upsert-and-link resolve the node through the unique
  predicate's index, draw a UID from the leases when they create it, and build a small changeset with the UID filled
  in (the creation properties when created, and an upsert-and-link's links) that `ChangesetWrite` applies, so tokens,
  reverse lists, statement IDs, statistics and vectors follow the same code as every write. Placeholders that let any
  write name an unresolved UID were considered and not taken: they reach every key, index and statistic a write
  derives, where upsert-and-link bounds the commit step's work to one node and its links.
- **Pipeline:** round k+1 is resolved while round k syncs; round k is applied once synced; the watermark passes k once it
  is applied; replies go out.
- **Rollups** read at W and write one above the newest commit version they fold, as `posting-lists` already states for
  a finite read timestamp; they never take a version from the sequencer.

### The WAL

As the note's "The WAL, after Apache Iggy's": segment files of a fixed size, preallocated; a round written as its frame,
padded to 4,096-byte blocks, with one aligned write and one `fdatasync`; records self-identifying by position, parent
checksum, cluster and incarnation; Iggy's ping-pong superblock with its three outcomes; Iggy's `DurableStorage` seam and
its simulated storage model, written here without `unsafe` (the commit step owns the files; methods take `&mut self`).
Recovery reads the store's applied version, finds its round, and replays the rounds after it.

### Sessions

Registered through a round; held in the keyspace without versions, written in the round's batch, so recovery restores
them with the store and replays the rest. Bounded; the oldest evicted.

### The protocol on compio

Workers are compio runtimes pinned to n−1 cores; a connection is accepted onto the least-loaded worker and stays there;
frames are read header first, then body into one pooled buffer; the body is checked once and then read through views.
Results stream as frames on the request's stream while credit lasts.

### Fuzzing

Mutation testing asks whether the suite notices a wrong line; it cannot find the input nobody thought of, which is where
decoders of untrusted bytes fail. Every decoder of bytes from a client, a peer or the disk is therefore fuzzed:

- **Tools:** `cargo-fuzz`, the Rust Fuzz project's recommended driver, over libFuzzer, with `arbitrary` for
  structure-aware inputs (a valid frame built from arbitrary fields, then mutated, reaches the body checks a random
  byte string rarely passes). `cargo-fuzz` needs a nightly toolchain, which the machine has, and builds libFuzzer from
  C++ through `libfuzzer-sys`. That C++ dependency exists only in the fuzz packages: no shipped crate depends on it, so
  the pure-Rust rule for shipped binaries holds.
- **Layout:** each fuzzed crate holds a `fuzz/` package of its own, as `cargo fuzz init` makes it, outside the
  workspace's member globs and outside Bazel; its targets call the crate's public decoders.
- **Properties:** never panic; accept or refuse with the typed error; re-encode an accepted value to the same bytes
  where the format has one encoding; the WAL scan never accepts a round whose checksums fail.
- **Seeds:** the conformance suite's golden frames and the posting format's golden values, so a run starts from inputs
  that pass the outer checks.
- **Running:** `scripts/fuzz.sh [crate [target]]`, shaped as `scripts/mutants.sh`: it checks that `cargo-fuzz` is
  installed, runs each target time-boxed (`-max_total_time`), and reports executions, coverage and failing inputs.
  Fuzzing is not hermetic and has no fixed end, so it stays outside Bazel, as mutation testing does.
- **What Bazel keeps:** every failing input is minimized (`cargo fuzz tmin`) and committed under the crate's
  `tests/fuzz_regressions/`, and a unit test replays each one, so the hermetic suite carries every finding.
- **In the protocol:** phase 5 of a group that decodes bytes is mutation testing and fuzzing; stage 5.5 runs longer
  campaigns.

### The vector indexer

A worker task per predicate with a vector index reads committed rounds in order (through the commit step's notification
of applied versions), inserts their vectors and records the version it has indexed. A search reads the HNSW lists, then
drops any node whose vector is not visible at its snapshot.

## Risks / Trade-offs

- [Every round is written twice, to the WAL and fjall's journal] → measured in stage 5; option C (fjall without its
  journal) is the recourse and needs a fork.
- [fjall's prefix recovery after a power loss is modelled by the simulator, not injected into fjall] → kill tests on
  the binary, and recovery refuses a store ahead of the WAL or with a gap.
- [The commit step becomes the ceiling] → its cost per transaction is measured against the execution cost divided by
  n−1; history checks move to the workers first.
- [Contention under optimistic execution] → commit-time operations; the Zipf workload at 8 writers measures both forms.
- [A search misses vectors committed but not yet indexed] → the indexer's lag is measured and reported.
- [Removing `StackedLayers` from transactions changes `local-commit`'s scenarios] → the delta spec rewrites them; the
  scenarios that a request reads its own writes stay.
- [The Lean model and the Rust code drift apart] → the simulator's invariants restate the model's theorems, and each
  model change names the code it covers.
- [Clock-based versions skew on a runaway clock] → `previous + 2` absorbs steps back; stage 10 bounds the clock.
- [A drive that tears the last round while reporting its sync complete loses that round on one node] → the scan detects
  it by its checksums and reports the truncation by name; from stage 10 a replica repairs it from a peer.
- [Elle runs on the Java virtual machine] → it runs outside Bazel, as the Lean model does, through the Rust client.
- [`cargo-fuzz` needs nightly and builds libFuzzer from C++] → only the fuzz packages depend on it, outside the
  workspace and Bazel; findings enter the hermetic suite as replayed regression cases.

## Migration Plan

No outside users: stores written before this stage are refused at open by the posting format's version byte and the
missing WAL superblock. Data enters again through stage 7's loaders or is regenerated.

## Open Questions

None: every decision was taken on 2026-10-27 in the note.
