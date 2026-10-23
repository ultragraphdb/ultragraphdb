Every task group runs in the same order: write the feature scenarios and port the Go tests (they fail), port the
code (they pass), refactor with both green. Multi-group scenarios are integration tests on stage 4's harness, with
two groups of three Alphas and three Zeros, as Dgraph's default test cluster has.

## 1. Tablets → `ultragraphdb_cluster_tablets`, `ultragraphdb_server_zero`, `ultragraphdb_server_alpha`

- [ ] 1.1 Scenarios: tablets; Dgraph has no unit tests of tablet assignment to port
- [ ] 1.2 Port Zero's tablet map: `ShouldServe`, `Inform` answering with the owner's tablet, reserved predicates in group 1, the race rule, read-only lookups, the delete instruction for unserved tablets
- [ ] 1.3 Port the Alpha's tablet cache, `BelongsTo`, `BelongsToReadOnly`, the ownership check at apply, the checksum gate
- [ ] 1.4 Tablet size reports summed per predicate prefix by the store; decide the method, since fjall reports disk space per keyspace only
- [ ] 1.5 Add `ultragraphdb_cluster/*`, `ultragraphdb_ops/*` and `ultragraphdb_tool/*` to the root `Cargo.toml`'s members

## 2. Fan-out → `ultragraphdb_server_alpha`, `ultragraphdb_query_task`

- [ ] 2.1 Scenarios: cross-group-queries; the query suites on two groups
- [ ] 2.2 Port `ProcessTaskOverNetwork`, hedged requests, `ServeTask`, and fan-out for sort and schema requests

## 3. Admin interface → `ultragraphdb_server_alpha`

- [ ] 3.1 Decide the admin transport (design, open question 3); scenarios: admin-operations
- [ ] 3.2 The superadmin-only task interface over Dgraph's task queue, with status queries

## 4. Cross-group transactions → `ultragraphdb_server_zero`, `ultragraphdb_server_alpha`

- [ ] 4.1 Decide the mechanism and the scope of cross-group upserts (design, open questions 1 and 2); record them in the design and the overview
- [ ] 4.2 Scenarios: cross-group-transactions
- [ ] 4.3 Implement the chosen mechanism with per-group watermarks for reads, stale-owner aborts and the blocked-tablet aborts
- [ ] 4.4 Elle runs across groups under stage 4's fault schedules

## 5. Moves → `ultragraphdb_cluster_move`

- [ ] 5.1 Scenarios: predicate-moves; a small move under load, written fresh
- [ ] 5.2 Port the commit block, the move timestamp, the source stream rolled up at it, clean-then-ingest, forced reassignment, checksum-gated cleanup
- [ ] 5.3 The move through the admin interface; refusals; queued moves; the move timestamp kept across size reports; the multi-gigabyte move with a killed destination

## 6. Rebalancing → `ultragraphdb_cluster_tablets`

- [ ] 6.1 Port `tablet_test.go`'s timeout, cooldown, backoff and disabled cases; scenarios: rebalancing
- [ ] 6.2 Port the rebalancer, the interval flag and the backoff; the move-tablet nemesis in the fault schedules

## 7. Input readers and xidmap → `ultragraphdb_core_chunker`, `ultragraphdb_tool_xidmap`

- [ ] 7.1 Port `chunk_test.go` and `xidmap/trie_test.go`, `xidmap_test.go`
- [ ] 7.2 Port the RDF and JSON chunkers over plain and gzipped files and streams; the xidmap with leases and fjall persistence

## 8. Live loader → `ultragraphdb_tool_live`

- [ ] 8.1 Features from `dgraph/cmd/live/load-uids` and `load-json`, `systest/bulk_live/live`; scenarios: live-loader
- [ ] 8.2 Port batching, the client-side conflict keys and hold-back, retries with backoff, uid handling, `--new_uids`, `--upsertPredicate`

## 9. Export → `ultragraphdb_ops_export`

- [ ] 9.1 Port `worker/export_test.go`; features from `systest/export`; scenarios: export
- [ ] 9.2 Port the export at one timestamp, RDF, JSON and schema files, gzip, file and S3 destinations; no GraphQL schema file

## 10. Backup and restore → `ultragraphdb_ops_backup`

- [ ] 10.1 Scenarios: backup-restore; features from the chain, incremental, drop and online-restore cases of `systest/backup` and `systest/online-restore`, rewritten for the port's format
- [ ] 10.2 The part format: header, LZ4 frames (`lz4_flex`) with CRC32C, records, trailer; a reader that fails on the first damaged frame, record or out-of-order key, with tests over truncation, bit flips and reordering
- [ ] 10.3 Backups: one read timestamp across groups, full and incremental content with `delete` records, the UTF-8 check of every string value, parts of at most 256 MiB, the manifest written last and atomically, removal of an unpublished backup's leftovers, file and S3 destinations
- [ ] 10.4 The tombstone horizon: set stage 2's horizon to the latest published backup's read timestamp, so its deletion sweep keeps every deletion newer than it
- [ ] 10.5 Restore: chain checks, the per-group merge with drops and deletes, sorted ingestion into staged stores, the switch on success (directory rename offline, all groups at once online), the free-space check, lease bumps for both; refusal of foreign manifests and unknown formats
- [ ] 10.6 `ultragraph backup verify`

## 11. Bulk loader and import → `ultragraphdb_tool_bulk`

- [ ] 11.1 Port `merge_shards_test.go`, `placement_test.go`, `shard_map_test.go`; features from `systest/bulk_live/bulk`, `systest/integration2/bulk_loader_test.go`, `dgraphimport`; scenarios: bulk-loader
- [ ] 11.2 Port the map, shuffle and reduce phases writing fjall stores by sorted ingestion; placement; split lists, counts and vectors
- [ ] 11.3 Port import into a running cluster: draining, replacing each group's store by streaming to its replicas, deleting all data on failure
- [ ] 11.4 Ledger 28: the bulk loader leases namespace identifiers from Zero's namespace counter, not with `AllocateTimestamps`

## 12. Close the stage and the port

- [ ] 12.1 Every scenario of this stage has its feature; `scripts/bdd_counts.sh` reports equal counts; `make test` green; the fault schedules with moves pass
- [ ] 12.2 Load Go Dgraph's export of scrith's data and run scrith's suites against ultragraphdb
- [ ] 12.3 Measure commits per second with one, two and three groups against Go Dgraph's measured 8–10% gain from three groups; results in the overview
- [ ] 12.4 Attribution headers; `AGENTS.md` crate table updated
