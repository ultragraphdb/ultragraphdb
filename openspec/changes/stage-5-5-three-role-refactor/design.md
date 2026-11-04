## Context

The decisions behind this change are recorded with their evidence in `openspec/notes/UltraGraphDB Shard-per-Core Write
Path.md` (P1–P23, and P24, the placement decision of 2026-11-02), `openspec/notes/review-stages-5.5-and-6.md` (D1–D8),
the distribution and single-container notes and backlog items 1–4, 7, 9, 10 and 11. This document states how the code
gets there.

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
  (`indexing_slicing`, `arithmetic_side_effects`, `cast_possible_truncation`, `cast_sign_loss`); `unimplemented` warns,
  so a phase-1 surface compiles.
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
- Partitions in the storage process, one by default, N in the benchmark, each the one owner of its subjects' facts,
  with referrer sets, registration and fan-out reverse reads between them (P24).
- Three role binaries and their images running together, serving the stage-5 wire protocol end to end, so stages 6 and
  7 build on the cluster's shape.
- Every touched crate in Ultra Style, every group test-first with its scenarios as Gherkin.

**Non-Goals:**
- Executing cross-partition work (P3, P14, P24 rule 7): refused with `CrossPartition` until stage 8, which executes it
  in the sequencer's order, the shares a delete sends to an entity's referrers included.
- Reclaiming referrer-set members and dictionary entries (P24 rule 9): specified here and proved by the placement model,
  built in stage 8; until then a referrer set only grows.
- Replication, VSR, preferred-leader enforcement, replica placement (P8), follower reads, the epoch agent per node
  (P12), node budgets (P4, P11), flush sharing behind the log interface (P10 builds only the interface), the bulk loader
  (P3), backup and restore (P24 rule 10): stage 8, the replication protocol in `openspec/notes/UltraGraphDB Distribution Protocol.md`.
- Moving bucket ranges at run time: stage 8. Here the frontend forms the placement, processes claim its partitions, and it
  stays fixed.
- Bolt, the web UI and the vector indexer in the server path (decided 2026-11-01): stage 8 or 9.5.
- UGS keys and declared types: stage 6. This stage knows the built-in `Resource` and `Statement` types only.
- SPARQL, GQL and query planning: stage 7. Reads are quad patterns.
- Prefix filters in lsm-tree (P19): only if stage 7 measures seeks binding.
- TLS between roles, `SCRAM-SHA-256-PLUS`, `EXTERNAL` and `OAUTHBEARER` (`SECURITY.md`, *Where it is built*,
  Stage 8): stage 8, with certificates per role. This stage encrypts the client endpoint, the traffic that leaves the
  cluster.

## Decisions

### Crates and layering

| Crate | Folder | Depends on | Holds |
|---|---|---|---|
| `ultragraphdb_error_cluster` | `ultragraphdb_error/` | `_error_invariant`, `_error_log` | `ClusterError(pub ClusterErrorEnum)`: placement, metadata group, sequencer, partitions |
| `ultragraphdb_error_role` | `ultragraphdb_error/` | `_error_cluster`, `_error_protocol` | `RoleError(pub RoleErrorEnum)`: configuration, start, readiness, exit codes |
| `ultragraphdb_core_ids` | `ultragraphdb_core/` | `xxhash-rust` | `SystemId`, `TypePrefix`, `Bucket`, `Sequence`, `Namespace`, the canonical key encoding, bucket derivation, `Version` (epoch and sequence), `Epoch`, `SessionToken`, `IdError`; no I/O |
| `ultragraphdb_cluster_placement` | `ultragraphdb_cluster/` | `_core_ids`, `_error_cluster` | `PartitionId`, `BucketRange`, `NamespaceRun`, `BucketGroup`, `PlacementMap` and its checks |
| `ultragraphdb_cluster_sequencer` | `ultragraphdb_cluster/` | `_core_ids`, `_error_cluster`, `ultragraphdb_traits_log` | The sequencer's sans-I/O state machine over the ordered-log interface: closes, close requests, catalog announcements, `as of`, subscriptions, the multi-partition log |
| `ultragraphdb_cluster_metadata` | `ultragraphdb_cluster/` | `_cluster_placement`, `_core_ids`, `_error_cluster`, `ultragraphdb_traits_log`, `ultragraphdb_protocol_scram` | The metadata group's sans-I/O state machine: catalog versions, block leases, the placement map, principals, snapshots |
| `ultragraphdb_cluster_partition` | `ultragraphdb_cluster/` | `_cluster_placement`, `_core_ids`, `ultragraphdb_transactions_*`, `ultragraphdb_store_*` | A partition: its log, commit step and store over its bucket ranges; routing checks; registration as a home and as a referrer; referrer sets; the read kernels |
| `ultragraphdb_common_role` | `ultragraphdb_common/` | `_common_service`, `_error_role`, `ultragraphdb_protocol_*` | What every role does on its role connections: the cluster's settings and the role secret, its verifier, dialling and authenticating a peer, serving a connection, joining the frontend |
| `ultragraphdb_common_frontend` | `ultragraphdb_common/` | `_cluster_metadata`, `_cluster_sequencer`, `ultragraphdb_protocol_*` | The client endpoint, authentication, routing, the metadata group's and sequencer's hosts |
| `ultragraphdb_common_compute` | `ultragraphdb_common/` | `_cluster_placement`, `_core_keys`, `_core_terms`, `ultragraphdb_protocol_*`; no storage engine | Quad-pattern reads over storage's kernels: routing by bucket, reverse reads through referrer sets, fan-out, merge in key order, resume keys, levels |
| `ultragraphdb_common_storage` | `ultragraphdb_common/` | `_cluster_partition`, `ultragraphdb_protocol_*` | The partitions' host: role commands, epoch delivery, block leases |
| `ultragraphdb_common_service` | `ultragraphdb_common/` | `_common_service_config`, `_common_telemetry`, `_error_role`, `compio`, `signal-hook` | What every role binary does around its role: reading its settings, its runtimes and their CPUs, `/health` and `/ready` from the in-house responder, stopping on a signal |
| `ultragraphdb_common_service_config` | `ultragraphdb_common/` | `ultragraphdb_tool_toml` | `ServiceConfig`: one type per TOML table and one field per setting, shared by every binary; the optional file, the environment's overrides, `ServiceConfigError` |
| `ultragraphdb_tool_toml` | `ultragraphdb_tool/` | — | The TOML subset's parser, its document and typed accessors, `TomlError` (unsupported feature, invalid document, limit, missing, wrong kind, unread key); no external dependency |
| `ultragraphdb_frontend`, `_compute`, `_storage`, `_supervisor` | `ultragraphdb/` | their role crate, `ultragraphdb_common_service`, `mimalloc` | `main`: configuration, telemetry, the runtime, health, signals; `_supervisor` starts and stops the three roles |

A role crate holds its role's logic and its I/O shell; a binary holds only `main`. The frontend's dependency graph
holds no storage engine, which the role-binaries scenario checks. An error takes a crate of its own under `ultragraphdb_error/` only when more than one crate raises it
(`AGENTS.md`): `ClusterError` and `RoleError` do, while `IdError` and `TomlError` stay in the one crate raising each.
Every error is `FooError(pub FooErrorEnum)`, and the error crates convert from `InvariantError`.

### Ultra Style first, then behaviour

The lint table already holds every crate, so a lift is the part no lint checks: each crate the change touches is
reviewed against the rules no lint checks (1, 2, 3, 5, 8, 9) and brought into line in a commit of its own before its
behaviour changes, the review recorded in the group's notes and the suite passing under Bazel before and after with the
same count. Clippy runs through `make fix` and formatting through `make format`; Bazel does not run Clippy. The lints
the table holds commented out stay out of this change.

Bazel is the one test gate (`make test`). Cargo runs three things: `make format`, `make fix`, and coverage, measured by
`cargo llvm-cov -p <crate> --lcov` for every crate the change adds files to, with every added line covered
(`tdd-protocol.md`, phase 4).

### Test-first, with the scenarios as Gherkin

Every group follows `tdd-protocol.md`'s five phases: the surface with bare `unimplemented!()` bodies, whose warnings
(`clippy::unimplemented` warns by design) are ignored until phase 4, where any left name an overlooked body; the full suite observed failing for that reason with its count recorded,
the review of the suite's input variety, the implementation judged by the suite alone, then `scripts/mutants.sh` over the
group's own files and its fuzz targets.
Each capability's scenarios are copied verbatim into the crate's `features/<capability>/*.feature` and bound by its
`bdd` target (cucumber, `fail_on_skipped`), as stage 5 did; unit tests cover the corner cases enumerated in advance and
every error variant. Expected values come from independent sources: the xxHash reference for buckets, hand-computed
LEB128 bytes, a plain reference model for the placement map's checks. The runs of `scripts/mutants.sh` and
`scripts/fuzz.sh` happen once, in the final verification group.

### The system ID

`SystemId(u128)`: type prefix in bits 127–112, sequence in 111–48, bucket in 47–24, reserved 23–0, compared and stored
big-endian (field order type, sequence, bucket, approved 2026-11-02): IDs of one type sort by sequence, and the bucket's
hashed bits, below it, keep a sorted list's deltas small. `TypePrefix(NonZeroU16)`, `Sequence(NonZeroU64)` and
`Bucket(u32)` checked below 2^24 make the invalid IDs of the spec unrepresentable once decoded. Built-in prefixes: 1
`Resource`, 2 `Statement`; the metadata group allocates declared prefixes from 3 in declaration order (D2) from stage 6.
The default graph keeps its own key encoding (no ID).

A blank node has no key, so its partition draws its bucket, from those it owns in the namespace's run, and 104 random
bits from its injected random source, and writes both into the skolem IRI, `…/.well-known/genid/{bucket}-{random}`. The
bucket rule reads a blank node's bucket back from that IRI, and the IRI shows nothing of the ID but its bucket, 8 hashed
bits of the namespace and 16 the partition drew. `PROTOCOL.md` already accepts an IRI under the skolem base only when
the dictionary holds it as a blank node's; this change extends the rule to an IRI the dictionary holds as a statement
reifier's name, so a forged genid is still refused. This is the interim name until D6's handles.

*Alternative considered:* keeping the ID in the skolem IRI. Rejected: it shows each type's sequence, which reveals how
many entities a type holds and in what order they came, and it ties every IRI a client holds to the placement that
stage 8's splits change.

The bucket is two XXH3-64 hashes (`xxhash-rust`, already pinned): the namespace's 8 big-endian bytes for the top 8 bits,
the declaring type's 2-byte prefix plus the canonical key for the low 16. The canonical IRI key is its length as a u32
big-endian and its UTF-8 bytes. Golden values come from the xxHash reference implementation, committed with their
source, so the derivation is pinned independently of the code (`tdd-protocol.md`, independent sources).

### Keys and lists

A key keeps its keyspace byte and the namespace as 8 big-endian bytes, then 16-byte IDs where stage 4 had 8-byte UIDs; a
reverse key ends with its object, as in stage 4; split keys append a 16-byte start ID; the referrers keyspace, new, keys
an entity's referrer set by its ID. The stored key's zero-run encoding absorbs the IDs' zero bytes, and the version
stays its fixed 8-byte suffix, so a prefix filter needs only "all but the last 8 bytes" (P19). The posting format takes
version byte 2; the packed ID list takes its own version byte 1, 104-bit LEB128 deltas in blocks of 256.

*Alternative considered:* dropping the namespace from keys, since the bucket carries 8 bits of it. Rejected: drops and
bans by namespace are prefix operations today, and 8 hashed bits identify no namespace.

### One owner per fact

Placing data by vertex (P1's subject bucket, adopted to escape the hot-predicate ceiling of predicate placement) gives
every edge, the unit of a graph write, two owners: the subject's partition for its forward entry and the object's for
its reverse entry. Keeping a second owner in step took a change stream of derived entries, receiver logs, seals, two
watermarks, receiver-assigned versions, rollup caps, a stall policy and sequenced blind writes, and still left nine
questions open. P24 inverts it: every fact stays with its writer, and only a small, monotone where-to-look record moves
toward the reader. The rules are P24's; this section states how the code holds them.

- **The owner.** A partition commits, in one local commit, every entry its own subjects' writes derive: forward and
  subject-predicates lists, its local reverse index from an object to the partition's own subjects, object-predicates
  entries, counts, value indexes and statistics, as `ultragraphdb_store_apply` writes them today. An edge's forward and
  reverse entries are always consistent, and nothing is derived in one partition and applied in another.
- **Registration.** Registration is per entity and referring bucket group. While a transaction executes, its partition's
  first reference to an entity `o` another partition owns from a subject of bucket group `g` (the subject bucket's top
  18 bits: 64 buckets, 2^10 groups per namespace run) becomes a registration at `o`'s home: the IRI's text for a new IRI
  or the ID of a known one, `g`, and the generation the referrer last saw for `(o, g)`. Registrations to one home node
  are batched within each write request. The home gets or creates the ID, adds or renews `g` in `R(o)`, writes both in a
  round of its log and replies once the round's ticket completes, with the ID, the member's generation and its last
  delivered close `c`. The referrer commits only after close `c` has been delivered to it; its commit writes the mark of
  `(o, g)` with the generation, in the marks keyspace keyed by `o` and `g`, and, at its first reference to `o` from any
  group, `o`'s dictionary entries in both directions into its own dictionary, so the partition names `o` in results and
  finds its ID after a restart without asking the home. The log record carries both (write-ahead-log). A predicate or
  graph IRI another partition owns is got or created there the same way before first use, by a registration that joins
  no referrer set. Partitions of one storage process exchange registrations as messages, and storage processes as a role
  command. A transaction's own new entities are got or created in its own partition the same way, durable no later than
  the round that commits it, so concurrent creators of one IRI share its ID and none aborts for it; a statement they
  both insert still conflicts as the transactions spec states.
- **Referrer sets.** `R(o)` lives at the home in the referrers keyspace, versioned like any list: per member, the
  bucket group, the epoch it was added at and its generation. A registration adds members for other partitions' groups;
  the home adds its own groups in the commit that first references `o` from them, so a move of a group rewrites
  nothing. A registration naming the member's current generation, or any generation for a group not yet a member, adds
  or renews it at one generation above the larger of the one named and the highest the group ever held in `R(o)`;
  one naming an older generation is a repeat and is answered with
  the current generation. In this stage `R(o)` only grows.
- **Reads.** Compute routes a bound subject to its bucket's partition. A bound IRI or blank-node object reads `R(o)` at
  the read's epoch `e` at the home once the home has closed `e`, resolves the groups to partitions through the placement
  map at `e`, and asks those partitions, batched per node, for their reverse entries at `e`, each once it has closed
  `e`. A bound literal object, which has no home or referrer set, value-index lookups, lookups by reverse count (which
  sum the partitions' counts) and other patterns ask every partition of the namespace. A read-write transaction's reads
  stay in its partition. A snapshot read answers complete or waits; at its deadline it fails naming the partition it
  waited on.
- **Deletes.** This stage's changesets delete by pattern; a delete by pattern reaching a subject another partition
  owns, or inbound entries another partition's subjects wrote, aborts with `CrossPartition`. The entity delete of P24
  rule 8 arrives with UGS in stage 6: a tombstone at the home, `EntityDeleted` for a later reference, "deleted" for a
  later registration and a new ID for a re-created IRI; stage 8 sends each member of `R(o)` a sequenced share fencing
  the entity there.
- **Reclamation and restore** (P24 rules 9 and 10) are stage 8's: a member leaves `R(o)` only on a report naming its
  unchanged generation, a cluster restores at one closed epoch, a restored partition abandons its open sequence blocks,
  and namespace numbers are never reused.
  The placement model found three runs in which P24's reclamation as first written loses an inbound edge, so stage 8
  builds it with the repairs proved in `lean/PlacementModel` (decided 2026-11-03, traces in `notes/group-6b.md`): a home
  applies a report only once its own open epoch has reached the report's epoch; only a partition holding every bucket
  of a group at every retained epoch reports it, so a group split across partitions or moved is never reclaimed; and a
  registration for a group not in `R(o)` takes one generation above the larger of the generation it names and the
  highest the group ever held there, so the home keeps a removed member's highest generation for as long as duplicate
  messages can arrive. A commit made under a registration reply reads its `(o, g)` mark key, so a report or renewal
  concurrent with it conflicts; group 7 builds that check now, and stage 8's sequenced path keeps it.
- **The placement model.** `lean/PlacementModel`, a Lake library beside `RoundModel`, models registration, stamping,
  catalog announcements, referrer sets, reverse reads, tombstones and reclamation, and proves P24 rules 4, 5, 6, 9 and
  11 (transaction-verification) before group 7 writes the partition code: every write is stamped at or above the
  registrations it references, a reverse read at a closed epoch returns every edge committed to the entity at or below
  it, no partition stamps a catalog version's epoch before it holds the version, and reclamation never removes a
  member whose group still holds an edge.

*Alternatives considered* (P24 records the reasons): keeping the derived-stream design; partition-local handles with a
derived entity key; IDs derived from a hash of type and key; and global value indexes, which recreate the hot-predicate
ceiling. The costs accepted: an inbound point read touches `min(d, partitions)` partitions for an in-degree `d` and
needs one extra round, about 20× the store probes of one reverse list at in-degree 20; a partition's first reference to
an entity from a bucket group waits for one durable round at its home; and an exact reverse count is a sum over the
referrers' partitions, so a lookup by reverse count asks every partition of the namespace.

### Versions and epochs

`Version(u64)` = epoch << 22 | sequence, sequences even. The commit step's `Sequencer` gives way to an epoch stamp: a
partition stamps every commit with exactly one above the latest close delivered to it, its open epoch, and never
advances on its own (P24 rule 4). When its sequence would run out, it asks the sequencer to close the epoch now, which
the sequencer does once `epoch_ms_min` has passed, and holds its commits until that close arrives; once a close request
has gone unanswered for `close_request_ms`, a required setting from 1 to 60,000, the sequencer counts as unreachable and
the partition refuses its held and new commits with `Busy` until the close arrives. A partition therefore never stamps
an epoch whose close has been delivered to it, and it holds the catalog versions announced with its last delivered close
when it stamps the epoch they take effect at; a snapshot read at an epoch cannot change after it answers. A partition
has closed an epoch once it holds that epoch's close and has applied every round stamped at or below it; a snapshot read
waits for exactly that in every partition it asks. A session token is (partition, log position, epoch). The rollup's
rule follows commit-rounds: it writes one above the newest delta it folds, an odd version within that delta's epoch,
which no commit takes. A round closes when a close is delivered to the partition, so every round's commits carry one
epoch and one catalog version, and a registration a home accepts is admitted into its commit step with a commit version
of its own, so replay derives the referrer set's deltas.

The epoch inequality holds on every write path: a registration is stamped at its home's open epoch, at most `c + 1`
for the close `c` its reply carries, and the registrant commits only once close `c` has been delivered to it, so every
write referencing the registration is stamped at or above it; a sequenced submission in stage 8 carries the largest
reply epoch `c + 1` among its registrations, and the sequencer places it at or above that epoch. A transaction
carrying a session token of epoch `t` is stamped at `t` or later, its partition waiting for close `t − 1`, so a
causally later write never lands in an earlier snapshot.

*Alternatives considered:* advancing the epoch locally when the sequence runs out, the rule P13 adopted. Rejected on
2026-11-02: a partition could then stamp a catalog version's effective epoch before the close announcing the version
reached it. Answering `Busy` at once when the sequence runs out: at an `epoch_ms_max` of one hour an epoch fills at
about 580 commits per second, while a requested close keeps commits flowing after a wait of at most `epoch_ms_min`.

### Sessions across partitions

A session's identity is 128 random bits the frontend draws; the metadata group records it with its principal, one
command per session, and the frontend checks every commit's principal against that registry before routing. The client
numbers its commits from one counter per session, because it cannot know which partition a commit reaches. Each
partition keeps its own session table, keyed by the session's 128-bit ID, recording a session at its first commit there
with the last request number and stored reply; it refuses a number below its last and accepts the gaps the session's
commits elsewhere leave. Only the metadata group's registry evicts sessions, bounded by `sessions_max`; the sequencer
carries each eviction to every partition with its next close, after which a partition purges the session's entry, so a
table holds at most `sessions_max` entries, and the frontend's owner check refuses a request of an evicted session, so
no partition takes an evicted session for a new one. A commit's session-table entry is in its round's record, so replay
rebuilds the table. A retry the frontend cannot route — after a reconnect, the open transaction gone — is asked of every
partition, which is rare and bounded by the partition count. The open transaction a reconnect leaves behind is
aborted by its partition's host when the connection closes, or at its lifetime limit by the host's own timer when the
close goes unseen, so the lifetime limit is the grace period and no client keeps a pending dictionary entry past it;
an aborted transaction releases its entries without writing them, and a transaction that took one carries it in its
own record (decided 2026-11-03).

*Alternatives considered:* a strict +1 counter per (session, partition), which the client cannot compute and which only
the frontend's memory could keep; and one session table in the metadata group, which would put every commit through
one state machine, the serial point shard-per-core removes.

### The sequencer and the metadata group

Both are sans-I/O state machines whose commands are records in an ordered log (`ultragraphdb_traits_log`), stored by the
WAL of `ultragraphdb_transactions_log` in their own directories (`/data/seq`, `/data/meta`). A command is applied when
its ticket completes, so an acknowledged command is durable. The metadata group snapshots its state (catalog versions,
the default-entry records, high-water marks, the placement map, principals, the session registry) every
`snapshot_rounds` rounds and trims below the snapshot. Both run inside the frontend process with one replica; stage 8
puts VSR behind the same interface.

Epoch lengths are settings, `epoch_ms_min` (1 to 1,000) and `epoch_ms_max` (`epoch_ms_min` to 10,000), required, with
the `dev` profile's 2 and 10; the
sequencer closes at `epoch_ms_max` or sooner, never before `epoch_ms_min`: when a snapshot read waits on the open epoch,
the frontend, which routes the read and hosts the sequencer, asks it for a close in process, and a partition whose
sequence would run out asks with a role command. `close_request_ms` is refused at start below twice `epoch_ms_max`, so a
reachable sequencer always answers within it.

The metadata group commits a new catalog version as pending; at its next close `c + 1` the sequencer assigns every
pending version the effective epoch `E = c + 2`, records the assignment in its own durable close record, which announces
it, and the metadata group learns `E` from that close (P24 rule 11). A frontend crash before the close leaves the
version pending, and the next close assigns it, so no window lets a partition stamp `E` without the version: a partition
stamps `E` only once the close of `c + 1` has been delivered to it. The group keeps every version a partition's
untrimmed log may name, and a partition loads them before replay. A first write of an undeclared predicate asks the
group for a default-entry record, keyed by (namespace, predicate) beside the opaque versions, and recovery asks again
for the predicates its replayed rounds wrote. Index builds start at a version's epoch in each partition, their start and
completion are logical records in its log, and a crash that interrupts one runs its backfill again; a type change is
checked by every partition at its epoch, and a refused one is restored by a new version.

### Partitions

A partition is the stage-5 transaction path instantiated per partition: its WAL directory, its commit step, its own
fjall instance (one per partition in this stage; one per core holding several replicas' keyspaces in stage 8, P11), its
dictionary cache, its sequence blocks. The storage process runs each partition on its own compio runtime, pinned when
configured. Routing checks the subject's bucket against the partition's ranges before a request reaches the commit
step; a miss answers with the owner from the partition's copy of the placement map.

A read-write transaction binds to the partition owning the first subject it reads or writes; when its first write names
only new blank nodes, to a partition the frontend chooses in turn; when its first operation is an insert-if-absent or an
upsert-and-link, to the partition owning the claimed value's bucket; and when its first operation names no subject
otherwise, to the partition owning the namespace's whole run if one does, the first operation aborting otherwise. It
reads and writes what that partition owns, and registers its references to entities owned elsewhere as pre-steps outside
it. It may make a reverse or fan-out read (a value-index lookup, a scan, an unbound subject, a bound literal object)
only when its partition owns the namespace's whole run, always so with one partition, reading its own entries at their
latest state; a read of another partition's subject, such a read over a namespace spread over several partitions, a
write or a delete by pattern to a subject another partition owns, a write of a unique value whose bucket another
partition owns, and any Drop reaching several partitions abort it with `CrossPartition` (a new abort reason in the
transactions error and the wire's error table). A read-only transaction reads across partitions through compute at its
level.

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

The core directive orders these choices: correctness, then performance, then scale. The frontend's metadata group,
sequencer and session registry have one owning core, and endpoint cores ask it by message for each commit's owner check and
each authentication's verifier, holding no copy: a stale copy would let a partition take an evicted session for a new one,
breaking exactly-once. The hop is paid by commits alone, which wait for a durable log anyway, and a turn's checks share one
message. A read of a read-write transaction reaches its partition through compute, on a connection apart from the Bind
stream, so the frontend holds it until the client's earlier writes are answered, and the partition records each entry's span
before sending it: the read set covers everything the client could have seen, and no more, so a stopped read conflicts with
nothing it did not return. Read-only reads pay none of this.

Zero copy covers the wire and the log, and stops at the store. A frame is read in place through its views, with no parse
step. A Write names terms by text, so its bytes are not the log's: the record holds resolved quads and system IDs, built
once, because the log must replay without the dictionary's answers. Text stays borrowed from the frame until an ID replaces
it, and the record's bytes are what the log stores, what replay views where they lie, and what stage 8's replication carries,
so no serialization step stands between them. The frontend relays frames unchanged. The store is where copying is
unavoidable: applying a round derives keys and posting lists and writes them to fjall, since the protocol's bytes cannot be
the persistent store while a KV backend holds that role. Entries carries decoded lists for the same reason. `Changeset`,
which owns its terms, is therefore a convenience for tests and tools, never the wire's path.

Thread per core rests on compio: one runtime per core, and `RuntimeBuilder::thread_affinity` for pinning. That call returns
nothing and on failure only logs. It drops CPUs outside the permitted set, so under a container's cpuset a CPU number counted
from zero can leave it nothing to bind, and on macOS it sets only a cache-locality hint. A role therefore reads the permitted
set first and reads back what it holds: an unreported failure to pin looks exactly like success. Ownership gives correctness on its own, so
pinning is only ever performance. Each partition's core owns its address, so peers batch per address, trading a frame's reach
across a node's partitions for never crossing cores on arrival; correctness and contention outrank that batching.

The read kernel is one walk over two sources, so a transaction's scan and a plain one cannot disagree about a key's row.
A transaction's `Request` is already its overlay: its held lists are sorted by key and hold its writes. The walk merges the
store's keys with the keys a source adds, lowest first, and asks the source for each key's value. A plain read's source adds
no key and reads `Lists`. A transaction's source adds its written keys and resolves a list without changing the request: a
held list as held, a list written blind from the store with its delta on top, any other from the store. The walk neither
inserts into `held` nor marks a list read whole, so a scan of a million lists costs the request one range and no memory:
calling `Request::get` per key, the obvious way, would record a list read per key and keep each list. Each frame reads the
same version and resumes from a key and an object, so no cursor borrows the transaction across a credit wait. In the
dictionary the transaction's source is its `WriteTerms`.

A Scan naming a transaction carries the text of the IRIs it needs IDs for, since a dictionary key holds only a hash and an
absent term is recorded by its ID, which reading creates. Recording the dictionary key instead would miss a conflict: a
home's registration round can create the entry without a statement, and a later statement of it then writes no such key.

A request declares its type by its command, because reads alone and writes alone are simple and the costs come from mixing them.
Read-only needs no commit. Read-write holds an open transaction, a Bind stream and a sweep entry. DirectWrite is one frame of unconditional
changes, and ConditionalWrite one frame whose commit-time operations the commit step decides in order. The frontend relays either and the
partition executes it in one turn, so a plain insert pays for no transaction state, and a DirectWrite is the natural unit for any later fast
path and for ordering across partitions, since its write set is declared. As no read reaches their client, a conflict on what the execution
tested is the partition's to retry, which a read-write transaction cannot do. The catalog checks a DirectWrite's claim, refusing a unique
predicate rather than running a condition the client did not ask for. A write computed from the client's own earlier read cannot be seen by the
server, so the API documents that such a write is a ConditionalWrite or a read-write transaction.

Role binaries are tested as processes, as scrith's services are: a tests crate copies each binary into the Bazel sandbox
with `copy_file`, starts it through `service_utils`, whose 0.4 is `std` alone, reads a process's exit code and isolates its environment, and waits on `/health` or `/ready`, dependencies first, and drives it
with the actual client. These tests need no container, so Bazel caches them; they declare fixed ports through the roles'
specs crates and carry `exclusive` so none runs beside another, tagged `integration_test` and run as their own group of
`scripts/test.sh`. `/health` and `/ready` therefore exist in every role from its first commit, ahead of the role's logic.

The frontend endpoint takes stage 5's server source material from backlog items 2 and 3: connections bound per core,
the handshake, SCRAM against the metadata group's principals, sessions, routing by bucket, statement caches, the
error table. Compute asks storage's read kernel — a scan of one keyspace's range at a version, bounded by a row limit —
through the routes of *One owner per fact*, and merges partitions' rows in key order. A transaction names on Begin the
level, session token and deadline every one of its reads shares, since they read one snapshot. A resume key is an
XXH3-checksummed token holding the pattern's hash and the last row by its terms in text form, never a store key, so a
client never sees an ID. A statement without a statement ID is named to a client by its quad; one with a statement ID
(it carries properties or repeats a quad as a parallel edge) by its reifier, a skolem IRI minted as a blank node's with
the statement's bucket and held in the dictionary as the reifier's name, through which a client addresses its
properties as RDF 1.2 reification does. A commit-time operation's node is named by its IRI or skolem IRI.

### The binaries, the supervisor and the images

Each binary reads its settings from a standard TOML file, one table per concern (`[transport]`, `[storage]`,
`[sequencer]`, ...), parsed by `ultragraphdb_tool_toml` (decided by the user, 2026-11-02): an in-house parser of
exactly the TOML subset the settings use (bare-named tables, bare keys, basic strings, decimal integers, booleans,
comments), refusing every other feature by name. A document it accepts parses as any TOML 1.0 parser would read it, so
the files stay standard; a new setting needing new syntax extends the parser, not the dependency list. A role reads its
settings through `ultragraphdb_common_service_config`'s `ServiceConfig`, whose types mirror the file table for table and
key for key, which takes every setting and refuses any other, so a misspelled setting stops the start. The crate sits
below the cluster crates, so it checks a value's kind and leaves its bound to the constructor of the domain type it
configures (`SequencerConfig::new`, `MetadataConfig`), the one place each bound is stated. An
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
  away, up to the 2^21 commits its sequence holds; once its epoch is full it asks for a close and holds its commits,
  refusing them with `Busy` only after the request has gone unanswered for `close_request_ms`; reads at a snapshot wait,
  within their deadline.
- [An inbound read fans out to every partition its entity's referrer set names, after one round at the home] → The
  fan-out is batched per node and bounded by the 2^10 groups of a namespace run; an inbound summary cache at the home
  stays a later option, taken only if stage 7's measurements ask for it.
- [A partition's first reference to an entity it does not own waits for a durable round at the home] → The mark makes
  every later reference free, registrations are batched per home node within each write request and issued while the
  transaction executes, and `wal_bench` measures the wait with a share of edges to other partitions' entities.
- [A protocol error in registration, stamping or reclamation loses inbound edges silently] → Rules 4, 5 and 9 are
  proved in `lean/PlacementModel` before the partition code exists, and the simulator checks every reverse read at a
  closed epoch against a single-partition reference model.
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
