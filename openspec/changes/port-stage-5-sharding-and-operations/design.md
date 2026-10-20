## Context

Stage 5 spreads stage 4's replicated groups across predicates and completes the port. The Go code is Zero's
`tablet.go` and the tablet half of `zero.go`; `worker/groups.go`'s ownership; `predicate_move.go`; `task.go`'s
fan-out (`ProcessTaskOverNetwork`, `processWithBackupRequest`, `ServeTask`); the cross-group parts of
`worker/mutation.go`; `backup*.go`, `backup/`, `restore_map.go`, `restore_reduce.go`, `online_restore.go`;
`export.go`; `dgraph/cmd/bulk`, `live`, `dgraphimport`, `worker/import.go`; `chunker`'s readers; `xidmap`.

In Dgraph a commit-now transaction spanning groups is proposed to each group, held pending there, decided by Zero
(conflict keys, owner checks, the blocked-tablet check during moves, `oracle.go:341-404`), and applied by each
group when Zero's delta reaches it (`groups.go:926`). Stage 3 moved the conflict check into the group, because one
group holds every key of its predicates. A transaction spanning groups has no such single holder.

Backup, restore and export are started through GraphQL `/admin` mutations in Dgraph (`graphql/admin/backup.go`,
`restore.go`, `export.go`), which the port cuts.

## Goals / Non-Goals

**Goals:**

- Dgraph's tablet lifecycle, moves and rebalancing.
- Queries over several groups with the results of one.
- Transactions spanning groups, all or nothing, under both transaction forms and the write-key check.
- Export, backup, restore and the loaders in Dgraph's formats, so data moves in from Go Dgraph.
- An admin interface in place of GraphQL `/admin`.

**Non-Goals:**

- Encryption at rest and of backups; cut from the port.
- Change data capture; cut.
- Re-architecting predicate sharding, the measured write-rate bound per predicate; step 5 of the plan.

## Decisions

### Crates

| Crate | Path | Ports |
|---|---|---|
| `cluster_tablets` | `ugdb_cluster/cluster_tablets` | Zero's tablet map, assignment, size accounting, the rebalancer and backoff |
| `cluster_move` | `ugdb_cluster/cluster_move` | `predicate_move.go` and Zero's `movePredicate` |
| `ops_export` | `ugdb_ops/ops_export` | `export.go` |
| `ops_backup` | `ugdb_ops/ops_backup` | `backup*.go`, `backup/`, the restore map and reduce, online and offline restore |
| `tool_xidmap` | `ugdb_tools/tool_xidmap` | `xidmap`, persisted in fjall |
| `tool_live` | `ugdb_tools/tool_live` | `dgraph/cmd/live` over `ultragraph_client`, as the `ultragraph live` subcommand |
| `tool_bulk` | `ugdb_tools/tool_bulk` | `dgraph/cmd/bulk`, `dgraphimport`, `worker/import.go`, as the `ultragraph bulk` and `ultragraph import` subcommands; `ultragraph restore` for offline restores |

The chunker's file and stream readers (gzip) join `core_chunker`.

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

### Formats as Dgraph writes them

Export files, backup files, manifests and the restore chain rules follow Dgraph, so Go Dgraph's exports and
unencrypted backups load into ultragraphdb. The bulk loader writes fjall stores by sorted batch ingestion per
keyspace in place of Badger's stream writer.

## Risks / Trade-offs

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

Data moves from Go Dgraph by export or unencrypted backup into ultragraphdb, or by bulk-loading its export.

## Open Questions

- Which mechanism commits a transaction spanning groups: Zero ordering the whole request, or Dgraph's prepare
  and decide?
- Does an upsert block spanning groups run under that mechanism, or is a serially executed upsert confined to one
  group?
- Is the admin interface a set of HTTP endpoints on the Alpha, or a gRPC admin service beside `Dgraph`?
