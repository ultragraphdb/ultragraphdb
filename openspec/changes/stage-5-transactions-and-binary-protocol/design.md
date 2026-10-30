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

### The server

Drafted 2026-10-30 for review (task 10.0), after the review of group 10 found the server under-specified. Where an
earlier decision settles a point it is cited; every other choice below is a proposal, listed under *Open Questions*
until the user confirms it. The code follows Ultra Style (`openspec/protocols/ULTRA_STYLE.md`), which applies from
this group on.

#### Crates

| Crate | Holds |
|---|---|
| `ultragraphdb_server_session` | The connection's session layer, sans-I/O: the transport's `ServerConnection` per connection, the requests it hands over turned into transaction operations, the conversions of Write, Describe and Operation bodies and request terms (10.2's), result batches built from rows, the authenticator, throttling and the audit events. A step function over a connection's frames, the time and the commit step's replies, so the node simulator (11.3) drives it without a runtime |
| `ultragraphdb` | The node: configuration, the data directory, cluster creation, recovery, the threads and their runtimes, the acceptor, the commit-step thread, the health endpoint, signals and shutdown, telemetry; the `ultragraphdb` binary |
| `ultragraphdb_tests` | The acceptance batteries: the built binary started by `service_utils` and driven through `ultragraphdb_client_tokio`, as scrith's `smdb_tests` drive their services |

The pattern engine (*Reads*) lives in `ultragraphdb_transactions_txn`, beside the transaction whose reads it records.
Each group-10 crate carries the Ultra lint set in its own `[lints]` table: Cargo lets a crate inherit the workspace's
table or state its own, not both.

#### Reads

A Read or an Execute is a quad pattern matched in its transaction: `Transaction::match_into(pattern, resume, limit,
now_us, rows)`, which fills `rows` with each match's terms in slot order (store encodings) and statement IDs and returns
the resume key when `limit` stopped it. A read-write transaction's matches are recorded in its read set (the lists read,
the ranges scanned, the dictionary absences), so a conflicting commit aborts it; it reads its own writes. A read-only
transaction reads its snapshot and records nothing.

Bound terms are resolved through the dictionary first (`QuadStore::uid_of`); a bound IRI or literal the dictionary
does not hold matches nothing, recorded as an absent term. Then one access path per binding of subject (S), predicate
(P) and object (O), in each graph the graph slot names:

| Bound | Path | Rows in the order of |
|---|---|---|
| S P O | the forward list (G, P, S), tested for O | — |
| S P | the forward list (G, P, S) | its postings |
| P O | the reverse list (G, P, O) | its postings |
| P | the forward lists of the range (G, P) | subject, then postings |
| S | the subject's predicates (`0x12`, G, S), then each forward list (G, P, S) | predicate, then postings |
| O | the object's predicates (`0x13`, G, O), then each reverse list (G, P, O) | predicate, then postings |
| S O | as S, each list tested for O | predicate |
| none | the forward keyspace of G | predicate, subject, postings |

A bound graph or the default graph names one graph; an unbound graph slot skip-scans the namespace's graphs, as the
stage-4 drops do (one seek per graph). A vector is never a bound object: the reverse keyspace holds none (stage 4), so
a pattern binding one is refused with code 7.

**Resume keys.** A resume key is the key of the list being read when the limit was reached, followed by the `uid`
field (u64 big-endian) of the last posting returned; the next request starts after it. The server refuses a key outside
the pattern's access path, or one it cannot parse, with Error 1 on the request's stream (`PROTOCOL.md` §11.8, point 47).

**Result batches.** The session layer turns rows into `ResultBatch`es: a UID's text and its skolem flag from
`QuadStore::iri_of` (tag `0x02` for a skolem), a hash's text from `string_of`, a custom datatype's IRI as the
annotation, each looked up once per batch, which holds each distinct term once. A batch closes before a row would take
its frame past the smaller of the client's and the server's `max_frame`, or at 4,096 rows; it streams while the stream
holds credit (`Outbound`). The last batch of a read carries `LAST`, and the resume key when the limit was reached. A
read matching nothing answers one empty batch flagged `LAST` (point 9).

#### Threads and queues

A node runs, inside one `std::thread::scope` so every thread borrows the store, the applier and the configuration
without reference counting:

- **The commit-step thread**, a compio runtime owning the `Pipeline` (its log, its step, its applier's apply).
- **Workers**, one compio runtime per thread, one thread per core but the commit step's (n−1, at least one; on one core
  the worker and the commit step share it, `server-node`), each pinned with `sched_setaffinity` (`rustix`) on Linux and
  unpinned on macOS. A worker owns its connections, their transactions and their statement caches.
- **The acceptor**, a thread with a blocking `std::net` listener: it accepts, sets `TCP_NODELAY` and keepalive (60 s
  idle, 10 s interval, 6 probes), and hands the socket to the worker holding the fewest connections (an `AtomicU32` per
  worker; ties to the lowest index) over that worker's bounded channel.
- **The health endpoint**, a thread of its own (*Health*).

Every compio runtime is built with `single_issuer` and `defer_taskrun` on Linux (decided 2026-10-27); the node checks
at start that the kernel is 6.6 or later and refuses to start below it, naming the kernel. The node's threads are its
own: `std::thread::scope` carries the `#[expect(clippy::disallowed_methods, reason)]` the store's production
implementations carry (`clippy.toml`), and the simulator drives the step functions those threads run, not the threads.

**Worker to commit step.** One bounded channel (`futures-channel`, as decided) carries the pipeline's inputs —
commits, registrations, drops, retries — tagged with a ticket: the worker's index in its top 8 bits, a counter of the
worker's in the rest. A send to a full channel is refused at once with code 5 (Busy). The pipeline's own waiting
queue is bounded by `Pipeline::with_queue`. **Commit step to worker:** one bounded channel per worker carries replies
and the transaction buffers coming back (`Pipeline::buffers_mut`), routed by the ticket's top bits; its capacity is the
most commits a worker may have in flight, so the commit step never waits on it, and a full one is an invariant
violation. A worker maps each ticket to its connection and stream. **Read floor:** each worker publishes the read
version of its oldest open read-write transaction in an `AtomicU64`; the commit step takes their minimum, or the
watermark when none is open, as `Input::OldestRead` once per round.

**Time.** Every thread reads the time through the clock it is given (`clippy.toml`'s `disallowed-methods`). A worker
checks a transaction's limit (`TxnLimits`, the Welcome's `transaction_ms`) at each of its requests and once a second
over all its open transactions, ending an expired one with Error 4 on its stream.

#### Bounds

| Bound | Default | Past it |
|---|---|---|
| Connections per node | 1,024 | the Hello is answered with Error 5 (Busy) on stream 0, and the connection closes |
| Frame buffers per worker | 64 of `max_frame` | a connection's next frame waits for a free buffer |
| Commits in flight per worker | the commit queue divided among the workers | Error 5 |
| Commit queue (channel and pipeline) | 4,096 | Error 5 |
| Rows per result batch | 4,096, within `max_frame` | the next batch |
| Source addresses tracked by throttling | 65,536 | the address least recently seen is forgotten |
| Streams, credit, sessions, statements | `PROTOCOL.md` §11.9 | its codes |

Outgoing frames are built into the worker's pooled buffers, so a worker's steady state allocates nothing (Ultra
rule 3).

#### The error table

| Raised | Answered |
|---|---|
| `TxnError::ReadAboveWatermark` | Error 7 on the stream, `detail` the watermark, the message naming both versions; the worker serves on |
| `TxnError::PastLimit` | Error 4, the transaction ended |
| `TxnError::WriteSetTooLarge`, `ReadSetTooLarge` | Error 6, `detail` the limit |
| A catalog's or a term's refusal | Error 7, `detail` the term's index where one is at fault |
| `CommitError` | its own `code()` |
| A body breaking a rule | Error 1 on the stream; a frame breaking one, Error 1 on stream 0 and the connection closes (§2.6) |
| A store or log I/O failure, a corruption, an `InvariantError` | fail-stop: Error 17 to every request from then on, the event logged at error, the node shuts down and exits 1 |

#### Configuration

The node reads its configuration once, at start, from environment variables, as scrith's `dgraph_migration` does:
every place it runs supplies the environment, a container manifest and the test harness alike. The binary takes no
arguments but `--help` and `--version`. Every value is parsed and range-checked before the data directory is opened;
the first one broken stops the start with exit code 2, naming the variable and its rule.

| Variable | Default | Rule |
|---|---|---|
| `UGDB_DATA_DIR` | required | a directory, created when missing |
| `UGDB_LISTEN` | `127.0.0.1:7400` | an address and port; loopback when `UGDB_AUTH` is `off` |
| `UGDB_HEALTH_LISTEN` | `127.0.0.1:7401` | an address and port, apart from `UGDB_LISTEN`'s |
| `UGDB_AUTH` | `on` | `on` or `off` |
| `UGDB_BOOTSTRAP_SECRET_FILE` | none | required with `UGDB_AUTH=on` on an empty data directory |
| `UGDB_PASSWORD_DENYLIST_FILE` | none | a file of passwords, one per line |
| `UGDB_AUDIT_LOG` | `<UGDB_DATA_DIR>/audit.log` | a file path |
| `UGDB_WORKERS` | cores − 1, at least 1 | 1 to cores |
| `UGDB_CONNECTIONS_MAX` | 1,024 | 1 to 65,535 |
| `UGDB_FRAME_BUFFERS` | 64 | per worker, at least 4 |
| `UGDB_COMMIT_QUEUE` | 4,096 | 1 to 1,048,576 |
| `UGDB_TRANSACTION_MS` | 10,000 | a read-write transaction's life, 1 to 600,000 |
| `UGDB_READ_ONLY_MS` | 16,000 | 1 to 3,600,000 |
| `UGDB_MAX_WRITE_SET` | 1,040,384 | 4,096 to 1,040,384 |
| `UGDB_SESSIONS`, `UGDB_STATEMENTS`, `UGDB_MAX_STREAMS`, `UGDB_INITIAL_CREDIT`, `UGDB_MAX_FRAME` | §11.9's | §11.9's ranges |
| `UGDB_IDLE_LIMIT_S`, `UGDB_AUTH_TIMEOUT_S`, `UGDB_REFUSAL_INTERVAL_S` | 600, 60, 900 | at least 1 |
| `UGDB_POWER_LOSS_PROTECTION` | `false` | `true` or `false` |
| `UGDB_ROUND_BYTES`, `UGDB_ROUND_TRANSACTIONS`, `UGDB_CHECKPOINT_BYTES` | the flag's (`WalConfig`) | within the log's limits |
| `UGDB_SHUTDOWN_GRACE_S` | 30 | 0 to 3,600 |
| `UGDB_LOG_FILTER`, `UGDB_LOG_FORMAT`, `UGDB_OTLP_ENDPOINT` | `info`, `plain`, none | `ultragraphdb_common_telemetry`'s |

#### The data directory and the cluster

`<UGDB_DATA_DIR>/store` holds the store (fjall), `<UGDB_DATA_DIR>/wal` the log, `audit.log` the audit log by default. At
start the node reads the log's superblock (`SuperblockRead`): `Present` gives the cluster; `Empty` on an empty data
directory creates one, the cluster drawn from the operating system's randomness; `Empty` beside a store, or
`Unreadable`, stops the start naming the directory (exit 3).

**The bootstrap administrator.** A cluster created with authentication takes its first principal from the bootstrap
secret file: UTF-8, the password alone, one trailing line feed (or CR LF) dropped. The node refuses a file readable by
group or others on Unix, a password under 15 characters or on the deny list, naming the rule and never the password,
and creates nothing. It derives the SCRAM-SHA-256 verifier at `DEFAULT_ITERATIONS` and proposes the principal to the
commit step as its first input, `Input::CreatePrincipal { name, verifier }`, numbered by its commit version as a session
is: its round writes the principal's entry under `\xffultragraphdb/security/principal/` without versions, the security
catalog's first entry and its format's first version, and its write-set record names it with operation 8, create a
principal (`PROTOCOL.md` §12.2, to be added), so replay restores it. Until stage 7 the node loads every principal entry
at start into the authenticator's `Credentials`; the file is read only when the cluster is created.

#### Start and shutdown

Start: configuration; telemetry, once; the kernel check; the data directory; the log opened and recovered
(`Wal::open`); the store opened (`Durability::Process`: the log is the durable record); `Replay::run`; the pipeline;
the principal entries; the threads; the listener bound; then the health endpoint turns ready and the node logs
`ultragraphdb ready` with both addresses.

Shutdown, on SIGTERM or SIGINT: readiness turns false; the listener closes; requests on open connections are answered
Error 17; open transactions are aborted; the commits already queued are decided, synced and answered for up to
`UGDB_SHUTDOWN_GRACE_S`; the log and the store are synced; the node exits 0. A second signal exits at once, code 1;
recovery covers it. A fail-stop takes the same path with exit code 1.

**Checkpoints.** The commit-step thread checkpoints when the rounds since the last checkpoint pass
`UGDB_CHECKPOINT_BYTES`: it syncs the store (`FjallStore::sync`), then trims the log through the last applied round with
its version (`OrderedLog::trim_through`), between rounds.

| Exit code | Meaning |
|---|---|
| 0 | a clean shutdown |
| 1 | a fail-stop, a failed sync, a second signal |
| 2 | a configuration refused, or a kernel below 6.6 |
| 3 | a data directory or a recovery refused |

#### Health

A thread of its own serves HTTP/1.1 on `UGDB_HEALTH_LISTEN`, after Kubernetes' probe conventions and the paths
`kube-apiserver` serves: `GET /livez` and `GET /readyz`, 200 with `ok` or 503 with the reason as text, every other path
404, every request one response and the connection closed. It reads a request line and headers of at most 1 KiB within
one second, answers one request at a time, and holds no data: it reads the node's state from atomics.

- `/livez` answers 200 while the commit-step thread and every worker have marked their loop within the last 10 seconds,
  through recovery included, so a long recovery is not restarted; a stalled loop answers 503.
- `/readyz` answers 200 from the ready log line until shutdown begins or a fail-stop, 503 before and after: Kubernetes
  sends connections only to a node that accepts them.

Kubernetes probes `/livez` for liveness, `/readyz` for readiness and as the startup probe. The tests wait on `/readyz`.
It is written over `std::net` by hand, a few hundred lines, so the node adds no HTTP or gRPC stack (tonic and hyper
need tokio; the node runs compio).

#### Throttling and the audit

Each failed authentication delays its Error 19 by `min(100 ms × 2^(n−1), 10 s)`, n the consecutive failures of the
principal or of the source address, whichever is more; the connection holds the reply until its deadline, so no worker
waits. After 100 consecutive failures of a principal its authentications are refused from every source for
`UGDB_REFUSAL_INTERVAL_S`; a success resets both counts (`server-node`).

The audit log is JSON lines: one object per authentication with `time` (RFC 3339, UTC, microseconds), `event`,
`outcome`, `name` claimed, `principal` (or null), `mechanism`, `source` and `connection`, never a password, proof or
SCRAM message. Events go to the `ultragraphdb::audit` tracing target, which `ultragraphdb_common_telemetry` writes to
the audit file alone.

#### The vector indexer under the pipeline (group 8's open questions)

- **The pipeline drives the indexer**, in the form builds take: `prepare_index_vectors` making a pass an `Executed`,
  `Prepared::index_vectors`, and `Pipeline::poll_vectors` proposing each predicate with work under
  `Pipeline::SYSTEM_TICKET`, one pass in flight per predicate. A pass reads the predicate's catalog entry and its
  vector-index range, so a drop, a declaration or a build decided meanwhile aborts it; its queue entries are taken when
  it is applied.
- **After a restart** the indexer reconciles each vector predicate at open: its forward lists against its neighbour
  lists, re-queueing the vectors the lists do not hold. It writes nothing per vector; it scans once per restart.
- **The dimension travels on the wire**: change record 8 (declare a predicate) gains a u32 argument, the vector
  dimension, 0 for none, matching the catalog (`PROTOCOL.md` §4.6, to be added before 10.2).
- **A declaration over vectors of another dimension** is refused as `Contradicted`, as every other contradiction a
  predicate's data makes of a declaration is.

#### Tests

- **Unit and BDD**, in each crate: the session layer, the pattern engine, configuration, the start's decisions and the
  health responses as step functions, without a runtime.
- **Acceptance**, `ultragraphdb_tests`: the binary copied into the sandbox (`copy_file`), started by
  `service_utils::ServiceUtil` with its environment (a data directory under `TEST_TMPDIR`, a port per battery),
  `WaitForHttpHealthCheck` on `/readyz`, then a battery of scenarios sequenced in one `#[test]` through
  `ultragraphdb_client_tokio`, as scrith's `smdb_tests` run. Every server scenario and the conformance suite run this
  way (10.5). `service_utils` joins the workspace's dependencies for it, as in scrith.
- **Simulation**: the node simulator (11.3) drives the session layer, the workers' logic and the commit step as step
  functions, the threads replaced by its schedule.

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

Every decision of the note was taken on 2026-10-27. *The server* (task 10.0, drafted 2026-10-30) proposes the rest of
the server's design; each proposal stands until the user confirms or changes it:

1. Reads: the access paths, the resume key's form, 4,096 rows per batch, and a bound vector object refused.
2. Threads: one `std::thread::scope`, a blocking acceptor thread, pinning through `rustix`, tickets carrying the
   worker's index.
3. The bounds' defaults: 1,024 connections, 64 frame buffers per worker, a commit queue of 4,096.
4. The error table, a read above the watermark answered with code 7, and fail-stop on a store or log failure.
5. Configuration from `UGDB_*` environment variables, the ports 7400 and 7401, and the exit codes.
6. The bootstrap principal through the commit step, `Input::CreatePrincipal` and the log's operation 8.
7. The health endpoint: HTTP/1.1 written over `std::net`, `/livez` and `/readyz`.
8. Throttling's delay, `min(100 ms × 2^(n−1), 10 s)`, and the audit log's JSON lines.
9. The vector indexer: the pipeline's proposals, reconciliation at open, the dimension on the wire, the refused
   contradiction.
