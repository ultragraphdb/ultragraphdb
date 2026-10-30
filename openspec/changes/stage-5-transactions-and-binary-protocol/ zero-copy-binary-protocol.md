# Stage 5: transactions and the zero-copy binary protocol

Stage 5 makes every acknowledged commit survive power loss, takes write execution from one serial lock to n−1 cores
under strict serializability, and replaces every protobuf and gRPC type in the repository with a binary format of its
own that a receiver reads in place. The transaction mechanism and the protocol are designed together, because in a VSR
cluster they meet: a round of decided transactions is one message, written to the log, replicated and applied as the
same bytes.

The note is the stage's design, approved on 2026-10-27: it states what the code does today, what changes, and the
decisions taken. It refines `UltraGraphDB Dual Query Engine Plan.md` ("The write path on n−1 cores", "Stages", row 5) where
evidence from the code or the literature contradicts it; those departures are listed in "Where this departs from the
plan".

Sources are cited inline. Where a figure was read in a paper's text or a repository's source it is cited by section,
file or line; "unverified" marks what came from an abstract, a secondary summary or memory.

## What the code does today

| Area | Today | Where |
|---|---|---|
| Execution | Every changeset of a batch is executed under one lock (`apply`), serially, each reading at `latest_ts` with the deltas of the earlier requests of its batch stacked on top | `ultragraphdb_store_apply/src/types/changeset_core/mod.rs:100-150`; `ultragraphdb_store_posting/src/types/batch/mod.rs`, `stacked_layers/mod.rs` |
| Commit | `Lists::commit_with` takes the writer lock, runs a check hook per request, gives each kept request a fresh timestamp from a local counter and writes the batch as one store batch with the applied timestamp | `ultragraphdb_store_posting/src/types/lists/lists_commit.rs:61-160` |
| Validation | The hook aborts a request whose new IRI another writer created, or that read a key a refused request of its batch wrote. No read is checked against concurrent commits: execution is serial, so none can exist | `ultragraphdb_store_quads/src/types/quad_store/mod.rs:90-142` |
| Read set | `Request::read_keys` records whole list keys the request read from the store; scans through `posting_lists()` and dictionary lookups are not recorded | `ultragraphdb_store_posting/src/types/request/mod.rs:163-168` |
| Timestamps | One counter (`Timestamps`) gives commit and rollup timestamps alike; a read gate waits for the batch in flight | `timestamps/mod.rs`, `lists/mod.rs:133-163`, `lists/lists_rollup.rs:166-171` |
| Durability | `Durability::Process` by default: each batch reaches the OS, the store syncs once a minute; `Filesystem` syncs each batch. The stage-4 measurements ran under `Process` | `ultragraphdb_store_kv/src/types/durability.rs`; `../../notes/dgraph-port-overview.md`, "Measured, stage 4" |
| Identifiers | UIDs and statement IDs from leased blocks of 10,000, each block's end synced before use | `ultragraphdb_store_quads/src/types/leases/mod.rs:68-79` |
| Vector index | A vector insert reads the index's entry list through the request, so the key enters the read set; level draws come from one sequential SplitMix64 stream | `changeset_write_vector.rs:48-52`, `:126`; `ultragraphdb_store_apply/src/types/level_draw.rs` |
| Protobuf as storage | `ultragraphdb_proto_pb`'s `PostingList`, `Posting`, `UidPack`, `UidBlock` and `List` are the posting lists' in-memory and on-disk form, encoded by prost in one place | `ultragraphdb_store_posting/src/utils/posting_codec.rs:22-96`; used in 53 source and 36 test files across `core_codec`, `core_algo`, `core_types`, `core_tok`, `store_posting`, `store_quads`, `store_apply`, `store_hnsw` |
| Protobuf elsewhere | `ultragraphdb_proto_dgraph`'s `api::Value` in `core_types::object_value`; `pb::SrcFunction`, `pb::TaskValue`, `pb::Facets` in `core_types`' geo filter and sort; `pb::VectorIndexSpec` in `core_tok::get_index_factory_from_spec`. None of the five functions is called outside its crate | `ultragraphdb_core_types/src/utils/{conversion,geofilter,sort}.rs`, `ultragraphdb_core_tok/src/utils/index_factory.rs` |
| gRPC elsewhere | The OTLP span exporter uses tonic (`with_tonic()`); it stays | `ultragraphdb_common_telemetry/src/telemetry/init.rs:49-50` |
| Build | `protobuf` and `rules_rust_prost` in `../../../MODULE.bazel`, a prost toolchain in `bazel/toolchains/prost_toolchain` | `MODULE.bazel:17, 74-81` |
| Server, client | None: `ultragraphdb_client` was removed in stage 4, and no server binary exists | — |

Three consequences:

1. **Removing protobuf replaces the posting lists' storage format,** not only an API. The encoding is confined to
   `posting_codec.rs`; the types are not.
2. **Stage 5 moves execution out of the commit lock.** Requests no longer read their batch's stacked writes; each reads
   its own snapshot, and validation replaces the ordering the lock gave.
3. **The stage-4 baseline is without per-commit sync.** Stage 5's numbers are with every acknowledgement durable, so
   the baseline is measured again under `Durability::Filesystem` before the comparison.

## Goals

- **Fast:** write execution on n−1 cores; one short commit step; no decoding of messages, results or posting lists on
  the hot path.
- **Scalable:** the single node's commit step, log and apply are the cluster's ordering service, replicated log and
  storage apply, behind interfaces shaped as messages, so stage 10 changes the transport and replication, not the
  semantics.
- **Reliable:** strict serializability; an acknowledged commit survives power loss; no input, however malformed,
  panics or corrupts; each of these is checked by a deterministic simulator, history checkers and kill tests
  ("Verification").
- **No protobuf, no gRPC of the repository's own:** `../../../ultragraphdb_proto` is gone at the end of the stage, with its Bazel
  toolchain and the workspace pins of `prost`, `tonic`, `tonic-prost` and `tonic-prost-build`. The OTLP span exporter
  keeps speaking gRPC through tonic (decided 2026-10-27), so `tonic` and `prost` remain as dependencies of
  `opentelemetry-otlp` inside `ultragraphdb_common_telemetry`, and of nothing else.

## The transaction mechanism

### Roles in one process

| Role | Runs on | Does | Stage 10 |
|---|---|---|---|
| Workers | n−1 compio runtimes, one per core | Sessions, interactive transactions, execution against a snapshot, read sets, write-set records | Compute nodes |
| Commit step | One core | Sequencer, resolver, commit-time operations, log append, apply, watermark, replies | The ordering service's primary |
| Ordered log | The commit step's core | Durable, ordered, checksummed rounds | Replicated with VSR |
| Store apply | The commit step, then fjall's threads | Writes each decided round to fjall and the posting cache | Storage nodes |

The roles talk through messages in the formats below, so moving one to another machine changes how a message travels,
not what it says.

### One transaction, end to end

1. **Begin.** A worker opens a snapshot at the visibility watermark W: every round at or below W is durable and
   applied, and no commit can land at or below it.
2. **Execute.** Reads go through the posting cache at W and record a read set; writes build a write-set record in the
   binary format: list deltas, dictionary entries, statement records, catalog entries, versionless entries, the commit
   record the client attached, and the write's conflict keys. New IRIs take UIDs from the worker's lease, as today.
3. **Commit request.** The record and the read set go to the commit step's queue. The record is never re-encoded after
   this point.
4. **Round.** The sequencer closes a round when the previous round's log sync completes, bounded by bytes and
   transactions (D6). It assigns commit versions in arrival order.
5. **Resolve.** Each transaction's read set is checked against the conflict history for versions above its read
   version, then against the writes of the transactions admitted before it in the round. An admitted transaction adds
   its writes to the history at its commit version (D3).
6. **Commit-time operations.** Operations the transaction asked the commit step to decide (D4) run against the state
   as of the transactions admitted before them.
7. **Log.** The round, its decisions and the admitted records, is appended and synced (D1).
8. **Apply.** The admitted records are written to fjall as one batch carrying the round's position, and the posting
   cache is updated, as `commit_with` does today.
9. **Acknowledge.** W advances over the round; each transaction's reply carries its commit version, or its abort and
   the reason.

Read-only transactions take steps 1 and 2 only, are never validated and never abort.

### D1. The log is the durability point; fjall is derived state

The durability point is the write after which an acknowledged commit survives power loss, and the state recovery
starts from. Today it is fjall's journal: each commit batch is one fjall write batch, synced once a minute under
`Durability::Process` or per batch under `Filesystem`, and recovery is fjall's own replay of its journal.

**The options:**

| Option | What it is |
|---|---|
| **A. Log first (recommended)** | A round is appended to a log of the project's own and synced; then it is written to fjall with `PersistMode::Buffer`; fjall is synced only at checkpoints; recovery replays the log above the position fjall last recorded |
| B. fjall is the log | A round is one fjall write batch, synced; the ordered-log interface is implemented over a keyspace inside fjall that holds the round records |
| C. fjall without its journal | As A, with fjall's own journal turned off, so each round is written once. fjall offers no such setting that this note found (unverified), so it needs a fork, which the pin at 3.1.12 already makes costly (`../../notes/perf-optimization.md`) |
| D. A storage engine of the project's own | The log and the data in one engine, as TigerBeetle's grid. Rejected on 2026-10-23: a database needs ordered keys, MVCC and compaction, which fjall provides |

**Why A, in order of weight:**

1. **The durability point must be the part the tests can break.** fjall writes through `std::fs` with no seam, so a
   power loss, a sync that reports success and loses data, or a torn page inside its journal cannot be injected; the
   simulator checks them on the simulated store only, and fjall itself only through kill tests
   (`ultragraphdb_store_sim/README.md`, "Neither injected nor detected"). The log is written by this project over a
   storage trait with a simulated implementation, so every one of those faults is injected beneath the exact code that
   decides whether a commit survives. Under B, "never loses an acknowledged commit" rests on the one component the
   simulator cannot reach.
2. **Stage 10 moves the durability point to a log in any case.** Under VSR a commit is durable once a quorum of replicas
   has journaled its prepare, before any storage node applies it; storage is applied from the log afterwards. B builds
   recovery from fjall in stage 5 and recovery from a log in stage 10. A builds the second once, in stage 5, and runs
   every simulator seed and kill test against it from then on.
3. **The log is checksummed and chained; fjall's batches are not chained.** A round's frame carries its parent's
   checksum (D2) and its chunk table ("Body checksums"), so a lost, reordered, torn or misdirected round is detected and
   named. fjall's journal checks each batch on its own.
4. **The sync is cheaper and off the apply path.** The log appends to preallocated segments, written with direct I/O
   through compio, and syncs with `fdatasync`, which flushes no metadata a later read does not need (`fdatasync(2)`), so
   none for a file that does not grow. On the drives Mark Callaghan measured with 16 KB direct writes, `fdatasync` took
   from 1.07 times (Samsung 990 Pro) to about 16 times (GCP Hyperdisk) less than `fsync` (Small Datum, "SSDs, power
   loss protection and fsync", January 2026). Whether fjall preallocates its journal was not checked. Applying a round to fjall, its memtable inserts and the posting cache, runs while the next round
   syncs, where B syncs and applies one round after the other on the commit core.

**What A costs:**

- **Every round is written twice:** once to the log, synced, and once to fjall's journal, unsynced. The round's bytes
  are much the same as the fjall batch's, since both are list deltas and entries, so the write volume of commits
  roughly doubles. fjall's journal held 66 % of the bytes on disk in the stage-4 bench at one relation per commit
  (`../../notes/dgraph-port-overview.md`, "Measured, stage 4"); the log is trimmed at every checkpoint, so it adds bandwidth more than space. The stage measures both.
- **Code moves forward:** the log, its compio storage and its simulated storage are built in stage 5 rather than with
  replication. They are the same code stage 10 needs.
- **Recovery takes longer:** fjall's own replay, then the log's rounds since the last checkpoint. The checkpoint
  interval bounds it.
- **Two stores must agree.** The invariant is that fjall's applied position never exceeds the log's committed position;
  recovery refuses to open otherwise (below).

**How it works on the current KV store:**

1. The commit step appends round r to the log, then syncs it.
2. It writes r's admitted records to fjall as one `WriteBatch` carrying r's position as the applied position, with
   `PersistMode::Buffer`, then updates the posting cache. These are the steps of today's `commit_with`, minus the sync.
3. The watermark passes r once r is both synced and applied, and r's transactions are acknowledged.
4. At a checkpoint, by bytes or time, the commit step syncs fjall (`Store::sync`), records the position in the log's
   superblock, and trims the log's segments below it.
5. On open, fjall replays its journal to a prefix of the batches it was handed, which may be behind the log. The
   commit step reads the applied position from fjall and applies every round above it from the log, each idempotent
   because its batch carries its position and is whole or absent.
6. Rollups, compaction and garbage collection stay local maintenance of fjall, outside the log. A rollup folds only
   applied rounds, and fjall's prefix recovery keeps a rollup's batch only with the batches before it.

**How it works with stage 10's distributed tiles:**

- **The plan already makes storage derived.** Stage 10's design has tiles "rebuilt from a checkpoint and the log, and
  applied deterministically" (plan, "The distributed design", point 5). Under A, fjall in stage 5 is the first instance
  of that rule, and a tile group is a second consumer of the same rounds behind the same apply interface.
- **Storage differential testing reads one log.** The plan's first stage-10 step builds read-only tiles beside the KV
  store and requires "identical results on the KV and the tile paths". Replaying one log into both stores gives them the
  same input, round by round, and a difference names the first round where they part.
- **A tile moves as a snapshot plus the log.** Moving or rebalancing a tile ships the tile at position p and the rounds
  after p; a new replica catches up the same way, as TigerBeetle's state sync does (`docs/internals/vsr.md`).
- **The log is trimmed behind its slowest consumer:** below the lowest checkpoint of every storage node holding a tile.
  `trim_through` (D2) takes that position. A node that falls further behind takes a snapshot instead of the log.
- **The single node migrates without a rewrite.** The local log is sealed at its tail and the VSR log continues from
  the next position, as Delos chains Loglets (OSDI 2020, §3.4).

Option A carries forward the decision of 2026-10-23 for the replication log: an append-only log of framed records, a
two-copy superblock, compio, a simulated storage in Iggy's style. Its records are frames with XXH3-64 chunk checksums
rather than the CRC32C framing decided then; its layout on disk is "The WAL, after Apache Iggy's". Its requirements:

- **The log is synced before a round is applied**, so fjall never holds a round the log could lose.
- **Recovery** opens fjall, reads the applied position its last batch recorded, and replays every round above it.
  Applying a round is idempotent: its batch carries its position and is whole or absent.
- **fjall recovers a prefix of its batches after a power loss.** The simulator models this (`SurvivorsArePrefix`); fjall
  itself is checked only by kill tests. A replay that finds the store ahead of the log, or a gap, refuses to open.
- **Checkpoints:** periodically the store is synced and the log trimmed through the applied position.
- **Writes outside the log stay as they are:** lease marks, drop and rebuild records, the discard timestamp and the
  banned namespaces are synced to the store directly. Stage 10 moves leases into the log.
- **Corruption:** a torn tail is truncated only when no complete round follows it, and the truncation is reported; a
  corrupt round with a complete round after it is fatal and never read as absent ("Protocol-Aware Recovery", FAST 2018,
  as TigerBeetle applies it in `src/vsr/journal.zig`). On one node a round the drive tore while reporting its sync
  complete is lost with the truncation, and the report names it; from stage 10 a replica repairs it from a peer.
- **A failed sync is fatal**, as a failed store write is today (`Lists::fail_writes`).

### D2. The ordered-log interface

Stage 5 defines it with a local implementation; stage 10 implements it with VSR. Its shape follows Delos' Loglet
(OSDI 2020, §3.4: append, check tail, read, trim, seal), whose seal is what lets the local log be chained to a
replicated one without rewriting it, and TigerBeetle's prepares (`src/vsr/message_header.zig:688-739`: a parent
checksum chaining each prepare to the one before).

```rust
pub trait OrderedLog {
    /// Primary only. Accepted when the record's position is the next one and its parent is the previous record's
    /// checksum.
    fn append(&mut self, round: Round) -> Result<Position, LogError>;
    /// The highest position durable: synced on one node, on a quorum in VSR. Never decreases.
    fn committed(&self) -> Position;
    /// In order, each record's checksums and parent verified; a reader applies only up to `committed()`.
    fn read(&self, from: Position, max_bytes: u32) -> Result<RoundIter<'_>, LogError>;
    /// Positions through `p` are covered by a durable checkpoint and may be removed.
    fn trim_through(&mut self, p: Position) -> Result<(), LogError>;
    /// Later appends fail; the final tail is returned.
    fn seal(&mut self) -> Result<Position, LogError>;
    /// View changes and the replacement of an uncommitted suffix, for the sequencer.
    fn next_event(&mut self) -> Option<LogEvent>;
}
```

- **Durability is reported, not awaited inside `append`**, so the commit step pipelines: round k+1 is resolved while
  round k syncs. The method is synchronous over a compio-driven implementation; the exact async boundary is set in the
  stage's design.
- **`LogError` is split by what the caller does:** retry or redirect (`NotPrimary`, `Busy`), outcome unknown and
  resolved through the client's session (`Truncated`), programming errors (`PositionMismatch`, `ParentMismatch`,
  `TooLarge`), and fatal (`Corrupt`, `Diverged`, `Io`).
- **What is logged is decisions, not inputs.** A round carries the admitted write-set records, so a replica applies
  without re-executing or re-validating; Calvin and Aria log inputs and must re-execute deterministically.
- **Positions are gapless; versions are not** (D5). A round records its first commit version and its transactions'
  count, so every version is recovered from the log.

### The WAL, after Apache Iggy's

The log of D1 is the node's write-ahead log, the local implementation of `OrderedLog`. Iggy builds two WALs of its own
over compio, both read in its local checkout (`/Users/marvin/RustroverProjects/iggy`, commit `fe3b426`, 2026-10-15).
The metadata WAL's recovery follows TigerBeetle's rules for corruption; the partition WAL's recovery was not read.

**What Iggy does:**

| Part | Iggy | Where |
|---|---|---|
| Entry | The replicated message itself, a 256-byte header and its body, written as received: nothing is re-encoded between the network and the disk | `core/journal/src/prepare_journal.rs:124-129` |
| Metadata WAL | One append-only file; each append writes at an offset reserved before the write, then `sync_data` (fdatasync); an in-memory index of 1,024 slots holds each entry's header and offset by `op % slots` | `prepare_journal.rs:70`, `:1160-1240`; `file_storage.rs:82-85` |
| Partition WAL | Blocks of 4,096 bytes written aligned; a batch of prepares appended without a barrier, then one `sync` for the batch; capacity bounded between 128 MiB and 4 GiB, which refuses an append past it | `partition_journal.rs:41-45`, `:689-760` |
| Acknowledgment point of the partition WAL | A "frontier": a file of two 4 KiB slots, rewritten in place alternately, sequence-numbered and checksummed, published after the WAL's sync; the newest slot that verifies wins | `partition_journal.rs:46-74`, `:389-427`, `:1317-1334` |
| Bodies written once | A partition WAL record may hold only a header and a reference (generation, offset, position, length) to a message body already synced in a data segment, kept alive by a hard link | `partition_journal.rs:88-98`, `:734-753` |
| Recovery scan | Per entry: a valid bit pattern, the command, a size between the header and 64 MiB (so a flipped `size` cannot allocate gigabytes), the header checksum, the parent chain, the body checksum (XXH3-64) | `prepare_journal.rs:535-700` |
| Torn tail or corruption | Damage is truncated only when no complete entry follows it, found by probing every offset after the damage; damage with an entry after it is interior corruption of acknowledged data and refuses to start | `prepare_journal.rs:198-308` (`truncate_or_fail`, `find_complete_entry`) |
| Validation before the write | An append that would evict an entry not yet covered by a snapshot is refused before the write and sync, because a refusal after the sync would recur at every restart | `prepare_journal.rs:1186-1207` |
| Compaction | The metadata WAL rewrites its live entries to a temporary file, renames it over the WAL and syncs the directory; a failure after the rename poisons the journal until it is reopened | `prepare_journal.rs:883-950`, `:1079-1158` |
| Superblock | Two files written alternately: a temporary file, synced, renamed over the older copy, the directory synced. "Empty", "present" and "unreadable" are three outcomes, so a lost superblock never reads as a fresh deployment; an unreadable copy blocks every later write | `superblock.rs:18-37`, `:107-131` |
| Storage seam | `DurableStorage` and `DurableFile`: open, preallocate, aligned reads and writes, truncate, file sync, directory sync, rename, hard link; implemented over compio and by the simulator | `durable_storage.rs:57-215` |
| Simulated storage | Each file and directory holds buffered and stable contents; a file sync makes a file's contents stable and a directory sync its entries; a power loss returns both to stable; faults before or after an operation, a torn write reported as an error, and a silent torn write | `core/simulator/src/storage.rs:18-54`, `:133-145` |

**What the WAL takes from it:**

- **A round is written as its frame.** The commit step writes the round's frame, header and body, at a block boundary,
  as Iggy writes a prepare; stage 10's replicas journal the same bytes.
- **Segments, not one file.** The WAL is a directory of preallocated segment files of a fixed size, so trimming below a
  checkpoint removes whole files, where Iggy's metadata WAL rewrites its live entries. A round never spans segments.
- **Aligned, batched, one barrier per round.** A round is padded to whole 4,096-byte blocks and written with one aligned
  write, then one `fdatasync`. Iggy's partition WAL pays two barriers per batch, the WAL's sync and then the
  frontier's; the WAL here has no frontier, because its records are self-identifying (next point), so the scan finds the
  durable tail itself, as Iggy's metadata WAL and TigerBeetle's do.
- **Self-identifying records.** A round's header carries its position, its parent's checksum, the cluster and the WAL's
  incarnation, a number the superblock raises each time the WAL is created afresh. A scan stops at the first block that
  is not the next round of this incarnation, so a preallocated segment's zeros or a reused segment's old rounds are never
  read as rounds.
- **Corruption rules unchanged from Iggy's and TigerBeetle's:** the size bound before any allocation; header checksum,
  chain and chunk table checked in that order; a torn tail truncated only when no complete round follows it; interior
  damage refuses to start, and the report names the round, the chunk and the transactions it holds ("Body checksums").
- **Refuse before writing.** A round that would exceed the WAL's capacity, or whose position or parent does not follow,
  is refused before its write, not discovered after its sync.
- **The superblock is Iggy's ping-pong pair** with its three outcomes. It holds the cluster, the incarnation, the last
  checkpoint's position and checksum, and the first segment kept. It is written at checkpoints only, never on the
  acknowledgment path.
- **The storage seam is Iggy's,** `DurableStorage` and `DurableFile` with the same operations, implemented over compio and
  by a simulated storage with the same model of buffered and stable contents, directory entries and the four fault
  modes. That is the file-level simulation D1 relies on.
- **A failure poisons the WAL** until it is reopened, as Iggy's does after a failed rewrite and as the posting crate does
  after a failed store write.

**Where it differs from Iggy's:**

- **No `unsafe`.** Iggy's `FileStorage` reaches its file through an `UnsafeCell` (`file_storage.rs:29`). Here the commit
  step owns the WAL, and the WAL owns its files; methods take `&mut self`.
- **Bodies are written twice, as D1 states.** Iggy's segment references avoid it because a message body has one home,
  the partition's data segment. A round's write sets land in fjall, whose files the WAL cannot reference.
- **No unsealed entries.** Iggy accepts entries with a zero body checksum from builds that predate sealing
  (`prepare_journal.rs:44-57`); there are no such builds here.

### D3. Validation is exact, as FoundationDB's resolver

The plan validates each transaction against every predecessor in its window, aborted ones included, so that checks are
independent and run in parallel. On one node that rule only adds aborts:

- FoundationDB's resolver checks a batch exactly: all read ranges of the batch against the history first, then the
  transactions against each other in batch order with a bitset, an aborted transaction adding no writes
  (`fdbserver/resolver/ConflictSet.cpp`, `ConflictBatch::detectConflicts`). One single-threaded resolver handles about
  280,000 transactions per second (SIGMOD 2021, §2.4.2), against scrith's 8 writers.
- FoundationDB accepts false conflicts from aborted transactions only across range-partitioned resolvers, and bounds
  them by its 5 s window (§2.4.2).
- Under Zipf-distributed deduplication the conservative rule chains aborts: T1 aborts, and T2 aborts on T1's discarded
  write.
- **Logging decisions removes the reason for it.** A new primary rebuilds the history from the committed write sets in
  the log; no replica re-validates, so no replica needs the same aborts.

**The conflict history** maps write keys and ranges to the version of their last write, pruned below the oldest open
read version, which the 10 s limit bounds. FoundationDB holds it as a skip list whose nodes carry the maximum version
below them, so a range check is one range-maximum query. The first implementation is a `BTreeMap` of points beside an
interval map of ranges, measured before anything faster is built.

**What is recorded and what conflicts:**

| A transaction | Records | Conflicts with a later-ordered commit that |
|---|---|---|
| Read a list | The list key | Wrote the list |
| Tested one statement: an insert's existence check, a delete | The list key and the object | Added or removed that object in that list |
| Scanned: an unbound subject, an index range, a declaration's "does the predicate hold data" | The key range | Wrote a key in the range |
| Looked up a term absent from the dictionary | The dictionary key | Created that term |
| Wrote anything in a namespace | The namespace's catalog range, as read | Changed the catalog (D7) |
| Wrote blind: inserts' reverse, predicate-list, index and statistics deltas | Writes only | Nothing: two writes never conflict with each other |
| Was ordered before a drop | — | A drop writes its whole range: a transaction whose read version is below the drop and that read **or wrote** in the range aborts, or its writes would land under a dropped prefix |

The record of writes carries objects for lists a commit changed by a few objects; past a bound it carries the list key
alone, which can only add conflicts.

**The code change this needs:** `Request::read_keys` records whole lists, and an insert reads its subject's forward
list to find whether the statement is already held (`QuadWrite::objects`, `quad_write/mod.rs:160-173`). Two concurrent
inserts of different objects for one subject and predicate would conflict. The posting crate's request records the
statement it tested instead, scans gain a recorded range, and dictionary lookups record absence.

### D4. Hot keys: commit-time operations

The plan sends a key whose abort rate passes a threshold to a pessimistic per-key queue in the commit step. With
interactive transactions, a lock held in a queue lives across client round trips. The work on hot keys points the
other way: Bamboo (SIGMOD 2021) releases hot locks early, for up to 19 times the throughput of wound-wait on a single
hotspot, and Chiller (SIGMOD 2020) runs hot records last so their locks are held briefly (from its abstract,
unverified). The replacement decides the hot operation inside the commit step, where it is serial and short:

- **Insert-if-absent on a unique value:** the node with this value, created if no node holds it; the reply carries its
  UID. It replaces match-then-insert for deduplication and never aborts on the race.
- **Upsert-and-link:** insert-if-absent, and in the same step the links it names, edges with the node as subject or
  object, so a transaction deduplicates an entity and connects its own nodes to it atomically; properties it carries
  for creation are written only when it creates the node (added 2026-10-27). Its links are blind deltas, so concurrent
  links to one hot entity never abort each other.
- **Compare-and-set on a single-valued property.**
- **Counter add:** commutative, no read, as FoundationDB's atomic add (§2.6). scrith's allocator counters take it.

Each operation reads at most a few lists, through the posting cache, which holds a hot key. They are the stage-5
counterpart of the overview's server-side conditions. Strictness holds: they run in the commit order.

MOCC (PVLDB 10(2)) and Polaris (SIGMOD 2023: 1.9× Silo's throughput under high contention) support moving hot keys out
of plain optimistic execution; deciding them at commit resembles Chiller's inner region.

### D5. Versions

- **Commit versions follow the clock** as FoundationDB's do (§2.4.1, about one million a second): version = max(previous
  + 2, twice the sequencer's clock in microseconds), even, recorded in the round. "Too old" is then a distance in time, the 10 s
  limit is a version distance, and a commit-log cursor is a time. TigerBeetle chooses its prepare timestamps the same way
  (`src/vsr/replica.zig:7356-7365`). On one node a clock stepping back is absorbed by "previous + 2"; a clock stepping
  forward far raises every later version, so stage 10 bounds the clock as TigerBeetle does (Marzullo's algorithm,
  `src/vsr/clock.zig`). Iggy enforces only the lower bound, and its code documents the consequence: a runaway primary
  clock skews every wall-clock-derived value cluster-wide (`core/consensus/src/impls.rs:2403-2413` in the local checkout).
- **Rollups do not take versions from the sequencer.** Today a rollup draws a fresh timestamp from the same counter
  (`lists_rollup.rs:171`), and the counter starts two above the applied timestamp for that reason (`timestamps/mod.rs:21`).
  Under sequenced versions a rollup writes at a version no commit can take: commit versions even, a rollup at one above
  the newest commit version it folds, which is odd. A second rollup at the same version writes the same list.
- **"Epoch" is not reused.** It is the posting cache's epoch in `Lists` (`lists/state.rs:24`) and VSR's reconfiguration
  epoch (TigerBeetle's header, VRR §7). The plan's ordering unit is called a **round** here.

### D6. Rounds

- A round closes when the previous round's log sync completes, with no timer, as the plan decided; a lone commit waits
  about one sync.
- It is bounded by the maximum frame ("The header") and a transaction count, so one large round cannot delay every later
  acknowledgement.
- **A configuration flag states whether the WAL's drive has power-loss protection,** `false` by default, since a
  deployment's drive is not known. A sync costs from microseconds on such a drive to milliseconds without it ("Measurements
  at the exit"), so the flag selects the defaults of the knobs that sync cost decides: a round's byte and transaction
  bounds, and the checkpoint interval. Their values are set from stage 5's measurements on both kinds of drive. The flag
  never removes a sync: every round is synced before it is acknowledged whatever it says, so a flag set wrongly costs
  throughput or latency and never an acknowledged commit.
- **Inside a round, reordering is allowed but not built first.** No transaction of a round has been acknowledged, so
  any order within it respects real time. Aria's deterministic reordering (PVLDB 13(11)) and Ding et al.'s validator
  batching (PVLDB 12(2), 2018: up to 2.2× throughput and 71 % lower tail latency under high contention) commit transactions the arrival order would abort. Stage 5 measures the
  abort rate first and builds reordering only if the Zipf workload asks for it.
- **The Aria fallback is not built.** Aria needs one-shot transactions; interactive transactions cannot take it, and the
  commit-time operations cover the contended one-shot case.

### D7. Catalog changes, drops and index builds in the round

- **Catalog changes** are writes to the namespace's catalog keys, which every write in the namespace reads (the table in
  D3). A transaction executed against an old catalog aborts; catalog changes are rare, so the cost is a retry.
- **Drops** are commands in a round, written as a range write (D3); their physical removal keeps today's drop records.
- **Index builds** keep their background build and crash-safe record; the nodes a write touches during a build are
  recorded by the commit step in commit order, not by the worker, because execution order is no longer commit order.

### D8. The vector index is maintained in commit order, outside the transaction

A vector insert reads the index's entry list (`changeset_write_vector.rs:48-52`) and the neighbour lists it links, so
under optimistic execution every two concurrent inserts into one predicate's index conflict. The DeepBrain MVP needs
vector search (plan, "The vector index"), so its writes put vectors into indexed predicates; how many per write is
measured, not known (the vector-heavy workload in "Measurements at the exit"). The forward list holding the vector commits transactionally; an indexer then
inserts committed vectors into the HNSW graph in commit order, on a worker, and records the version it has indexed. A
search returns nodes whose vectors are visible at its snapshot, which it checks against the forward list; a vector
committed but not yet indexed is absent from a search for that interval. Two smaller changes go with it:

- **Level draws are a function of the node,** a hash of the seed, the namespace, the predicate and the UID, not the next
  value of one stream (`level_draw.rs`), so a draw is the same whatever order the indexer meets the nodes in.
- **The dimension check reads the catalog,** not the entry point (`check_dimension`, `:70`). The index declaration
  gains a dimension for it: `VectorIndexSpec` holds a factory name and options today
  (`ultragraphdb_core_mapping/src/types/vector_index_spec.rs`).

### D9. Sessions and exactly-once commits

TigerBeetle's client sessions (`src/vsr/client_sessions.zig`), adapted:

- A client registers a session through the log and receives its number; sessions are part of the logged state, so a
  new primary in stage 10 holds the same table.
- Each commit carries the session and a request number one above the last. The table holds each session's last
  committed request and its reply; a retry of it returns the stored reply, and a request number out of order is refused.
- **One commit in flight per session; any number of reads per connection.** Reads are not logged and need no session;
  a connection opens as many sessions as it needs concurrent writers.
- The table is bounded; the oldest session is evicted and an evicted session's request is refused with a reason the
  client acts on.
- TigerBeetle documents two bugs in the client table of Viewstamped Replication Revisited: request numbers that
  collide after successive client crashes, and a client locked out when a request is reordered through a view change.
  It avoids them by registering sessions through the state machine and by keeping uncommitted request numbers apart
  from committed ones (`src/vsr/client_sessions.zig:16-33`); this design takes both.
- The plan's retry deduplication by a unique value on the commit record's request identifier becomes unnecessary for
  commits sent through a session.

### D10. Read summaries are measured in stage 5, not used

Validation on one node reads exact read sets in memory, in-process. The plan's Bloom-filter summary exists for read
sets that travel to a remote ordering service, and its false positives become aborts on hot data. Stage 5 keeps the
plan's measurement: the simulator replays the log's write sets against each transaction's summary and its exact read
set and reports the false-abort rate per budget and bits per key. Stage 10 decides from it.

### Threads

- **The commit step** owns one core: sequencer, resolver, commit-time operations, the log's compio runtime and the
  apply. If its cost per transaction approaches the execution cost divided by n−1, the resolver's history checks go to
  the workers first, since they only read the history as of the previous round.
- **Workers** are compio runtimes pinned to n−1 cores. A connection is accepted onto the least-loaded core and stays
  there; an interactive transaction lives on its connection's core.
- **Queues between them are bounded.** A full commit queue refuses a commit with `Busy`, which the client retries, rather
  than growing.
- **fjall's flush and compaction threads** share the cores; their cost is measured, as the plan states.
- **Every loop is a step function** over its inputs, so the simulator drives it with no runtime and no clock of its own.

## The binary protocol

### Why not an existing format

| Format | Untrusted input | Evolution | `forbid(unsafe_code)` | Verdict |
|---|---|---|---|---|
| prost (protobuf) | Full decode, allocation per field: prost decodes the `log` dataset of rust_serialization_benchmark in 3.38 ms, where Cap'n Proto reads every field of it, validated, in 151 µs (README, updated 2026-10-05) | Good | Compiles | The cost this stage removes |
| rkyv 0.8 | Validation proportional to pointers | None | Derives compile | Open validation soundness issues #693, #698, #699, #708 against 0.8.18, where crafted input reaches undefined behaviour in safe code |
| FlatBuffers | Verifier, 46 ns on `mesh` | Good | Generated code contains `unsafe` | Needs an exempt crate |
| Cap'n Proto | Lazy, traversal limits | Good | Generated code has no `unsafe` (read, not compiled) | Segments and far pointers add a layer the reader must walk |
| SBE | Bounds only | Fields appended at the end | Generated code forbids `unsafe` | Ruled out: its code generator is legacy Java |
| Own layout, read with `from_le_bytes` | One length check per field | By `release` and appended fields | Nothing to exempt | **Chosen** |

Apache Iggy took the same path: rkyv first, then its own layout of a batch header, a separate index vector and message
bytes, with views instead of decoded messages, for 40 % more producer and nearly twice the consumer throughput
(iggy.apache.org, "Zero-copy (de)serialization", 2025-05-08). Reading a field with
`u64::from_le_bytes(buf.get(8..16)?.try_into().ok()?)` compiles to one length check and a plain load on x86-64 and
aarch64, instruction for instruction as `zerocopy`'s unaligned byte-order types do, so the safe form costs nothing and
needs no aligned buffers. This was checked in the assembly rustc emits, during the research for this note; the probe
is not committed.

### Crates

| Crate | Path | Depends on | Provides |
|---|---|---|---|
| `ultragraphdb_error_protocol` | `../../../ultragraphdb_error` | — | `ProtocolError(pub ProtocolErrorEnum)`, one variant per validation rule |
| `ultragraphdb_protocol_wire` | `../../../ultragraphdb_protocol` | `ultragraphdb_error_protocol`, `ultragraphdb_core_terms`, `xxhash-rust` (`xxh3`) | The header, commands, checksums, the body layouts and their views and builders; no I/O |
| `ultragraphdb_protocol_transport` | `../../../ultragraphdb_protocol` | `_wire` | The transport, sans-I/O and with no runtime: the pull-style framer, the handshake machines, the buffer pool, the streams with credit, the simulated network's model (renamed from `_bus`, sans-I/O, decided 2026-10-28) |
| `ultragraphdb_protocol_transport_compio` | `../../../ultragraphdb_protocol` | `_transport`, `compio` | The transport's compio shell, `Connection`, shared by the server and the compio client (decided 2026-10-28) |
| `ultragraphdb_client_core` | `../../../ultragraphdb_client` | `_transport` | The Rust client, sans-I/O, time passed in: every rule of the protocol a client follows (decided 2026-10-28) |
| `ultragraphdb_client_tokio` | `../../../ultragraphdb_client` | `_client_core`, `tokio` | The primary public Rust client, the core's tokio shell with `Send` futures (decided 2026-10-28) |
| `ultragraphdb_client_compio` | `../../../ultragraphdb_client` | `_client_core`, `_transport_compio` | The public compio client, the core's compio shell (decided 2026-10-28) |
| `ultragraphdb_server_session` | `../../../ultragraphdb_server` | `ultragraphdb_protocol_wire`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_transactions_*` | Handshake, sessions, dispatch to workers, the describe request |
| `ultragraphdb_server` | `../../../ultragraphdb_server` | `ultragraphdb_server_session`, `ultragraphdb_common_telemetry`, `ultragraphdb_store_kv` | The single-node server binary: configuration, startup, recovery, shutdown |
| `ultragraphdb_error_log` | `../../../ultragraphdb_error` | — | `LogError(pub LogErrorEnum)` (D2) |
| `ultragraphdb_traits_log` | `../../../ultragraphdb_traits` | `ultragraphdb_error_log` | `OrderedLog`, `Position`, `Round` (D2) |
| `ultragraphdb_transactions_log` | `../../../ultragraphdb_transactions` | `ultragraphdb_traits_log`, `_wire`, `compio` | The local log: segments, superblock, recovery; its storage over compio and simulated |
| `ultragraphdb_transactions_commit` | `../../../ultragraphdb_transactions` | `ultragraphdb_traits_log`, `_wire`, `ultragraphdb_store_apply` | Sequencer, resolver and conflict history, commit-time operations, apply, watermark, sessions |
| `ultragraphdb_transactions_txn` | `../../../ultragraphdb_transactions` | `_wire`, `ultragraphdb_store_quads` | The transaction on a worker: snapshot, read set, write-set builder, limits |
| `ultragraphdb_transactions_sim` | `../../../ultragraphdb_transactions` | the above, `ultragraphdb_store_sim` | The node simulator and the history checker |

`xxhash-rust` and `compio` are new external dependencies, both pure Rust. The crate names follow the repository's rule
that a crate is named for its group folder. The server binary and the crates that serve it live in their own group
folder, `../../../ultragraphdb_server` at the repository's root (decided 2026-10-27), beside `ultragraphdb_protocol/`, which
holds what client and server share.

**Encryption** is not part of stage 5: TLS arrives in stage 7, with access control (decided 2026-10-27). Until then the
protocol runs in clear text on trusted networks; the header's reserved nonce keeps room for an authenticated cipher.

### The header

One 256-byte header, little-endian, for client traffic, log records and, from stage 10, replica traffic, as
TigerBeetle's and Iggy's are (`message_header.zig:17-78`; Iggy `core/binary_protocol/src/consensus/header.rs:231`).

| Offset | Field | Type | Meaning |
|---|---|---|---|
| 0 | `checksum` | u64 | Of bytes 8..256: the frame's identity |
| 8 | `checksum_body` | u64 | The root of the body's chunk checksums ("Body checksums") |
| 16 | `nonce_reserved` | u128 | Zero; reserved for an authenticated cipher |
| 32 | `cluster` | u64 | The cluster's identity, drawn at random when the cluster is created; a frame of another cluster is refused. Two of 1,000 clusters share an identity with probability about 3 × 10⁻¹⁴ |
| 40 | `size` | u32 | Header and body |
| 44 | `view` | u32 | The VSR view; zero on one node and in client frames |
| 48 | `release` | u32 | The sender's release |
| 52 | `protocol` | u16 | The frame format's version |
| 54 | `command` | u16 | What the body is |
| 56 | `stream` | u64 | The request this frame belongs to, for multiplexing; zero for connection-level frames |
| 64 | `replica` | u8 | The sending replica; zero from clients |
| 65 | `flags` | u8 | Last frame of a stream, and others per command |
| 66 | `reserved_frame` | [u8; 62] | Zero |
| 128 | `command_header` | [u8; 128] | Laid out by each command: session, request number, read version, round position, parent checksum, counts |

- **Checksums are XXH3-64** as `doc/xxhash_spec.md` (v0.2.0) of the xxHash repository specifies it, so a native client
  in any language implements it from the specification or takes one of the implementations the reference site lists;
  in Rust, `xxhash-rust`, pure Rust. XXH3-64 runs at 31.5 GB/s with SSE2 and 59.4 GB/s with AVX2 on an Intel
  i7-9700K (xxHash's README).
- **Every checksum is stored as a little-endian u64,** like every other field. The specification's canonical form for
  display is big-endian; a client compares the integer, never the canonical bytes.
- **64 bits are enough for every use here.** Corruption escapes a check with probability about 2⁻⁶⁴. A checksum as a
  frame's identity and as a round's parent is compared pairwise: with the previous round, or between candidates for one
  position in a view change. Neither compares a checksum against all others, so the birthday bound does not apply.
  TigerBeetle uses 128-bit Aegis-128L as a MAC, which needs AES instructions and, in Rust, C for its fast path; CRC32C
  is faster but 32 bits. A keyed MAC can replace the function through `protocol` and the reserved nonce.
- **A log record is a frame.** The commit step writes a round as one frame, header and body, so the log and stage 10's
  replication carry the same bytes; a replica writes a prepare to its log as it arrived.
- **Validation order:** length; header checksum; command known, reserved bytes zero, the cluster, the header's
  fields, `size` at least the header and within the maximum; then the body, read to `size`; the chunk table against
  `checksum_body`, then each chunk against its entry; the sequence; then the command's own rules
  (`ultragraphdb_protocol/PROTOCOL.md` §2.5 states the order step by step). Every failure is a `ProtocolError`, answered with an
  error frame where the connection can still carry one and then closed. TigerBeetle crashes the process on an unknown
  command with a valid checksum (`src/message_buffer.zig:144`); this protocol never does.
- **Commands are never renumbered:** a retired command's number stays reserved, as TigerBeetle's `deprecated_N`.
  Ranges are reserved for the replica commands of stage 10.
- **The maximum frame** is 1 MiB, a multiple of 4,096 so a log record can be written with direct I/O, as TigerBeetle's
  (`config.zig:159`). A result larger than a frame is a stream of frames; a write set larger than a frame is refused,
  and bulk loads are stage 7's loaders.

### Body checksums

A body is checked in chunks, so a failed check names the chunk that broke, not only the frame:

- **Chunks follow the 4,096-byte blocks** the WAL writes with direct I/O, so a torn or lost sector of a WAL record is
  one chunk. A frame starts on a block boundary and its header fills the first 256 bytes, so the first chunk is the
  body's first 3,840 bytes and every later chunk 4,096; the last may be shorter.
- **Each chunk has a leaf checksum,** XXH3-64 seeded with the chunk's index, so two chunks swapped or a chunk written at
  the wrong offset fail their check.
- **The chunk table** is the leaves in order, at the end of the body, so a writer appends it once the last chunk is
  written. The chunks cover the body before the table, and the table's length follows from `size`. `checksum_body` is
  XXH3-64 of the table, and the header checksum covers `checksum_body`: header, table and chunks form one chain.
- **A body of one chunk carries no table:** `checksum_body` is that chunk's leaf, so a body of at most 3,840 bytes pays
  nothing.
- **Cost:** 8 bytes per 4,096, 0.2 %; a 1 MiB body carries a 2 KiB table. Verifying is one pass over the body, as a
  single checksum is, and the chunks can be checked in parallel or as they arrive.
- **Diagnosis:** a failed chunk is reported with its index and byte range, and the sections it overlaps: in a round,
  which transactions' write-set records. A failed table check means the table or `checksum_body` broke, and no chunk
  is trusted.
- **Partial checks:** a reader that needs one section, as the commit step reading one write-set record or a replica
  repairing one range from a peer in stage 10, checks the table and only the chunks under that section.
- **Why one level:** the tree is the table and its root. A deeper Merkle tree pays only when a reader checks one chunk
  without the table, with a proof of log₂(chunks) hashes; the table travels in the body, and a 1 MiB body has about
  256 leaves. A frame larger than the table's reach would take a second level.

XXH3 detects corruption, not tampering; an authenticated cipher replaces it through `protocol` and the reserved nonce
when frames cross networks that are not trusted.

### Bodies

Every body is a fixed part, then sections located by u32 offsets and lengths within the body, each section aligned to
8 bytes. A receiver checks a body once: offsets in bounds and monotone, counts consistent, UTF-8 where a section is
text. Afterwards every access is a view borrowing the receive buffer; nothing is decoded until a caller asks for a
value.

| Body | Contents |
|---|---|
| Hello, Welcome | Protocol range, release, client identity; the server's limits: maximum frame, in-flight streams, initial credit |
| Register | A session (D9) |
| Begin, Commit, Abort | A transaction on a stream; Commit carries the session and request number |
| Read | Stage 5's data requests: a quad pattern with bound and unbound positions, a limit and a resume key |
| Write | A changeset: the mapping crate's `Changeset` in this layout |
| Operation | A commit-time operation (D4) |
| Prepare, Execute | A statement and its parameter types, answered with a statement id and the result's column types and a schema id; Execute binds parameters by position |
| Describe | The data dictionary's answer |
| Result | A columnar batch (below) |
| Credit, Cancel | Flow control and cancellation of a stream |
| Error | A code, the stream, and a message |
| Round | The log record: position, parent, incarnation, first version and transaction count in the command header; the decisions and the admitted write-set records in the body |

**The write-set record** is what a worker builds and nothing re-encodes: list deltas in the posting format, dictionary
entries, statement records, catalog entries, versionless entries, the commit record, and the conflict keys of its
writes. It is a section of a Round frame unchanged.

**A result batch is columnar and dictionary-encoded.** A cell is a u32 index into the batch's term table, `u32::MAX`
for an unbound variable; the term table holds each distinct term of the batch once, in `ultragraphdb_core_terms`'
encoding (a tag byte and its payload, self-delimiting; at most 32 bytes except a vector, which is stored by value at
any length), with a text section for IRIs and for literals the store holds by hash. The Rust client reads a cell as `core_terms`'
`TermView<'a>`; `PROTOCOL.md` §10 states the bytes for a client in any language. A term repeated across rows is sent
once per batch, and storage and the wire share one term encoding.

**Requests name terms by text, in a term table of their own** (decided 2026-10-27). A client knows a term only by its
text: an IRI by its IRI, a blank node by its label, a literal by its lexical form and datatype or language. The store's
encoding names IRIs, blank nodes and literals of datatypes it does not know by UIDs only its dictionary holds, and has no
form for a triple term. So a request body carries a table of the terms it names, each once: a record of 24 bytes per
term, kind (IRI, blank node, literal, language-tagged literal, triple term), base direction, and two offset-and-length
pairs into the body's text section. A triple term's record names its subject, predicate and object by the indices of
earlier entries, so the table has no cycle, and nests at most 32 deep, a bound checked in the same pass as the indices.
Texts follow each other in table order, `xsd:string` is never stated, and no entry repeats another, so a term has one
encoding. A receiver reads a term in place, as `&str` slices of the text section checked for UTF-8 once, with no parser
and no escape grammar. Every term a Read or Prepare pattern, an Execute's parameters, a Write's changes and an
Operation's arguments name is an index into that table. Results differ because the server holds the store's encoding
and the dictionary's text both, and sends each. The server session converts request terms, Write, Describe and
Operation bodies to the mapping crate's and oxrdf's types; the wire crate depends on `ultragraphdb_core_terms` alone
among the term crates.

### Multiplexing, flow control, cancellation

- **Streams:** each request takes a `stream` id; responses arrive in any order, as CQL v5's stream ids allow
  (`native_protocol_v5.spec`, 16-bit ids, 32,768 per connection).
- **Credit:** a result stream sends batches while the client has credit, and a `Credit` frame grants more, as Bolt's
  `PULL n` and HTTP/2's `WINDOW_UPDATE` (RFC 9113) do; the server holds no unbounded output.
- **Cancel:** in-band, by stream, as ClickHouse's `Cancel` packet; PostgreSQL's separate connection is not copied.
- **Limits** from Welcome: the largest frame in bytes (`max_frame`), the streams in flight per connection
  (`max_streams`), each result stream's initial credit in frames (`initial_credit`), the largest write set and a
  transaction's time limit (`PROTOCOL.md` §5.2).
- **Buffers:** each core holds a pool of frame buffers sized at start, as TigerBeetle's `MessagePool`; a frame is read
  header first, then body into the same buffer, as Iggy's `read_message` does.

### Prepared statements in stage 5

The query languages arrive in stage 6, so stage 5's statements are quad patterns with parameter slots. The frames, the
statement cache per session, parameter binding, the schema id that tells a client its statement is stale after a
catalog change, and the conformance cases are built in stage 5; stage 6 compiles SPARQL and GQL into the same
statements.

### Conformance suite

- **Golden frames:** every command, valid, as committed byte files with the values they decode to.
- **Malformed frames:** a case per validation rule; every truncation of every golden frame; bit flips in header and
  body; lengths and offsets past the end, overlapping or out of order; wrong cluster, unknown command, nonzero reserved
  bytes; a corrupted chunk, two chunks swapped, a corrupted chunk table, each named by the chunk it breaks. Each yields
  its `ProtocolError`, never a panic.
- **Conversations:** handshake, multiplexed streams, credit exhaustion, cancellation mid-stream, session retries, as
  Gherkin scenarios on the repository's cucumber harness.
- **Property tests:** every builder's output round-trips through its view.
- The suite runs against the client and the server alike, and in the simulator with the network's faults.

### Posting lists without protobuf

`PostingList`, `Posting`, `UidPack`, `UidBlock` and `List` become the repository's own types, all in one crate,
`ultragraphdb_core_codec`, with their byte format and their views. It is the lowest crate that names them: it encodes
the UID pack today and depends on nothing but `ultragraphdb_proto_pb`, which goes; `core_algo` and `store_posting`
already depend on it, and `store_quads`, `store_apply` and `store_hnsw` take it as a direct dependency in place of
`ultragraphdb_proto_pb`. The posting codec moves there from `store_posting/src/utils/posting_codec.rs`, so the format
is defined and checked in one place. It is versioned and checksummed, which also gives the simulator's corruption
workloads the checksum its README asks for ("Where detection grows next"). Two steps:

1. **Own types, own encoding,** decoded as today; every suite on them. This removes `ultragraphdb_proto_pb`.
2. **Views over the stored bytes,** so a cache miss reads a list in place and a cache hit stops cloning it whole
   (`../../notes/perf-optimization.md`, "Allocation still on the path"). The `Store` trait's `Item` copies key and value into
   `Vec<u8>` today; a view needs it to hand out fjall's shared slices. Measured on a read-heavy workload before it is
   built, since the stage-4 workload reads no lists.

`core_types`' `object_value`, `get_geo_tokens`, `match_geo` and `sort_with_facet`, and `core_tok`'s
`get_index_factory_from_spec`, are Dgraph query-layer functions no crate calls; they are removed with the user's
approval, or retyped.

## Verification

| What | How |
|---|---|
| The round protocol, formally | A model of rounds, exact validation, commit-time operations, the watermark, the WAL's sync, crash and recovery, and sessions, checked for strict serializability of acknowledged commits, no acknowledged commit lost across a crash, idempotent replay and exactly-once commits per session. Written in Lean 4, which MGQL already brings, so each property is proved for every number of transactions, rounds and crashes rather than checked on small ones; kept in the repository's `lean/` folder and built with `lake` outside Bazel, as MGQL's Lean runs, and the simulator's invariants restate the model's theorems over its traces (decided 2026-10-27) |
| Strict serializability, black box | Elle's list-append and read-write register workloads through the client, with real-time edges (PVLDB 14(3), 2020) |
| Strict serializability, white box | A checker over the log: every read's version and every commit's version are known to the engine, so the serialization graph with real-time edges is checked exactly, in the manner of Emme (EuroSys 2024) |
| No acknowledged commit lost | The node simulator: workers, commit step, log over simulated storage (torn, misdirected and lost writes, a sync that reports success and loses data), `ultragraphdb_store_sim`, a network that drops, delays, duplicates and splits frames, and a stepped clock; after every crash and power loss, every acknowledged commit is present and the store equals the replay of the log |
| No corruption | Every log record and posting list checksummed; the simulator's corruption faults must surface as errors, never as data |
| Kills on the real binary | SIGKILL during rounds, syncs and checkpoints, then recovery and the checks above |
| Protocol | The conformance suite, under the network faults |

Stage 5.5 runs the same checks under every fault class, as the plan's stage table sets out.

## Measurements at the exit

The plan's exit row, with:

- the stage-4 workload under `Durability::Filesystem` as the baseline, then under stage 5's log;
- the sync cost on the test machine: with 16 KB direct writes, `fsync` took from 1.6 µs on a Samsung PM9a3 with
  power-loss protection to about 3 ms on a Samsung 990 Pro without it (Small Datum, January 2026), so the round adapts
  and the numbers name the drive. The benchmarks and the DeepBrain MVP run on drives with power-loss protection; other
  deployments may not, so the design assumes none and a configuration flag states it (D6). Power-loss protection makes
  a sync fast and never makes one optional: every round is synced before it is acknowledged on any drive, and the
  numbers are taken on a drive without it as well, to show the slowest case and to set the flag's defaults;
- the commit step's cost per transaction against the execution cost divided by n−1;
- abort rates on the Zipf deduplication workload at 8 writers, with match-then-insert and with insert-if-absent;
- the indexer's lag behind commits on a vector-heavy workload.

## Where this departs from the plan

| The plan | Here | Why |
|---|---|---|
| Validation counts predecessors that abort | Exact validation (D3) | Single-node evidence; decisions are logged |
| A pessimistic per-key queue for hot keys | Commit-time operations (D4) | Interactive transactions would hold the queue across round trips |
| The Aria fallback | Not built; reordering within a round measured first (D6) | Aria needs one-shot transactions |
| Read summaries in the commit step | Measured offline (D10) | One node validates exact sets in memory |
| "Epoch" | "Round" (D5) | The word is taken twice |
| Durability in fjall's batch | The log (D1) | The stage-10 shape, built once |
| Vector index inside the transaction | Indexer in commit order (D8) | Every concurrent insert would conflict |
| Statement-level read records | Need a change to the posting crate (D3) | The code records whole lists |
| Removing protobuf removes an API | It replaces the posting lists' storage format | The code stores prost messages |

The plan's "The write path on n−1 cores", "What a conflict is checked on" and stage row 5 take these decisions when
the plan is next revised.

## Decisions

Approved on 2026-10-27:

- D3: exact validation in place of counting aborted predecessors.
- D4: commit-time operations in place of the per-key queue; the Aria fallback dropped.
- D5: versions follow the clock; rollups at odd versions above the commits they fold.
- D8: the vector index maintained by an indexer in commit order, searches checked against the snapshot.
- D9: sessions through the log, one commit in flight per session.
- The crates and their folders as listed, with `compio` and `xxhash-rust` as new dependencies.
- Removing `core_types`' and `core_tok`'s unused Dgraph query functions.
- XXH3-64 as the checksum, in place of the XXH3-128 first proposed, so native clients implement one specified hash.
- D1: the log is the durability point in stage 5 (A), not fjall's batch (B), and its layout on disk ("The WAL, after
  Apache Iggy's").
- The header with a 64-bit cluster identity and XXH3-64 checksums, chunked body checksums under one root, the 1 MiB
  frame, and the own layout read with `from_le_bytes`.
- D2: the ordered-log interface.
- D6: rounds closed by the previous sync; reordering within a round built only if measured to pay.
- D7: catalog changes, drops and index builds ordered in rounds.
- D10: read summaries measured in stage 5, not used.
- The bodies, multiplexing, credit, cancellation, prepared statements over quad patterns, and the conformance suite.
- The posting lists' own format in two steps, its types, byte format and views in `ultragraphdb_core_codec` alone.
- The OTLP exporter keeps gRPC through tonic.
- The server binary and its crates in the group folder `../../../ultragraphdb_server`.
- TLS in stage 7, with access control.
- A formal model of the round protocol in stage 5, in Lean 4.
- No assumption of power-loss protection: the benchmarks' and the DeepBrain MVP's drives have it, other deployments
  may not. A configuration flag states it, `false` by default, and tunes round bounds and checkpoints only (D6).

## Open questions

None remain; every decision above was taken on 2026-10-27.
