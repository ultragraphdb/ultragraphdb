## Why

A replicated group carries only what one machine holds. Stage 5 spreads predicates across several groups, as
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
- Full and incremental backups to the filesystem and S3, manifests, restore offline and online.
- The live loader over one-request transactions, the bulk loader writing per-group fjall stores, and import into
  a running cluster.
- **Behaviour change:** backups are unencrypted, because encryption at rest is cut; an encrypted Go backup is
  refused with an error that says so.

## Capabilities

### New Capabilities

- `tablets`: which group serves a predicate and how that is decided and checked.
- `cross-group-queries`: answering a query whose predicates live in several groups.
- `cross-group-transactions`: committing a transaction whose writes span groups.
- `predicate-moves`: moving a predicate from one group to another.
- `rebalancing`: choosing what to move, when, and backing off after failures.
- `admin-operations`: the superadmin interface for export, backup, restore, listing backups and moves.
- `export`: RDF, JSON and schema exports.
- `backup-restore`: full and incremental backups, manifests, and restores.
- `live-loader`: loading data through the client API.
- `bulk-loader`: building group stores offline and importing them.

### Modified Capabilities

None.

## Impact

- New crates under `ugdb_cluster/`, `ugdb_ops/` and `ugdb_tools/`; S3 access (`object_store`) and gzip join the
  dependencies.
- `server_zero` gains tablets and the rebalancer; `server_alpha` gains fan-out, moves and the admin interface.
- The chunker's file and stream readers, deferred from stage 1, arrive with the loaders.
