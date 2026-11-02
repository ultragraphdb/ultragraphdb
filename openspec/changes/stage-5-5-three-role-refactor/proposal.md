## Why

Stage 5 built the transaction path, the write-ahead log, the wire protocol and the Rust client for one server that the
distribution architecture has since replaced. Nothing yet runs: there is no server, every ID is a 64-bit Dgraph UID,
versions follow one commit step's clock, and one commit thread caps a node at about 38,000 relations per second.
Stage 5.5 lays the formats and seams every later stage stores and builds on — the 128-bit identity, epoch-based
versions, the logical log record, the metadata group, the sequencer and partitions — and ships the three roles as
running binaries, so that stages 6 and 7 build on the cluster's shape from their first line. The decisions are
recorded in `openspec/notes/UltraGraphDB Shard-per-Core Write Path.md` (P1–P23, adopted 2026-11-01), the review's D1–D8
(`openspec/notes/review-stages-5.5-and-6.md`), the single-container note and backlog items 1–4, 7, 9, 10 and 11.

## What Changes

- **Identity (D1 as revised, P7, P15, P16):** every entity and statement receives a 128-bit system ID: a 16-bit type
  prefix, a 24-bit bucket (8 bits hashed from the namespace, 16 from the declaring type and the key), a 64-bit
  sequence unique per type, and 24 reserved bits. Until UGS keys exist, an IRI is the key of the built-in `Resource`
  type. Sequences are leased in blocks per (type, partition) from the metadata group. IDs never leave the database:
  result batches name IRIs by their text.
- **BREAKING — formats:** keys, posting lists and packed ID lists carry 128-bit IDs under new version bytes; versions
  become a 42-bit epoch and a 22-bit sequence (P13); a log record carries resolved quads, the round's new dictionary
  entries, its version, the catalog version and the derivation version (P18). Stores written before this stage are not
  readable; there are no outside users.
- **The sequencer (P21)** with one replica closes epochs at its wall-clock time, records each close, and keeps the
  ordered log of multi-partition transactions. Every commit carries the epoch it last closed.
- **The metadata group (D4)** with one replica holds catalog versions, which take effect at epoch boundaries, ID
  sequence blocks, the placement map (bucket ranges per partition, namespace runs, split nodes, preferred leaders) and
  the principals, whose credentials authenticate every connection.
- **Partitions (P1–P4, P6, P11, P17, P19, P20):** a storage process hosts partitions, each its own log, commit step and
  keyspaces over the bucket ranges the placement map gives it; one partition by default for stages 6 and 7, N in
  `wal_bench`, each with its own seeded generator. A transaction stays within one partition; one reaching a second
  partition is refused until stage 8 executes cross-partition commits. Reverse keys carry their source-bucket part;
  absence is proven by a node's predicate lists, never by a seek.
- **Three role binaries and images (P22):** `ultragraphdb_frontend` (the endpoint, authentication, the metadata group,
  the sequencer), `ultragraphdb_compute` (quad-pattern reads) and `ultragraphdb_storage` (partitions), each its own
  binary under `ultragraphdb_bin` and its own OCI image on the Wolfi glibc base (D7); a supervisor binary runs the three
  in the development container, joined over Unix-domain sockets; health, readiness, start and shutdown order, exit
  codes, the seccomp profile and io_uring's fallback.
- **The client against the cluster (backlog item 9):** commits answer a session token (partition, position, epoch);
  reads name a consistency level and wait for a token's epoch; the shells keep automatic credit; a commit in flight
  answers `ConnectionLost` before its frame is sent and `InDoubt` after.
- **Ultra Style:** every crate the change touches adopts the Ultra lint set and conforms to `openspec/protocols/
  ULTRA_STYLE.md` before its behaviour changes; every crate it creates starts conformant. Every task group follows
  `openspec/protocols/tdd-protocol.md`'s five phases, with each capability's scenarios run verbatim as Gherkin.
- **Removed (P23):** the Go golden vectors of the 64-bit packed-UID format (`codec_uidpack`, `codec_seek`) with their
  generator code and tests; the key-encoding scenarios citing Dgraph's bytes are rewritten. Bolt, the web UI and the
  vector indexer in the server path wait for stage 8; the frontend refuses vector-index declarations until then.

## Capabilities

### New Capabilities

- `system-identity`: the 128-bit system ID, type prefixes, bucket derivation from namespace and key, the canonical key
  encoding, sequences leased per (type, partition), and IDs that never leave the database.
- `sequencer`: epochs closed at the sequencer's wall-clock time, the record of closes and `as of time`, epoch
  delivery to partitions, the ordered log of multi-partition transactions, one replica, log-backed.
- `metadata-group`: the log-backed state machine with one replica holding catalog versions, ID sequence blocks, the
  placement map and principals; its commands, snapshots and recovery.
- `partitions`: a storage process hosting partitions over bucket ranges, routing by bucket, one partition per
  transaction, reverse keys by source bucket, absence by index, and the benchmark of N partitions.
- `role-binaries`: the frontend, compute and storage binaries and the supervisor; their images, profiles, transport
  addresses, data layout, start, readiness, health, shutdown order, exit codes, seccomp profile and I/O fallback.
- `client-endpoint`: the frontend's endpoint: connections, authentication against the metadata group's principals,
  sessions, routing requests to partitions and compute, statement caches, the error table and result batches.
- `quad-reads`: quad-pattern reads in a transaction executed by compute over storage: access paths, resume keys, an
  unbound graph, statement IDs, the empty result and the read levels.
- `settings-toml`: the in-house parser of the TOML subset the roles' settings use, refusing every other feature by
  name, with typed accessors that refuse unread keys.
- `ultra-style`: the Ultra lint set, its adoption by every crate the change touches, and the conformance each crate
  reaches before its behaviour changes.

### Modified Capabilities

- `key-encoding`: keys carry 128-bit IDs; reverse keys carry their source-bucket part; scenarios citing Dgraph's
  bytes are rewritten.
- `uid-lists`: the packed list holds 128-bit IDs in a format of the repository's own with a version byte; Dgraph's
  `UidPack` format and its golden vectors are removed.
- `term-dictionary`: IRIs and blank nodes receive system IDs, their bucket from the IRI's hash and their sequence from
  a leased block; creating one IRI twice is decided in its bucket's partition.
- `posting-lists`: the stored format carries 128-bit IDs under a new version byte.
- `quad-storage`: statements receive system IDs of the statement type; keyspaces hold 128-bit IDs.
- `vector-index`: HNSW node IDs are 128-bit system IDs.
- `schema-storage`: the catalog lives in the metadata group, not in each namespace's catalog keys.
- `schema-changes`: a schema change takes effect at an epoch boundary in every partition.
- `transactions`: strict serializability within a partition; a transaction confined to one partition; session tokens.
- `commit-rounds`: commit versions are built from the epoch, not the clock.
- `write-ahead-log`: the record is logical; the log is the only synced stream; appends return durability tickets.
- `wire-protocol`: result batches name IRIs by text; role commands between roles; session tokens and read levels;
  the client's flow-control and in-doubt rules.

## Impact

- **New group folders and crates:** `ultragraphdb_cluster/` (`_placement`, `_metadata`, `_sequencer`, `_partition`),
  `ultragraphdb_roles/` (`_frontend`, `_compute`, `_storage`), the binaries `ultragraphdb_bin/ultragraphdb_frontend`,
  `_compute`, `_storage` and `_dev`, `ultragraphdb_core/ultragraphdb_core_ids`,
  `ultragraphdb_tool/ultragraphdb_tool_toml`, and the error crates `ultragraphdb_error_ids`,
  `ultragraphdb_error_cluster`, `ultragraphdb_error_role` and `ultragraphdb_error_toml`.
- **Changed crates, each lifted into Ultra Style first:** `ultragraphdb_core_keys`, `_core_codec`, `_core_algo`,
  `_core_terms`, `_core_catalog`, `ultragraphdb_store_kv`, `_store_posting`, `_store_quads`, `_store_apply`,
  `_store_hnsw`, `ultragraphdb_traits_log`, `ultragraphdb_transactions_commit`, `_transactions_log`, `_transactions_txn`,
  `ultragraphdb_protocol_wire`, `_protocol_transport`, `_protocol_transport_compio`, `ultragraphdb_client_core`,
  `_client_tokio`, `_client_compio`, `ultragraphdb_tool_bench`, `_tool_test`, `_tool_bdd`.
- **Removed:** `tools/golden/codec.go`'s generators, `ultragraphdb_core_codec/testdata/golden/codec_uidpack.json` and
  `codec_seek.json` with their tests (decided 2026-11-01).
- **Build:** the Ultra lint set in each adopting crate's manifest and `too-many-lines-threshold = 70` in `clippy.toml`;
  role images through `glibc_multiarch_img` (`bazel/container/img.bzl`) on `bazel/oci_images/glibc_base`; a seccomp
  profile allowing io_uring for the role images.
- **Documentation:** `AGENTS.md` (crates, dependencies), `ROADMAP.md`, and the notes the decisions came from.
