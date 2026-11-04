## Why

Stage 5 built the transaction path, the write-ahead log, the wire protocol and the Rust client for one server that the
distribution architecture has since replaced. Nothing yet runs: there is no server, every ID is a 64-bit Dgraph UID,
versions follow one commit step's clock, and one commit thread caps a node at about 38,000 relations per second.
Stage 5.5 lays the formats and seams every later stage stores and builds on — the 128-bit identity, epoch-based
versions, the logical log record, the metadata group, the sequencer and partitions — and ships the three roles as
running binaries, so that stages 6 and 7 build on the cluster's shape from their first line. The decisions are
recorded in `openspec/notes/UltraGraphDB Shard-per-Core Write Path.md` (P1–P23, adopted 2026-11-01, and P24, the
placement decision adopted 2026-11-02), the review's D1–D8
(`openspec/notes/review-stages-5.5-and-6.md`), the single-container note and backlog items 1–4, 7, 9, 10 and 11.

## What Changes

- **Identity (D1 as revised, P7, P15, P16, P24):** every entity and statement receives a 128-bit system ID: from the
  most significant bit, a 16-bit type prefix, a 64-bit sequence unique per type, a 24-bit bucket (8 bits hashed from the
  namespace, 16 from the declaring type and the key), and 24 reserved bits. Until UGS keys exist, an IRI is the key of
  the built-in `Resource` type. Sequences are leased in blocks per (type, partition) from the metadata group. IDs never
  leave the database: result batches name IRIs by their text, a statement by its quad or, when it holds a statement ID,
  by its reifier's skolem IRI, and a commit-time operation's node by its IRI, a resume key carries its last row's terms
  as text, and a blank node's skolem IRI carries its bucket and 104 random bits.
- **BREAKING — formats:** keys, posting lists and packed ID lists carry 128-bit IDs under new version bytes; versions
  become a 42-bit epoch and a 22-bit sequence (P13); a log record carries resolved quads, the round's new dictionary
  entries and the registrations a home accepted, its version, the catalog version and the derivation version (P18).
  Stores written before this stage are not readable; there are no outside users.
- **The sequencer (P21, P24)** with one replica closes epochs at its wall-clock time, or early when a partition's
  sequence would run out, records each close, announces with each close the catalog versions taking effect after it,
  and keeps the ordered log of multi-partition transactions. Every commit carries its partition's open epoch, exactly
  one above the latest close delivered to it: a partition never advances on its own.
- **The metadata group (D4)** with one replica holds catalog versions, each taking effect two epochs after the latest
  close when it is committed, ID sequence blocks, the placement map (bucket ranges per partition, namespace runs,
  preferred leaders) and the principals, whose credentials authenticate every client connection, and the registry of
  sessions.
- **The config service:** one service, `ultragraphdb_config`, holds every role's settings for every deployment target as immutable
  numbered revisions over an ordered log, validates a revision whole before storing it, refuses to start on anything broken, and serves them over authenticated HTTP.
  `ultragraphdb_config_check` runs the same validation for an administrator before anything starts.
  Every role takes its settings from it at start, and loads another revision by restarting at an administrator's request. One
  instance until stage 8's VSR replicates its log.
- **Partitions (P1–P4, P6, P11, P19, P20, P24):** a storage process hosts partitions, each its own log, commit step and
  keyspaces over the bucket ranges the placement map gives it; one partition by default for stages 6 and 7, N in
  `wal_bench`, each with its own seeded generator exercising registration. A partition is the one owner of every fact
  its subjects write: forward entries, its own reverse index, predicate lists, indexes and statistics, in one local
  commit. A partition's first reference to an entity another partition owns registers at the entity's home, which gets
  or creates its ID and adds the referrer's bucket group to the entity's referrer set; a reverse read asks the home for
  that set and fans out to the partitions it names. Every write is stamped at or above the registrations it references.
  A read-write transaction stays within one partition; cross-partition work, a delete by pattern reaching another
  partition's state included, is refused until stage 8 sequences it. Absence is proven by a node's predicate lists,
  never by a seek.
- **Three role binaries and images (P22):** `ultragraphdb_frontend` (the endpoint, authentication, the metadata group,
  the sequencer), `ultragraphdb_compute` (quad-pattern reads) and `ultragraphdb_storage` (partitions), each its own
  binary under `ultragraphdb/` and its own OCI image on the Wolfi glibc base (D7); a supervisor binary runs the three
  in the development container, joined over Unix-domain sockets; roles authenticate each other with the cluster's role
  secret; health, readiness, start and shutdown order, `sysexits` exit codes, the seccomp profile and io_uring's
  fallback.
- **The client against the cluster (backlog item 9):** commits answer a session token (partition, position, epoch);
  a transaction names on Begin the consistency level, session token and deadline its reads share, and they wait for the
  token's epoch; the shells keep automatic credit; a commit in flight
  answers `ConnectionLost` before its frame is sent and `InDoubt` after; sessions keep one counter, checked in each
  partition's own session table; the client endpoint speaks TLS 1.3 when configured, and the client shells verify it.
- **Ultra Style:** every crate the change touches conforms to `openspec/protocols/ULTRA_STYLE.md` — the workspace's
  lint table, run by `make fix`, and the rules no lint checks — before its behaviour changes; every crate it creates
  starts conformant. Every task group follows
  `openspec/protocols/tdd-protocol.md`'s five phases, with each capability's scenarios run verbatim as Gherkin; Bazel
  is the test gate, and `cargo llvm-cov --lcov` measures the coverage of every added file.
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
- `partitions`: a storage process hosting partitions over bucket ranges, routing by bucket, one owner per fact,
  referrer sets, the epoch inequality, one partition per read-write transaction, the delete refusal, absence by index,
  and the benchmark of N partitions.
- `config-service`: one service holding every role's settings per deployment target as immutable revisions over an ordered log, its
  validation, its authenticated API, the roles' bootstrap and loading a revision by restarting.
- `role-binaries`: the frontend, compute and storage binaries and the supervisor; their images, profiles, transport
  addresses, the role secret, data layout, start, readiness, health, shutdown order, exit codes, seccomp profile and
  I/O fallback.
- `client-endpoint`: the frontend's endpoint: connections, TLS 1.3 when configured, authentication against the metadata
  group's principals, sessions, routing requests to partitions and compute, statement caches, the error table and
  result batches.
- `quad-reads`: quad-pattern reads in a transaction executed by compute over storage: routing, reverse reads through
  referrer sets, access paths, resume keys, an unbound graph, statements named by their quad or reifier IRI, the empty
  result and the read levels a transaction names.
- `settings-toml`: the in-house parser of the TOML subset the roles' settings use, refusing every other feature by
  name, with typed accessors that refuse unread keys.
- `ultra-style`: the workspace's lint table, the conformance each touched crate reaches before its behaviour changes,
  Bazel as the test gate and coverage per added crate.

### Modified Capabilities

- `key-encoding`: keys carry 128-bit IDs; a referrer key names an entity's referrer set; scenarios citing Dgraph's
  bytes are rewritten.
- `uid-lists`: the packed list holds 128-bit IDs in a format of the repository's own with a version byte; decoders seek
  to any 128-bit target; set operations run over 128-bit IDs; Dgraph's `UidPack` format and its golden vectors are
  removed.
- `term-dictionary`: IRIs and blank nodes receive system IDs, an IRI's bucket from its hash, a blank node's from its
  opaque skolem IRI, and their sequence from a leased block; an IRI's ID is got or created at its home, so concurrent
  creators share it and none aborts; a reference from another partition registers there; the dictionary is naming
  state, and an aborted write may leave an entry.
- `posting-lists`: the stored format carries 128-bit IDs under a new version byte.
- `quad-storage`: statements receive system IDs of the statement type; keyspaces hold 128-bit IDs; a partition's
  keyspaces hold every entry its subjects' statements derive, a registration mark per object and bucket group, and a
  referrers keyspace holds referrer sets, each with its region; graphs and fresh terms in system-ID terms.
- `vector-index`: HNSW node IDs are 128-bit system IDs.
- `schema-storage`: the catalog lives in the metadata group, not in each namespace's catalog keys; a first write of an
  undeclared predicate records its default entry there, and an undecodable version refuses the load.
- `schema-changes`: a schema change takes effect in every partition at the epoch two above the latest close when it is
  committed, announced with the close between.
- `transactions`: strict serializability within a partition; a read-write transaction confined to one partition,
  aborting cross-partition otherwise, binding at its first subject read or written; reads at the level a transaction
  names; absent terms and the catalog version in the conflict rules; sessions checked per partition; session tokens;
  commit-time operations claim unique values at their bucket's partition and name their node by its IRI.
- `commit-rounds`: commit versions are built from the epoch, not the clock; a full epoch asks the sequencer for a close,
  bounded by `close_request_ms`; catalog changes are metadata-group commands, and partitions hold no catalog keys.
- `changesets`: terms resolve to system IDs got or created at their home before the commit, with no provisional ID and
  no dictionary key in the read set.
- `mapping-contract`: a node created through the property-graph view is named by a skolem IRI, never by an IRI holding
  its ID.
- `index-maintenance`: a partition counts its own subjects in a reverse count index; an exact reverse count is the sum
  over the referrers' partitions.
- `write-ahead-log`: the record is logical, registrations included; the log is the only synced stream; appends return
  durability tickets.
- `wire-protocol`: result batches name IRIs by text, blank nodes by opaque skolem IRIs and statements by their quad or
  reifier IRI; role commands between roles, registration included; session tokens, and the level, token and deadline a
  transaction names on Begin; the replica range 256–511 kept for stage 8; the client's flow-control and in-doubt rules.
- `transport-tls`: a server accepts TLS 1.3 with its configured certificate; one TLS session drives over any carrier.
- `transaction-verification`: a Lean model of placement proves P24 rules 4, 5, 6, 9 and 11: the epoch inequality, the
  completeness of reverse reads through referrer sets, catalog versions held before their epoch, and safe reclamation.

## Impact

- **New Lean library:** `lean/PlacementModel`, beside `RoundModel`, built with `lake` outside Bazel.
- **New group folders and crates:** `ultragraphdb_cluster/` (`_placement`, `_metadata`, `_sequencer`, `_partition`),
  `ultragraphdb_common/` (`_frontend`, `_compute`, `_storage`), the binaries `ultragraphdb/ultragraphdb_frontend`,
  `_compute`, `_storage` and `_supervisor`, `ultragraphdb_core/ultragraphdb_core_ids`,
  `ultragraphdb_tool/ultragraphdb_tool_toml`, and the error crates `ultragraphdb_error_cluster` and
  `ultragraphdb_error_role`, each raised by several crates; `IdError` and `TomlError` live in the crate raising each.
- **Changed crates, each lifted into Ultra Style first (26):** `ultragraphdb_core_keys`, `_core_codec`, `_core_algo`,
  `_core_terms`, `_core_catalog`, `_core_mapping`, `ultragraphdb_store_kv`, `_store_posting`, `_store_quads`,
  `_store_apply`, `_store_hnsw`, `_store_sim`, `ultragraphdb_traits_log`, `_traits_index`,
  `ultragraphdb_transactions_commit`, `_transactions_log`, `_transactions_txn`, `ultragraphdb_protocol_wire`,
  `_protocol_transport`, `_protocol_transport_compio`, `ultragraphdb_transport_tls`, `ultragraphdb_client_core`,
  `_client_tokio`, `_client_compio`, `ultragraphdb_tool_bench`, `_tool_test`.
- **Removed:** `tools/golden/codec.go`'s generators, `ultragraphdb_core_codec/testdata/golden/codec_uidpack.json` and
  `codec_seek.json` with their tests (decided 2026-11-01).
- **Build:** the workspace's lint table unchanged; role images through `glibc_multiarch_img` (`bazel/container/img.bzl`) on `bazel/oci_images/glibc_base`; a seccomp
  profile allowing io_uring for the role images; a coverage script running `cargo llvm-cov -p <crate> --lcov`.
- **Documentation:** `PROTOCOL.md` (role commands and registration, session rules, the level, token and deadline on
  Begin, statements by quad or reifier IRI, skolem IRIs), `SECURITY.md` (what this stage builds
  with TLS), `AGENTS.md` (crates, dependencies), `ROADMAP.md`, and the notes the decisions came from.
