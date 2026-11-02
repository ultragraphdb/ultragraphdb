## Context

The decisions behind this change are recorded with their evidence in `openspec/notes/UltraGraphDB Shard-per-Core Write
Path.md` (P1–P23), `openspec/notes/review-stages-5.5-and-6.md` (D1–D8), the distribution and single-container notes and
backlog items 1–4, 7, 9, 10 and 11. This document states how the code gets there.

Today:
- Every ID is a 64-bit Dgraph UID: the term dictionary leases UIDs (`ultragraphdb_store_quads`' `Leases`), keys carry
  u64 fields (`ultragraphdb_core_keys`' `QuadKey`), packed lists are Dgraph's `UidPack` (`ultragraphdb_core_codec`), and
  HNSW names nodes by u64.
- Commit versions follow the clock, `max(previous + 2, 2 × clock µs)` (`ultragraphdb_transactions_commit`'s
  `Sequencer`), and one commit step serves one log.
- The log records physical key deltas; fjall journals the same writes again (P9, P18).
- The catalog lives in each namespace's catalog keys (`ultragraphdb_core_catalog`, `ultragraphdb_store_quads`).
- Stage 5 built the wire protocol, its transport, SCRAM, sessions, the commit step, the WAL and the Rust client, and no
  server: nothing answers a client.
- The Ultra lint set is the workspace's one lint table, `[workspace.lints]` in the root `Cargo.toml`, with
  `too-many-lines-threshold = 70` in `clippy.toml`; four lints stay commented out until the crates meet them
  (`indexing_slicing`, `arithmetic_side_effects`, `cast_possible_truncation`, `cast_sign_loss`).
- `ultragraphdb_transport_tls` verifies a server for a client over a blocking `std::net` socket; nothing accepts TLS.
- Skolem IRIs name a blank node by its UID, `…/.well-known/genid/{uid}` (`ultragraphdb_core_mapping`'s `Names`,
  `ultragraphdb_store_quads`' `WriteTerms`).
- The zero-run stored key and the dictionary cache are in `ultragraphdb_store_kv`; the Wolfi glibc base and
  `glibc_multiarch_img` exist (`bazel/oci_images/glibc_base`, `bazel/container/img.bzl`).

## Goals / Non-Goals

**Goals:**
- The formats every later stage stores, fixed now: the 128-bit system ID, epoch-based versions, keys and packed lists
  of 128-bit IDs, the logical log record.
- The sequencer and the metadata group as log-backed state machines with one replica, the interfaces stage 8
  replicates.
- Partitions in the storage process, one by default, N in the benchmark.
- Three role binaries and their images running together, serving the stage-5 wire protocol end to end, so stages 6 and
  7 build on the cluster's shape.
- Every touched crate in Ultra Style, every group test-first with its scenarios as Gherkin.

**Non-Goals:**
- Executing cross-partition transactions (P3, P14): refused with `CrossPartition` until stage 8, which executes them in
  the sequencer's order.
- Replication, VSR, preferred-leader enforcement, replica placement (P8), the epoch agent per node (P12), node budgets
  (P4, P11), change streams and their aggregation, split reverse lists in use (P17 builds only the key part), flush
  sharing behind the log interface (P10 builds only the interface), the bulk loader (P3): stage 8.
- Bolt, the web UI and the vector indexer in the server path (decided 2026-11-01): stage 8 or 9.5.
- UGS keys and declared types: stage 6. This stage knows the built-in `Resource` and `Statement` types only.
- SPARQL, GQL and query planning: stage 7. Reads are quad patterns.
- Prefix filters in lsm-tree (P19): only if stage 7 measures seeks binding.
- TLS between roles, `SCRAM-SHA-256-PLUS`, `EXTERNAL` and `OAUTHBEARER` (`SECURITY.md`, *With TLS*): stage 8, with
  certificates per role. This stage encrypts the client endpoint, the traffic that leaves the cluster.

## Decisions

### Crates and layering

| Crate | Folder | Depends on | Holds |
|---|---|---|---|
| `ultragraphdb_error_ids` | `ultragraphdb_error/` | — | `IdError(pub IdErrorEnum)` |
| `ultragraphdb_error_cluster` | `ultragraphdb_error/` | `_error_invariant`, `_error_log` | `ClusterError(pub ClusterErrorEnum)`: placement, metadata group, sequencer, partitions |
| `ultragraphdb_error_role` | `ultragraphdb_error/` | `_error_cluster`, `_error_protocol` | `RoleError(pub RoleErrorEnum)`: configuration, start, readiness, exit codes |
| `ultragraphdb_core_ids` | `ultragraphdb_core/` | `_error_ids`, `xxhash-rust` | `SystemId`, `TypePrefix`, `Bucket`, `Sequence`, `Namespace`, the canonical key encoding, bucket derivation, `Version` (epoch and sequence), `Epoch`, `SessionToken`; no I/O |
| `ultragraphdb_cluster_placement` | `ultragraphdb_cluster/` | `_core_ids`, `_error_cluster` | `PartitionId`, `BucketRange`, `NamespaceRun`, `SplitNode`, `PlacementMap` and its checks |
| `ultragraphdb_cluster_sequencer` | `ultragraphdb_cluster/` | `_core_ids`, `_error_cluster`, `ultragraphdb_traits_log` | The sequencer's sans-I/O state machine over the ordered-log interface: closes, `as of`, subscriptions, the multi-partition log |
| `ultragraphdb_cluster_metadata` | `ultragraphdb_cluster/` | `_cluster_placement`, `_core_ids`, `_error_cluster`, `ultragraphdb_traits_log`, `ultragraphdb_protocol_scram` | The metadata group's sans-I/O state machine: catalog versions, block leases, the placement map, principals, snapshots |
| `ultragraphdb_cluster_partition` | `ultragraphdb_cluster/` | `_cluster_placement`, `_core_ids`, `ultragraphdb_transactions_*`, `ultragraphdb_store_*` | A partition: its log, commit step and store over its bucket ranges; routing checks; the read kernels |
| `ultragraphdb_roles_frontend` | `ultragraphdb_roles/` | `_cluster_metadata`, `_cluster_sequencer`, `ultragraphdb_protocol_*` | The client endpoint, authentication, routing, the metadata group's and sequencer's hosts |
| `ultragraphdb_roles_compute` | `ultragraphdb_roles/` | `_cluster_placement`, `ultragraphdb_protocol_*` | Quad-pattern reads over storage's kernels: fan-out, merge in key order, resume keys, levels |
| `ultragraphdb_roles_storage` | `ultragraphdb_roles/` | `_cluster_partition`, `ultragraphdb_protocol_*` | The partitions' host: role commands, epoch delivery, block leases |
| `ultragraphdb_error_toml` | `ultragraphdb_error/` | — | `TomlError(pub TomlErrorEnum)`: unsupported feature, invalid document, limit, missing, wrong kind, unread key |
| `ultragraphdb_tool_toml` | `ultragraphdb_tool/` | `_error_toml` | The TOML subset's parser, its document and typed accessors; no external dependency |
| `ultragraphdb_frontend`, `_compute`, `_storage`, `_supervisor` | `ultragraphdb/` | their role crate, `ultragraphdb_common_telemetry` | `main`: configuration, telemetry, the runtime, health, signals; `_supervisor` starts and stops the three roles |

A role crate holds its role's logic and its I/O shell; a binary holds only `main`. The frontend's dependency graph
holds no storage engine, which the role-binaries scenario checks. Error crates follow the repository's rule, one per
error type, `FooError(pub FooErrorEnum)`, each converting from `InvariantError`.

### Ultra Style first, then behaviour

The lint table already holds every crate, so a lift is the part no lint checks: each crate the change touches is
reviewed against the rules no lint checks (1, 2, 3, 5, 8, 9) and brought into line in a commit of its own before its
behaviour changes, the review recorded in the group's notes and the suite passing under Bazel before and after with the
same count. Clippy runs through `make fix` and formatting through `make format`; Bazel does not run Clippy. Every crate
the change creates also meets the four held-back lints, checked by `cargo clippy -p <crate> --all-targets` with each
denied on the command line, so the global table stays as it is until the existing crates meet them too.

Bazel is the one test gate (`make test`). Cargo runs three things: `make format`, `make fix`, and coverage, measured by
`cargo llvm-cov -p <crate> --lcov` for every crate the change adds files to, with every added line covered
(`tdd-protocol.md`, phase 4).

### Test-first, with the scenarios as Gherkin

Every group follows `tdd-protocol.md`'s five phases: the surface with `unimplemented!()` bodies (`#[expect(clippy::
unimplemented, reason = "phase 1 surface")]`), the full suite observed failing for that reason with its count recorded,
the audit against planted defects, the implementation judged by the audited suite alone, then mutants and fuzz targets.
Each capability's scenarios are copied verbatim into the crate's `features/<capability>/*.feature` and bound by its
`bdd` target (cucumber, `fail_on_skipped`), as stage 5 did; unit tests cover the corner cases enumerated in advance and
every error variant. Expected values come from independent sources: the xxHash reference for buckets, hand-computed
LEB128 bytes, a plain reference model for the placement map's checks. The runs of `scripts/mutants.sh` and
`scripts/fuzz.sh` happen once, in the final verification group.

### The system ID

`SystemId(u128)`: type prefix in bits 127–112, bucket in 111–88, sequence in 87–24, reserved 23–0, compared and stored
big-endian. `TypePrefix(NonZeroU16)`, `Sequence(NonZeroU64)` and `Bucket(u32)` checked below 2^24 make the invalid IDs
of the spec unrepresentable once decoded. Built-in prefixes: 1 `Resource`, 2 `Statement`; the metadata group allocates
declared prefixes from 3 in declaration order (D2) from stage 6. The default graph keeps its own key encoding (no ID).

A blank node has no key, so its partition draws its bucket, from those it owns in the namespace's run, and 104 random
bits from its injected random source, and writes both into the skolem IRI, `…/.well-known/genid/{bucket}-{random}`. The
bucket rule reads a blank node's bucket back from that IRI, and the IRI shows nothing of the ID but its bucket, 8 hashed
bits of the namespace and 16 the partition drew. `PROTOCOL.md` already accepts an IRI under the skolem base only when
the dictionary holds it as a blank node's, so a forged genid is refused. This is the interim name until D6's handles.

*Alternative considered:* keeping the ID in the skolem IRI. Rejected: it shows each type's sequence, which reveals how
many entities a type holds and in what order they came, and it ties every IRI a client holds to the placement that
stage 8's splits change.

The bucket is two XXH3-64 hashes (`xxhash-rust`, already pinned): the namespace's 8 big-endian bytes for the top 8 bits,
the declaring type's 2-byte prefix plus the canonical key for the low 16. The canonical IRI key is its length as a u32
big-endian and its UTF-8 bytes. Golden values come from the xxHash reference implementation, committed with their
source, so the derivation is pinned independently of the code (`tdd-protocol.md`, independent sources).

### Keys and lists

A key keeps its keyspace byte and the namespace as 8 big-endian bytes, then 16-byte IDs where stage 4 had 8-byte UIDs;
a reverse key appends its source-bucket part as two big-endian u16s, part then count, after its object; split keys
append a 16-byte start ID. The stored key's zero-run encoding absorbs the IDs' zero bytes, and the version stays its
fixed 8-byte suffix, so a prefix filter needs only "all but the last 8 bytes" (P19). The posting format takes version
byte 2; the packed ID list takes its own version byte 1, 104-bit LEB128 deltas in blocks of 256.

*Alternative considered:* dropping the namespace from keys, since the bucket carries 8 bits of it. Rejected: drops and
bans by namespace are prefix operations today, and 8 hashed bits identify no namespace.

### Versions and epochs

`Version(u64)` = epoch << 22 | sequence, sequences even. The commit step's `Sequencer` gives way to an epoch stamp: the
partition's open epoch, the larger of one above the latest close delivered to it and its own epoch, which it advances
by one when the sequence runs out (P13). A commit is therefore never stamped with a closed epoch, and a snapshot read at
an epoch cannot change after it answers. A partition has closed an epoch once it holds that epoch's close and has
applied every round stamped at or below it; a snapshot read waits for exactly that in every partition it asks. A
session token is (partition, log position, epoch). The rollup's rule is unchanged: it writes below the deltas it folds.

*Alternative considered:* answering `Busy` when an epoch's sequence runs out, in place of the local advance. Rejected:
`epoch_ms_max` is a setting, and at one hour an epoch fills at about 580 commits per second.

### Sessions across partitions

A session's identity is 128 random bits the frontend draws; the metadata group records it with its principal, one
command per session, and the frontend checks every commit's principal against that registry before routing. The client
numbers its commits from one counter per session, because it cannot know which partition a commit reaches. Each
partition keeps its own bounded session table, recording a session at its first commit there with the last request
number and stored reply; it refuses a number below its last and accepts the gaps the session's commits elsewhere leave.
A retry the frontend cannot route — after a reconnect, the open transaction gone — is asked of every partition, which is
rare and bounded by the partition count.

*Alternatives considered:* a strict +1 counter per (session, partition), which the client cannot compute and which only
the frontend's memory could keep; and one session table in the metadata group, which would put every commit through
one state machine, the serial point shard-per-core removes.

### The sequencer and the metadata group

Both are sans-I/O state machines whose commands are records in an ordered log (`ultragraphdb_traits_log`), stored by
the WAL of `ultragraphdb_transactions_log` in their own directories (`/data/seq`, `/data/meta`). A command is applied
when its ticket completes, so an acknowledged command is durable. The metadata group snapshots its state (catalog
versions, high-water marks, the placement map, principals) every `snapshot_rounds` rounds and trims below the
snapshot. Both run inside the frontend process with one replica; stage 8 puts VSR behind the same interface.

Epoch lengths are settings, `epoch_ms_min` and `epoch_ms_max`, required, with the `dev` profile's 2 and 10; the
sequencer closes at `epoch_ms_max` or sooner when a read waits on the open epoch, never before `epoch_ms_min`.

### Partitions

A partition is the stage-5 transaction path instantiated per partition: its WAL directory, its commit step, its own
fjall instance (one per partition in this stage; one per core holding several replicas' keyspaces in stage 8, P11), its
dictionary cache, its sequence blocks. The storage process runs each partition on its own compio runtime, pinned when
configured. Routing checks the subject's bucket against the partition's ranges before a request reaches the commit
step; a miss answers with the owner from the partition's copy of the placement map.

A transaction binds to the partition of its first key; the first key outside it aborts the transaction with
`CrossPartition` (a new abort reason in the transactions error and the wire's error table).

### Roles and their protocol

Roles talk the binary protocol through the existing transport, with role commands in the new range 512–767 (the
replica range 256–511 stays reserved for stage 8's VSR). `PROTOCOL.md` is extended first, before any code, as the
repository's rule asks (strengthen the protocol before implementing). Role connections authenticate with SCRAM-SHA-256
against the cluster's role secret, read by every role from the file `[cluster] role_secret_file` names: each role
verifies its peer locally, since the principals live in the frontend, which is itself a peer, and the secret is refused
on the client endpoint. Rotating it takes a rolling restart. The transport's compio shell gains the Unix-domain-socket
carrier, chosen by the address scheme; in the development container each socket belongs to user 65532 with mode 0600.

The client endpoint speaks TLS 1.3 from the first byte when `[endpoint]` names `tls_cert_file` and `tls_key_file`, and
clear text only when it sets `plaintext = true`, which the `dev` profile does; naming neither stops the start, so clear
text is always a visible choice. `ultragraphdb_transport_tls` gains `TlsAcceptor` and a session with no I/O of its own,
rustls's connection driven by bytes in and bytes out, so the blocking socket, the compio transport and the tokio client
shell drive the same code.

The frontend endpoint takes stage 5's server source material from backlog items 2 and 3: connections bound per core,
the handshake, SCRAM against the metadata group's principals, sessions, routing by bucket, statement caches, the
error table. Compute asks storage's read kernel — a scan of one keyspace's range at a version, bounded by a row limit —
and merges partitions' rows in key order; resume keys are XXH3-checksummed tokens holding the pattern's hash and the
last key, so a client never sees an ID.

### The binaries, the supervisor and the images

Each binary reads its settings from a standard TOML file, one table per concern (`[transport]`, `[storage]`,
`[sequencer]`, ...), parsed by `ultragraphdb_tool_toml` (decided by the user, 2026-11-02): an in-house parser of
exactly the TOML subset the settings use (bare-named tables, bare keys, basic strings, decimal integers, booleans,
comments), refusing every other feature by name. A document it accepts parses as any TOML 1.0 parser would read it, so
the files stay standard; a new setting needing new syntax extends the parser, not the dependency list. A role reads its
settings through typed accessors and then refuses any key it did not take, so a misspelled setting stops the start; an
environment variable `ULTRAGRAPHDB_<TABLE>_<KEY>` overrides a file value, and every setting's bound is checked after
both are read. Storage and compute register with the frontend at its address, so no setting is a list and the subset
needs no arrays. Its independent test source is the toml-test suite's valid and invalid documents, vendored as data
with their source, not a crate. The
profiles are TOML files shipped in the images. `/health` and `/ready` are served by a minimal HTTP/1.1 responder (GET
only, bounded request size) on the observability port, written in-house: two fixed answers do not earn an HTTP
library. The supervisor spawns the role binaries with
`std::process::Command`, waits on readiness over each role's `/ready`, forwards signals, and exits with the first
failing role's code. Exit codes follow `sysexits`: 0 clean, 69 a lost dependency, 70 an invariant violation, 74 a failed
store or log, 78 configuration; a role a signal killed is reported as 128 plus the signal, as a shell does. Table and
setting names use `a`–`z`, `0`–`9` and `_` alone, so each maps to one `ULTRAGRAPHDB_<TABLE>_<KEY>` variable and back.

Each role image is `glibc_multiarch_img` of its binary; the development image layers the four binaries on the same base
through a variant of the macro taking several binaries. The seccomp profile, Docker's default plus the three io_uring
system calls, ships beside the images in `bazel/oci_images/seccomp/`.

### Decisions taken in drafting

- **Principals in the metadata group** (decided by the user, 2026-11-01): backlog item 3's open question.
- **The client's credit is automatic** (backlog item 9): the shells expose no `grant`; the core's grant takes
  `NonZeroU32`. No caller needs manual flow control, and a zero grant is refused on the wire.
- **A commit in flight is in doubt once sent** (backlog item 9): `ConnectionLost` before its frame is sent, `InDoubt`
  after, on any attempt; `RetriesExhausted` folds into `InDoubt`, so a caller handles one in-doubt state through its
  session.
- **The welcome's write-set bound** comes from the log's round size (backlog item 1), so a client never sends a
  write set the log cannot hold.

## Risks / Trade-offs

- [Scope: identity, versions, the log record, two state machines, partitions, three roles and the client in one change]
  → Groups ordered so each lands green on its own: formats first, state machines second, partitions third, roles last;
  the gate (`make test`) runs after every group.
- [Lifting 26 existing crates into Ultra Style costs time and touches code unrelated to behaviour] → Each lift is
  its own commit with an unchanged suite; a lint that misreads code is answered by rewriting the code, an irreducible
  exception by `#[expect]` with its reason and the user's consent, listed in the group's notes.
- [128-bit IDs grow keys and lists] → The zero-run stored key absorbs their zero bytes; LEB128 deltas over 104 bits keep
  lists compact; `wal_bench` measures bytes per relation against stage 5's figures in the final group.
- [Epoch versions depend on a reachable sequencer] → A partition keeps stamping its open epoch while the sequencer is
  away and advances locally when its sequence runs out; reads at a snapshot wait, within their deadline.
- [The in-house TOML subset and HTTP responder are code to maintain] → Both are bounded, single-pass, fuzzed decoders
  of untrusted bytes with targets in phase 5; the subset is pinned against the toml-test suite, and anything it does
  not support is refused by name, never misread.
- [A misbehaving supervisor hides role failures] → It restarts nothing and exits with the failing role's code, so the
  orchestrator sees what a cluster would.

## Migration Plan

There are no outside users: stores written before this stage are not readable, and no migration is offered. Work lands
group by group, each green under `make test` before the next starts; the user commits after each group. Removing the Go
golden vectors and their generator happens in the uid-lists group, approved on 2026-11-01. Rolling back a group is
reverting its commits.

## Open Questions

None blocks this change. Measurements still owed, recorded in the Shard-per-Core note, finalise stage 8's runtime
choices: a follower's cost against a leader's, fjall's compaction amplification, the flush rate on a drive without
power-loss protection, and a run on 64-core bare metal.
