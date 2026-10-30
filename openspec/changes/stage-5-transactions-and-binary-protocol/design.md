## Context

The design behind this change is ` zero-copy-binary-protocol.md` in this folder, approved on 2026-10-27: the evidence
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
- RBAC enforcement, the security catalog and namespaces as tenants (stage 7), and TLS (later): `openspec/notes/security.md`
  designs them now, and stage 5 builds the protocol's part and authentication (group 9c, 10.7).
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
| `ultragraphdb_protocol_transport` | `ultragraphdb_protocol/` | `_wire` | The transport that carries the wire protocol, sans-I/O and with no runtime: the pull-style `Framer` (it tells a shell how many bytes the frame's pooled buffer must hold, so an owned-buffer read or a `&mut [u8]` read lands bytes where the frame stays), the handshake machines `ClientHandshake` and `ServerHandshake`, `Outbound` (streams, credit, cancel), `BufferPool` (`Send`), and the simulated network's model (renamed from `_bus` and made sans-I/O, decided 2026-10-28) |
| `ultragraphdb_protocol_transport_compio` | `ultragraphdb_protocol/` | `_transport`, `_wire`, `compio` | `Connection`, the thin compio shell obeying the transport's machines, and `CompioSimStream`, the simulated stream as a compio stream; shared by the server and the compio client (decided 2026-10-28) |
| `ultragraphdb_client_core` | `ultragraphdb_client/` | `_transport`, `_wire` | The Rust client, sans-I/O and with no runtime: handshake through the transport's machine, sessions, transactions, requests in the wire's bodies, replies and result batches read through views, streams, credit, cancel, timeouts, and commits retried only through their session, the time passed in; the test server behind `test-utils` (decided 2026-10-28) |
| `ultragraphdb_client_tokio` | `ultragraphdb_client/` | `_client_core`, `_transport`, `tokio` | The primary public Rust client: the core's tokio shell, `Send` futures over TCP; what the conformance suite, Elle and every stage-5 test drive the server with (decided 2026-10-28: most Rust applications run tokio) |
| `ultragraphdb_client_compio` | `ultragraphdb_client/` | `_client_core`, `_transport_compio`, `compio` | The public compio client: the core's compio shell over the transport's compio `Connection`, thread per core (decided 2026-10-28) |
| `ultragraphdb_transactions_txn` | `ultragraphdb_transactions/` | `_wire`, `ultragraphdb_store_apply` | `Transaction` on a worker: snapshot, `ReadSet`, write-set builder, limits |
| `ultragraphdb_transactions_log` | `ultragraphdb_transactions/` | `ultragraphdb_traits_log`, `_wire`, `compio` | The WAL: `DurableStorage`, `DurableFile`, segments, superblock, scan, recovery; compio and simulated storage |
| `ultragraphdb_transactions_commit` | `ultragraphdb_transactions/` | `_txn`, `ultragraphdb_traits_log`, `_wire`, `ultragraphdb_store_apply` | Sequencer, resolver, conflict history, commit-time operations, apply, watermark, sessions |
| `ultragraphdb_transactions_sim` | `ultragraphdb_transactions/` | the above, `ultragraphdb_store_sim` | The node simulator, the white-box history checker |
| `ultragraphdb_server_session` | `ultragraphdb_server/` | `_wire`, `_transport`, `_transport_compio`, `ultragraphdb_transactions_*` | Handshake, sessions, dispatch to workers, describe |
| `ultragraphdb` | `ultragraphdb_server/` | `ultragraphdb_server_session`, `ultragraphdb_common_telemetry`, `ultragraphdb_store_kv` | The server: its library and the `ultragraphdb` binary (renamed from `ultragraphdb_server`, 2026-10-30) |

- The root `Cargo.toml` gains the globs `ultragraphdb_protocol/*`, `ultragraphdb_server/*` and
  `ultragraphdb_transactions/*`, and loses `ultragraphdb_proto/*`.
- `compio` (pure Rust, io_uring on Linux) and `xxhash-rust` (feature `xxh3`) are added to `[workspace.dependencies]`.
  The bounded queues between workers and the commit step use `futures-channel`, part of the `futures` the workspace
  already pins.
- The repository's Lean proofs live in `lean/`, one Lake project whose libraries hold them; the round model is its
  library `RoundModel`. It is built outside Bazel as MGQL's Lean is, and its toolchain is pinned in `lean-toolchain`
  to the release MGQL uses.

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
- **Resolver:** the conflict history is a sequence of sorted runs, oldest first (replacing the `BTreeMap` of written
  keys and the interval map of written ranges sketched here before; approved 2026-10-28): each admitted transaction's
  conflict points, statements and ranges become a run of their own, its keys held once in one byte arena and named by
  u32 spans, sorted and deduplicated with the newest version that wrote each; the newest run is merged into the one
  before it while that one is no larger, a binary counter capped at 16,384 entries a run, so the young end holds a
  logarithmic number of small runs. Pruning below the read version of the oldest open read-write transaction drops
  whole runs from the old end; a merge leaves out what is at or below the floor. Each transaction is checked once, in
  round order, against every run holding a version above its read version, the round's admitted transactions being in
  the history at their versions: a merge join of its sealed read set against each run, exact byte for byte. A run
  records the bounds of its keys, its first and last point and the lowest start and highest end of its ranges, and a
  check skips a run whose bounds cannot hold a read's key; the bounds only skip and never decide (approved 2026-10-28;
  a probabilistic filter was not taken). A written statement also counts as its list touched, an absent key conflicts
  as a list read does, a written range conflicts as a drop with every read sharing a key with it, and the kind reported
  is the first of list, statement, range, absent term and catalog, then drop (the conflict table of `transactions`,
  amended 2026-10-28). `QuadStore::confirm`'s new-IRI check becomes one rule of it.
- **Commit-time operations** run after a transaction is admitted, against the store at W plus the writes of every round
  decided above W (the round still syncing included) and the current round's admitted writes, through the posting
  cache. The commit step keeps those decided but unapplied writes in memory until their round is applied. Reading the
  store at W alone loses updates under pipelining: a counter add in round k+1 would read the value from before round
  k, which is still syncing (found by the Lean model, `RoundModel.serializable`). Insert-if-absent and upsert-and-link resolve the node through the unique
  predicate's index, draw a UID from the leases when they create it, and build a small changeset with the UID filled
  in (the creation properties when created, and an upsert-and-link's links) that `ChangesetWrite` applies, so tokens,
  reverse lists, statement IDs, statistics and vectors follow the same code as every write. Placeholders that let any
  write name an unresolved UID were considered and not taken: they reach every key, index and statistic a write
  derives, where upsert-and-link bounds the commit step's work to one node and its links. A predicate's implicit
  default catalog entry, recorded by its first write, is an insert-if-absent on its catalog key, not a catalog change,
  so concurrent first writes of one predicate both commit as stage 4's schema storage requires; a declaration remains a
  catalog change and aborts its namespace's concurrent writers (decided 2026-10-27).
  As built (2026-10-28, 7c): the operations run between the resolver's checks and the version, so a refused one takes
  no version and adds nothing to the history; they run in a write of their own seeded with the transaction's
  (`QuadWrite::seed`: its terms, its deltas as the request's own blind writes, its statistics merged), over the
  posting lists' stacked layers (`Batch::stack`) of the round syncing and the current round's admitted proposals,
  built at a round's first operation and dropped when the next round begins, and with the IRIs those proposals created
  known by their UIDs, whose dictionary entries no layer holds. The transaction's record is written again from the
  result and is what the history admits, the round logs and the apply writes, so a replay applies what was applied and
  never runs an operation again; the wire format is unchanged. A record that no longer fits its round waits for the
  next, decided there from the start. The operations' reads are no part of the read set. Results come with the reply
  in the wire's `OperationResult` (a UID and whether it was created; zero for compare-and-set and counter add).
- **Drops in rounds** (decided 2026-10-28, 7c): a drop is a proposal, an execution writing nothing but the
  predicate's key ranges in each graph holding it and its catalog entry; a round closes after a drop, which
  `apply_round` performs at its version, the round's last, once the batch is written. Its catalog entry lies in the
  namespace's catalog range every transaction reads, so a drop aborts the namespace's transactions that read below it
  as a declaration does; the resolver also checks each transaction's written keys and statements' lists against the
  ranges written above its read version. A round closes before a second declaration starting an index build, so it
  holds at most one; nothing aborts.
- **Reads above the watermark:** the session refuses a request naming a read version above W with an error naming both,
  instead of letting the store's read gate hold a worker until W reaches it (decided 2026-10-27).
- **Pipeline:** round k+1 is resolved while round k syncs; round k is applied once synced; the watermark passes k once it
  is applied; replies go out.
- **Rollups** read at W and write one above the newest commit version they fold, as `posting-lists` already states for
  a finite read timestamp; they never take a version from the sequencer.
- **Every write of the node takes its version from the sequencer** (decided 2026-10-28). An index build's end and a
  predicate's drop are proposals of the commit step, computed at apply time under the apply lock at the version their
  round took, and named in their write-set record's section 10 (`PROTOCOL.md` §12.2): drop a predicate, swap a build
  in, undo a build. A build's end is a round of its own; a drop is its round's last. The pipeline sequences its
  applier's builds (`ChangesetApplier::sequence_builds`): a build done ends with a swap, a build failed or cancelled by
  a drop with an undo, which the pipeline proposes (`Pipeline::poll_builds`); a swap whose build was cancelled after it
  was made is discarded. Rounds apply in order, so the swap's catch-up sees every write committed before it and every
  later transaction aborts on its catalog change. A build-starting declaration that arrives while a build runs waits in
  the step, set aside, until the build's end is admitted, so `apply_round` never refuses a durable round. The physical
  removal of an index no catalog names and a rebuild record's removal take no version: they write at the watermark.
  The applier's direct entry points (`apply`, the drops of a graph, a namespace's data, a namespace and everything)
  remain for tests and tools that run no commit step; the wire carries no drop command yet (`PROTOCOL.md` open point
  28). Recovery redoes a swap round the store had not applied as a build and swap at that round's version before
  replaying later rounds: an index is a function of the data at its version, so the redone index equals the lost one.
- **The operations' overlay holds the dictionary too** (decided 2026-10-28): the entries without versions that the
  unapplied rounds wrote, long literals' strings among them, so a compare-and-set or counter add on a value stored out
  of line reads it rather than being refused.

### The WAL

As the note's "The WAL, after Apache Iggy's": segment files of a fixed size, preallocated; a round written as its frame,
padded to 4,096-byte blocks, with one aligned write and one `fdatasync`; records self-identifying by position, parent
checksum, cluster and incarnation; Iggy's ping-pong superblock with its three outcomes; Iggy's `DurableStorage` seam and
its simulated storage model, written here without `unsafe` (the commit step owns the files; methods take `&mut self`).
Recovery reads the store's applied version, finds its round, and replays the rounds after it (`Replay::run` in
`ultragraphdb_transactions_commit`, 7.9): each committed record's sections 0 to 4 written at its commit version, each
entry with the metadata its keyspace gives every entry of it, so a replayed round writes what its first apply wrote;
its operation, a drop or a build's end, the build restored from the declaration its record's section 3 names; and the
session table's entries, from the registrations and the committed requests with their results. The node calls it
before `Pipeline::new`, with the session capacity it runs with, so evictions replay as they ran. A store ahead of the
log, or written outside it, is refused naming its version; an empty store below a trimmed log, naming the checkpoint.
A store below the first round after a checkpoint is taken as the checkpoint's: the superblock keeps no version, so a
store behind its checkpoint is not detected.

Segment allocation (task 3.9; recycling decided 2026-10-27, option (a)): a segment that has never existed is reserved
with `fallocate(2)` and `FALLOC_FL_KEEP_SIZE` through `rustix` on Linux, not written with zeros, and is as long as its
rounds; macOS reserves nothing. A file system without `fallocate` (`EOPNOTSUPP`) gets the segment filled with zeros
instead, with one warning. A checkpoint does not remove the segments it trims: after the superblock is written, it
zeroes each one whole, syncs it, renames it a spare and syncs the directory, all on the checkpoint path; a new segment
is a spare renamed, so the steady state allocates nothing and every round's sync is data only. The zeros are what make
a recycled segment safe to scan: only a round's first block carries a header, so a block of an old round's body after
the tail could not be told from a damaged header, and would be reported as a torn tail at every restart. Spares are
kept and never released: a log keeps the disk of the most segments it has held, at most its capacity, which is the
price of allocating nothing in the steady state. The round that moves into a new segment pays a rename and a directory
sync on the commit path, once per segment; task 12.4 measures it, and only a measured spike would move the rename to
the checkpoint.

### Sessions

Registered through a round; held in the keyspace without versions, written in the round's batch, so recovery restores
them with the store and replays the rest. Bounded; the oldest evicted. Each session's entry holds its last committed
request and that request's whole reply, the commit version and every operation's result, so a retry is answered byte
for byte as the original was (decided 2026-10-28).

### Request terms

Decided 2026-10-27: a request names its terms by text, in a term table of its own in the body. Each entry is a kind
(IRI, blank node, literal, language-tagged literal, triple term), a base direction, and two offset-and-length pairs into
the body's text section; a triple term names three earlier entries, nested at most 32 deep. Result batches keep
`ultragraphdb_core_terms`' encoding. `ultragraphdb_protocol_wire` depends only on `ultragraphdb_error_protocol`,
`ultragraphdb_core_terms` and `xxhash-rust`; its Write, Describe and Operation bodies have layouts of their own, and the
server session converts them and the request terms to `Changeset`, declarations and oxrdf types.

### The protocol on compio

Workers are compio runtimes pinned to n−1 cores; a connection is accepted onto the least-loaded worker and stays there;
frames are read header first, then body into one pooled buffer; the body is checked once and then read through views.
Results stream as frames on the request's stream while credit lasts.

Every compio runtime, the workers' and the commit step's, is built with `single_issuer` and `defer_taskrun`
(decided 2026-10-27): each ring is driven by the one thread that owns it, so `IORING_SETUP_SINGLE_ISSUER` (Linux 6.0)
holds, and `IORING_SETUP_DEFER_TASKRUN` (6.1) runs a ring's completion work only when its thread waits for
completions, never interrupting a transaction's execution or the commit step at a system call. The README's baseline,
Linux 6.6, has both, so they are set unconditionally.

### Fuzzing

Mutation testing asks whether the suite notices a wrong line; it cannot find the input nobody thought of, which is where
decoders of untrusted bytes fail. Every decoder of bytes from a client, a peer or the disk is therefore fuzzed:

- **Tools:** `cargo-fuzz`, the Rust Fuzz project's recommended driver, over libFuzzer, with `arbitrary` for
  structure-aware inputs (a valid frame built from arbitrary fields, then mutated, reaches the body checks a random
  byte string rarely passes). `cargo-fuzz` needs a nightly toolchain, which the machine has, and builds libFuzzer from
  C++ through `libfuzzer-sys`. That C++ dependency exists only in the fuzz packages: no shipped crate depends on it, so
  the pure-Rust rule for shipped binaries holds.
- **Layout:** each fuzzed crate has a package at `fuzz/<crate>/`, its targets calling the crate's public decoders. The
  packages form one workspace in `fuzz/`, which the root workspace excludes and Bazel never builds: every dependency is
  pinned once in `fuzz/Cargo.toml` and declared `{ workspace = true }`, as in the root workspace, and the packages share
  one lock file and one build directory. Seeds and regression cases stay in the crate's own `testdata/`.
- **Properties:** never panic; accept or refuse with the typed error; re-encode an accepted value to the same bytes
  where the format has one encoding; the WAL scan never accepts a round whose checksums fail.
- **Seeds:** the conformance suite's golden frames and the posting format's golden values, so a run starts from inputs
  that pass the outer checks.
- **Running:** `scripts/fuzz.sh [crate [target]]`, shaped as `scripts/mutants.sh`: it checks that `cargo-fuzz` is
  installed, runs each target time-boxed (`-max_total_time`), and reports executions, coverage and failing inputs.
  Fuzzing is not hermetic and has no fixed end, so it stays outside Bazel, as mutation testing does.
- **What Bazel keeps:** every failing input is minimized (`cargo fuzz tmin`) and committed under the crate's
  `testdata/fuzz_regressions/<target>/`, the test data Bazel already declares, and a unit test replays each one through
  `ultragraphdb_tool_bdd`'s `fuzz_regressions`, so the hermetic suite carries every finding.
- **In the protocol:** a group that decodes bytes writes its fuzz targets and seeds in phase 5; mutation testing and the
  fuzz runs happen once, at the very end, in the stage's final verification (decided 2026-10-27); stage 5.5 runs longer
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
- [On macOS a sync does not reach the drive: compio's `sync_data` is `fsync` there, which leaves the drive's cache
  unflushed (`F_FULLFSYNC` would flush it)] → a known limitation, documented on `CompioStorage` and
  `CompioFile::sync`; macOS is a development platform and the node ships for Linux, so durability is measured and
  kill-tested on Linux only.
- [Elle runs on the Java virtual machine] → it runs outside Bazel, as the Lean model does, through the Rust client.
- [`cargo-fuzz` needs nightly and builds libFuzzer from C++] → only the fuzz workspace depends on it, outside the root
  workspace and Bazel; findings enter the hermetic suite as replayed regression cases.

## Migration Plan

No outside users: stores written before this stage are refused at open by the posting format's version byte and the
missing WAL superblock. Data enters again through stage 7's loaders or is regenerated.

## Open Questions

None: every decision was taken on 2026-10-27 in the note.
