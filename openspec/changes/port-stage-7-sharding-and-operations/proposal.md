## Why

A replicated group carries only what one machine holds. Stage 7 spreads predicates across several groups, as
Dgraph's tablets do, and completes the port with the operations a production deployment needs: moving and
rebalancing predicates, backups and restores, exports, and the bulk and live loaders that move data in, from Go
Dgraph included.

## What Changes

- Tablets: a predicate's group assigned by Zero on first write, reserved predicates in group 1, ownership checked
  on every read and write.
- Queries fanned out to the group serving each predicate, with hedged requests.
- Transactions spanning groups, visible in every group at once or in none.
- Predicate moves between groups, the rebalancer and its backoff, and manual moves.
- An admin interface for export, backup, restore and moves, replacing Dgraph's GraphQL `/admin` mutations.
- Export to RDF and JSON with the schema, without the GraphQL schema file.
- Full and incremental backups to the filesystem and S3 in the port's own format, verification of a chain, and staged
  restores offline and online.
- The live loader over one-request transactions, the bulk loader writing per-group fjall stores, and import into
  a running cluster.
- **Behaviour changes:**
  - Backups take the port's own format, not Dgraph's layout: per-group parts in key order with checksummed frames, one
    immutable manifest per backup written last, and every string value checked for valid UTF-8 before writing. Restore
    loads only these and refuses a Go Dgraph backup; data from Go Dgraph enters through its export and the loaders.
  - A failed restore leaves the target as it was; Dgraph's online restore drops the data first.
  - Backups are unencrypted, because encryption at rest is cut.
  - Restore runs as a task with a status, as export and backup do, and export is confined to the superadmin.
  - Offline restores raise the uid and namespace leases; Dgraph's do not.
  - A tablet keeps its move timestamp across size reports, and `Inform` answers with the owner's tablet.

## Capabilities

### New Capabilities

- `tablets`: which group serves a predicate and how that is decided and checked.
- `cross-group-queries`: answering a query whose predicates live in several groups.
- `cross-group-transactions`: committing a transaction whose writes span groups.
- `predicate-moves`: moving a predicate from one group to another.
- `rebalancing`: choosing what to move, when, and backing off after failures.
- `admin-operations`: the superadmin interface for export, backup, restore, listing backups and moves.
- `export`: RDF, JSON and schema exports.
- `backup-restore`: full and incremental backups in the port's own format, published by their manifests, chain
  verification, and staged restores.
- `live-loader`: loading data through the client API.
- `bulk-loader`: building group stores offline and importing them.

### Modified Capabilities

None.

## Impact

- New crates `ultragraphdb_cluster_*`, `ultragraphdb_ops_*` and `ultragraphdb_tool_*`; S3 access (`object_store`), gzip for exports, and
  `lz4_flex`, CRC32C and SHA-256 for backups join the dependencies, each pure Rust.
- `ultragraphdb_server_zero` gains tablets and the rebalancer; `ultragraphdb_server_alpha` gains fan-out, moves and the admin interface.
- The chunker's file and stream readers, deferred from stage 1, arrive with the loaders.
