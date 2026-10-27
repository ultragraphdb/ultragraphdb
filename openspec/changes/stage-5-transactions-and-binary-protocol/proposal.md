## Why

Writes run serially under one lock today (`ChangesetCore::apply_batch`), the store syncs once a minute by default, and
nothing validates a read, because nothing runs concurrently. The protobuf messages of Dgraph are still the posting lists'
storage format, and no server or client exists. Stage 5 makes every acknowledged commit survive power loss, takes write
execution to n−1 cores under strict serializability, and replaces protobuf with a binary format of the project's own
that a receiver reads in place, before stage 6's query core writes through it. The design is
` zero-copy-binary-protocol.md` in this change's folder, approved on 2026-10-27; it refines stage 5 of
`openspec/notes/UltraGraphDB Dual Query Engine Plan.md`.

## What Changes

- **Transactions on n−1 cores:** read-write transactions execute on worker cores against a snapshot at the visibility
  watermark, recording a read set at statement granularity and building a write-set record; read-only transactions are
  never validated and never abort; read-write transactions live at most 10 s, read-only ones 16 s, both settings.
- **One commit step:** rounds closed when the previous round's sync completes; commit versions that follow the clock,
  even; exact validation against a conflict history, as FoundationDB's resolver; commit-time operations
  (insert-if-absent on a unique value, compare-and-set, counter add) decided in commit order; acknowledgement at the
  visibility watermark; catalog changes, drops and index builds ordered in rounds; sessions through the log with
  exactly-once commits.
- **A write-ahead log of the project's own** is the durability point: rounds appended as checksummed, chained frames in
  preallocated segments, one `fdatasync` per round, a ping-pong superblock, recovery that replays rounds above the
  store's applied position and refuses interior corruption; fjall becomes derived state, written without a sync and
  synced at checkpoints. The ordered-log interface (append, committed, read, trim, seal) is the seam stage 10's VSR log
  implements. A configuration flag states whether the drive has power-loss protection, `false` by default; it tunes
  round bounds and checkpoints and never removes a sync.
- **The binary protocol:** one 256-byte little-endian header for client traffic, log records and later replica traffic;
  XXH3-64 checksums with a chunked body checksum under one root; 1 MiB frames; bodies read in place; streams,
  credit-based flow control and in-band cancellation; prepared statements over quad patterns; the describe request;
  columnar, dictionary-encoded result batches; a conformance suite of valid and malformed frames; a Rust client and the
  single-node server binary.
- **The vector index** is maintained by an indexer in commit order, outside the transaction; level draws are a function
  of the node; a vector index declaration states its dimension.
- **Posting lists in the project's own format:** `PostingList`, `Posting`, `UidPack`, `UidBlock` and `List` become the
  repository's types in `ultragraphdb_core_codec`, with a versioned, checksummed byte format.
- **Verification:** a Lean 4 model of the round protocol with its properties proved; a node simulator with storage,
  network and clock faults; Elle through the client and a white-box checker over the log; kill tests on the binary.
- **BREAKING:** `ultragraphdb_proto_dgraph` and `ultragraphdb_proto_pb` are removed with the `ultragraphdb_proto/`
  folder, the prost toolchain in Bazel and the workspace pins of `prost`, `tonic`, `tonic-prost` and
  `tonic-prost-build`. The OTLP exporter keeps gRPC through tonic, as a dependency of `opentelemetry-otlp`.
- **BREAKING:** stores written before this stage are not readable: the posting format and the durability layout change.
  There are no outside users.
- `core_types`' `object_value`, `get_geo_tokens`, `match_geo` and `sort_with_facet`, and `core_tok`'s
  `get_index_factory_from_spec`, Dgraph query-layer functions no crate calls, are removed.

## Capabilities

### New Capabilities

- `transactions`: snapshots, read sets, conflict rules, strict serializability, read-only transactions, interactive
  transactions, limits, commit-time operations and exactly-once commits through sessions.
- `commit-rounds`: the commit step: rounds, commit versions, exact validation, apply, the visibility watermark and
  acknowledgement, and catalog changes, drops and index builds in rounds.
- `write-ahead-log`: the ordered-log interface, the WAL's frames, segments and superblock, durability, recovery and its
  corruption rules, checkpoints, the power-loss-protection flag, and its simulated storage.
- `wire-protocol`: the header, checksums, validation, bodies, streams, credit, cancellation, prepared statements,
  describe, result batches, the conformance suite and the Rust client.
- `server-node`: the single-node server process: workers, dispatch, bounded queues, configuration, startup with
  recovery, and shutdown.
- `transaction-verification`: the Lean model of the round protocol, the node simulator, the history checkers and the
  kill tests.

### Modified Capabilities

- `local-commit`: a transaction no longer reads another's writes from its batch; commits are written per round at
  sequenced versions with the round's log position; reads take the watermark instead of waiting; rollups read at the
  watermark and take no version from the sequencer.
- `changesets`: terms are resolved during execution against the snapshot; a changeset reports its read and write sets
  for validation in place of regions for read summaries.
- `posting-lists`: posting lists are stored in the repository's own versioned, checksummed format.
- `vector-index`: inserts by an indexer in commit order; level draws per node; the dimension declared and checked
  against the declaration; searches checked against the snapshot.

## Impact

- **New group folders and crates:**
  - `ultragraphdb_protocol/`: `ultragraphdb_protocol_wire`, `ultragraphdb_protocol_bus`, `ultragraphdb_protocol_client`;
  - `ultragraphdb_server/`: `ultragraphdb_server_session`, `ultragraphdb_server_node` (the binary);
  - `ultragraphdb_transactions/`: `ultragraphdb_transactions_log`, `ultragraphdb_transactions_commit`,
    `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_sim`;
  - `ultragraphdb_traits_log`, `ultragraphdb_error_log`, `ultragraphdb_error_protocol`.
- **Changed crates:** `ultragraphdb_core_codec` (the posting types and format), `ultragraphdb_core_algo`,
  `ultragraphdb_core_types`, `ultragraphdb_core_tok`, `ultragraphdb_core_mapping` (the vector dimension),
  `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_store_apply`, `ultragraphdb_store_hnsw`,
  `ultragraphdb_tool_bench`, `ultragraphdb_tool_test`.
- **Removed:** `ultragraphdb_proto_dgraph`, `ultragraphdb_proto_pb`, `bazel/toolchains/prost_toolchain`, the
  `protobuf` and `rules_rust_prost` entries of `MODULE.bazel`; each removal at its task, with the user's approval.
- **Dependencies:** `compio` and `xxhash-rust`, both pure Rust. Lean 4 runs outside Bazel for the model, as MGQL's Lean
  does.
- **Documentation:** `AGENTS.md` (crates, dependencies), `ROADMAP.md`, and the plan's write-path sections take the
  approved decisions.
