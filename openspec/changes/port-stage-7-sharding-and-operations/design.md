## Context

Stage 7 spreads stage 6's replicated groups across predicates and completes the port. The Go code is Zero's
`tablet.go` and the tablet half of `zero.go`; `worker/groups.go`'s ownership; `predicate_move.go`; `task.go`'s
fan-out (`ProcessTaskOverNetwork`, `processWithBackupRequest`, `ServeTask`); the cross-group parts of
`worker/mutation.go`; `backup*.go`, `backup/`, `restore_map.go`, `restore_reduce.go`, `online_restore.go`;
`export.go`; `dgraph/cmd/bulk`, `live`, `dgraphimport`, `worker/import.go`; `chunker`'s readers; `xidmap`.

In Dgraph a commit-now transaction spanning groups is proposed to each group, held pending there, decided by Zero
(conflict keys, owner checks, the blocked-tablet check during moves, `oracle.go:341-404`), and applied by each
group when Zero's delta reaches it (`groups.go:926`). Stage 5 moved the conflict check into the group, because one
group holds every key of its predicates. A transaction spanning groups has no such single holder.

Backup, restore and export are started through GraphQL `/admin` mutations in Dgraph (`graphql/admin/backup.go`,
`restore.go`, `export.go`), which the port cuts.

## Goals / Non-Goals

**Goals:**

- Dgraph's tablet lifecycle, moves and rebalancing.
- Queries over several groups with the results of one.
- Transactions spanning groups, all or nothing, under both transaction forms and the write-key check.
- Export and the loaders in Dgraph's formats, so data moves in from Go Dgraph; backup and restore for the port's own
  backups.
- An admin interface in place of GraphQL `/admin`.

**Non-Goals:**

- Encryption at rest and of backups; cut from the port.
- Change data capture; cut.
- Re-architecting predicate sharding, the measured write-rate bound per predicate; step 5 of the plan.

## Decisions

### Crates

| Crate | Path | Ports |
|---|---|---|
| `ultragraphdb_cluster_tablets` | `ultragraphdb_cluster/ultragraphdb_cluster_tablets` | Zero's tablet map, assignment, size accounting, the rebalancer and backoff |
| `ultragraphdb_cluster_move` | `ultragraphdb_cluster/ultragraphdb_cluster_move` | `predicate_move.go` and Zero's `movePredicate` |
| `ultragraphdb_ops_export` | `ultragraphdb_ops/ultragraphdb_ops_export` | `export.go` |
| `ultragraphdb_ops_backup` | `ultragraphdb_ops/ultragraphdb_ops_backup` | The port's backup format, written fresh: full and incremental backups, the chain, verification, staged online and offline restore; Dgraph's `backup*.go` and `online_restore.go` for the admin flow and the lease bumps |
| `ultragraphdb_tool_xidmap` | `ultragraphdb_tool/ultragraphdb_tool_xidmap` | `xidmap`, persisted in fjall through `ultragraphdb_store_kv` |
| `ultragraphdb_tool_live` | `ultragraphdb_tool/ultragraphdb_tool_live` | `dgraph/cmd/live` over `ultragraphdb_client`, as the `ultragraph live` subcommand |
| `ultragraphdb_tool_bulk` | `ultragraphdb_tool/ultragraphdb_tool_bulk` | `dgraph/cmd/bulk`, `dgraphimport`, `worker/import.go`, as the `ultragraph bulk` and `ultragraph import` subcommands; `ultragraph restore` for offline restores |

The chunker's file and stream readers (gzip) join `ultragraphdb_core_chunker`.

### Tablets, fan-out and moves as Dgraph has them

Ported from `tablet.go`, `groups.go` and `predicate_move.go`: first-writer assignment with
reserved predicates in group 1, read-only lookups that create nothing, ownership checks at apply and at task
time with the membership checksum gate, hedged tasks, the move protocol (commit block, move timestamp, stream
rolled up at it, clean-then-ingest at the destination, forced reassignment, checksum-gated cleanup at the source),
moves queued one at a time, and the rebalancer with its backoff. fjall 3.1 reports disk space per keyspace only, so
the store sums tablet sizes per predicate prefix in place of Badger's table infos; task 1.4 settles how.

### Transactions spanning groups

The requirement is fixed by `cross-group-transactions`; the mechanism is open (see Open Questions). Both candidates
keep Zero's role as the single orderer and keep per-group watermarks for reads:

- **Zero orders the whole request.** The request travels to Zero, which checks it (read set or write keys), assigns
  its commit timestamp and puts it in its log; each group applies its part from Zero's stream in timestamp order.
  No group holds pending writes. Writes pass through Zero's log, and Zero checks every cross-group transaction.
- **Dgraph's prepare and decide.** Each group holds the transaction's writes pending; Zero decides; each group applies
  on the decision. The uncommitted layer returns, for cross-group transactions only.

Upsert blocks spanning groups need their query to read several groups at the block's position in the order; that
follows Calvin's sequencing under the first candidate and has no counterpart under the second.

### The admin interface

Export, backup, restore, listing backups and tablet moves are started by the superadmin. Export, backup and restore
run as tasks with a status (queued, running, success, failed) on Dgraph's task queue (`worker/queue.go`), which in
Dgraph runs only backup and export; restore and moves answer synchronously there. Its transport is open (see Open
Questions).

### Export in Dgraph's formats

Export files follow Dgraph, so Go Dgraph's exports load into ultragraphdb through the live and bulk loaders, which
build every index from values. The bulk loader writes fjall stores by sorted batch ingestion per keyspace in place of
Badger's stream writer.

### Backups in the port's own format

Restore loads only backups the port wrote, so the format is the port's own, chosen to be simple to read, to fail
loudly, and to leave a backup that is either complete or invisible. Dgraph's layout is dropped: its single rewritten
`manifest.json`, its unsorted per-group files that need a map and a reduce phase to restore, its manifest upgrades
between Go releases and its encryption.

**Layout.** One directory (or key prefix in S3) per backup, under the chain's identifier:

```
<root>/<backup_id>/<backup_num, 6 digits>/g<group, 4 digits>-<part, 5 digits>.ugb
<root>/<backup_id>/<backup_num, 6 digits>/manifest.json
```

`backup_id` is a UUIDv7 drawn by the full backup and kept by its incrementals; `backup_num` counts from 1. Every object
is written once and never rewritten, so no write can corrupt an earlier backup, and S3 needs no read-modify-write.

**Publication.** The data files are written first; `manifest.json` last, through a temporary name and a rename on a
filesystem, as one PUT on S3. A backup without its manifest does not exist: listing, restore and the next incremental
ignore it, so a crash or a failed upload leaves the chain as it was. A later backup removes the leftovers of an
unpublished one under its own number before writing.

**Data files.** A group's records are written in strictly ascending key order and split into parts of at most 256 MiB,
so each part uploads and retries on its own and a restore reads them as one sorted stream.
- A part starts with a header: the magic `UGBK`, the format version (1), the group, `backup_id` and `backup_num`.
- Frames follow: each holds the uncompressed and compressed lengths, the CRC32C of the compressed bytes, and an LZ4
  block (`lz4_flex`, pure Rust) of about 4 MiB of records.
- A record is a tag (`put` or `delete`), the key, and for a put the key's posting list rolled up at the backup's read
  timestamp, each length-prefixed.
- A trailer closes the part: the magic `UGBE`, the record count and the last key.

A truncated part, a flipped bit or keys out of order fail on the frame or record where they occur.

**Manifest.** A JSON object holding:
- `writer: "ultragraphdb"`, the writer's version, and `format: 1`;
- `backup_id`, `backup_num`, `type` (`full` or `incremental`), `since_ts` (0 for a full backup) and `read_ts`;
- the drop operations applied after the previous backup, in order;
- the uid and namespace leases' maxima at `read_ts`;
- per group, its predicates and its parts, each with its byte size, record count, first and last key and SHA-256.

Restore refuses a manifest without `writer: "ultragraphdb"` or with a format it does not know.

**Content.** A backup reads every group at one read timestamp leased from Zero.
- A full backup holds every key, with its posting list rolled up at that timestamp; schema and type keys included.
- An incremental backup holds every key whose newest version at or below `read_ts` is above `since_ts`: a `put` with the
  rolled-up list, or a `delete` when that version is a tombstone.
- Before a string value is written, it is checked for valid UTF-8; a backup meeting one that is not fails, naming the
  key, and publishes nothing.

**Restore.** The chain up to backup `N` must be complete: one `backup_id`, numbers 1 to `N` without a gap, every part
present with its listed size and hash. Per group, restore merges the chain's sorted streams; for each key the newest
backup's record wins, a `delete` removes it, and a record of backup `j` is skipped when a drop recorded in a backup `m`
with `j < m ≤ N` covers its predicate or namespace. Because the merged stream is sorted, it goes straight into a fresh
fjall keyspace by sorted batch ingestion, verifying CRCs and hashes as it reads; Dgraph's map and reduce phases
have no counterpart.
- An offline restore writes a new store directory and renames it into place on success.
- An online restore writes each group's staged keyspace and switches the group to it once every group has succeeded.

A failed restore leaves the target as it was; Dgraph's online restore drops the data first. The uid and namespace
leases are raised to the manifest's maxima, online and offline alike.

**Verification.** `ultragraph backup verify` reads a chain as restore does and checks every frame, hash, count, key
order and string value without writing anything, so a backup can be proven restorable before it is needed.

Alternatives considered:
- Dgraph's layout: kept nothing the port needs once Go backups are not restored, and its restore needs map and reduce.
- Copying fjall's files: ties a backup to one storage version and file layout, has no incrementals, and cannot restore
  into another group layout.
- A general container (Parquet, SQLite): adds a large dependency for a sorted stream of byte pairs.

## Risks / Trade-offs

- [Compaction drops a tombstone before the next incremental backup reads it, and the deleted key returns on restore]
  → The store keeps every tombstone newer than the latest published backup's `read_ts` (stage 2's deletion sweep
  takes that horizon), and a restore of the chain is checked against the source in the stage's tests.
- [A restore needs room for the staged store beside the live one] → Restore checks free space against the manifest's
  sizes before it starts and refuses with the shortfall.

- [The cross-group mechanism is undecided] → `cross-group-transactions` and the upsert scenarios cannot pass until it
  is; task group 4 starts with the decision.
- [Zero ordering whole requests makes Zero the bottleneck the investigation measured] → Measured in the stage's
  benchmark against the prepare-and-decide candidate.
- [Moves of large predicates take hours] → Dgraph's move timeout (`max(2h, max(on-disk, uncompressed) / 256 KiB/s)`)
  is ported; moves are measured on a multi-gigabyte predicate, as Dgraph's `largemove` test does.
- [A failed import deletes all data] → As in Dgraph: an import replaces every group's store, and a failure drops all
  data in every group.
- [Dgraph has no small, routinely run test of a move under load] → Written fresh as features in this stage.

## Migration Plan

Data moves from Go Dgraph by export into ultragraphdb, through the live loader or by bulk-loading the export.

## Open Questions

- Which mechanism commits a transaction spanning groups: Zero ordering the whole request, or Dgraph's prepare
  and decide?
- Does an upsert block spanning groups run under that mechanism, or is a serially executed upsert confined to one
  group?
- Is the admin interface a set of HTTP endpoints on the Alpha, or a gRPC admin service beside `Dgraph`?
