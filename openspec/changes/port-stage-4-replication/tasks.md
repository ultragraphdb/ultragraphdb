Every task group runs in the same order: write the feature scenarios and port the Go tests (they fail), port the
code (they pass), refactor with both green. Multi-process scenarios are integration tests on the fault harness.

## 1. Log storage → `ultragraphdb_raft_storage`

- [ ] 1.1 Add `raft` (raft-rs); port `raftwal/storage_test.go` (11) and `encryption_test.go`'s format-independent cases; scenarios: raft-log-storage
- [ ] 1.2 Implement raft-rs `Storage` over fjall: entries, terms, first and last index, hard state, configuration, snapshot metadata, compaction
- [ ] 1.3 The node identifiers; the two durability modes; the kill-during-append test
- [ ] 1.4 The entry payload codec (`key ‖ pb.Proposal`, `pb.ZeroProposal`), proposal key generation, the log dump tool

## 2. Transport → `ultragraphdb_raft_transport`

- [ ] 2.1 Port `conn/node_test.go`; scenarios: raft-transport
- [ ] 2.2 Port the per-peer batched stream, the first-batch Raft context, dropping on a full queue, unreachable reports
- [ ] 2.3 Port connection pools, heartbeats, health, `IsPeer`, `JoinCluster` endpoints

## 3. Raft node core → `ultragraphdb_raft_node`

- [ ] 3.1 Scenarios: group-replication (election, leadership reads)
- [ ] 3.2 The Raft thread pool, batched mailboxes, the Ready handling order, the step-function loop, the leadership atomics
- [ ] 3.3 Configuration changes and their waiters

## 4. The Alpha group → `ultragraphdb_server_alpha`

- [ ] 4.1 Port `draft_test.go`, `proposal_test.go`; scenarios: group-replication (quorum, determinism, proposals, restarts, leased timestamps)
- [ ] 4.2 Proposals: keys, retries carrying the same bytes, the limiter, schema proposals' single attempt
- [ ] 4.3 Entries carrying commit timestamps in append order, blank-node uids and watermark entries; the apply thread's order check; abandonment reported after the new leader's first own-term entry; Zero refusing leases to non-leaders
- [ ] 4.4 The apply dispatch over committed entries (mutations, schema, drops, watermarks), feeding stage 3's apply thread; the applied index and applied proposal keys in the store batch
- [ ] 4.5 Reads on every replica gated by the applied watermark; restart from snapshot and log, skipping entries at or below the stored applied index

## 5. Snapshots → `ultragraphdb_server_alpha`

- [ ] 5.1 Port `TestCalculateSnapshot`; features from `worker/snapshot_test.go` and `systest/integration2/snapshot_test.go`; scenarios: group-snapshots
- [ ] 5.2 Port the snapshot thresholds, `calculateSnapshot`, the proposal, the discard timestamp
- [ ] 5.3 Port the snapshot stream: sender waits for its timestamp, full and incremental streams, the acknowledgement, stale-predicate cleanup, retries

## 6. Zero as a Raft group → `ultragraphdb_server_zero`

- [ ] 6.1 Port `zero_test.go`'s replication cases; scenarios: zero-replication
- [ ] 6.2 Port Zero's state machine: cluster ID, Raft IDs, members, leases, the done watermark, Zero snapshots
- [ ] 6.3 The leader-only services with quorum contact, read index for linearizable state, the watermark stream across leader changes

## 7. Membership → `ultragraphdb_server_alpha`, `ultragraphdb_server_zero`

- [ ] 7.1 Features from `systest/integration2/zero_addr_test.go` and `systest/group-delete`; scenarios: group-membership
- [ ] 7.2 Port joining, learners, removal through Zero, the membership stream and its counter, membership reports, leadership transfer at shutdown

## 8. Fault harness

- [ ] 8.1 The process harness: start, kill, restart, partition and slow links by proxy; history recording in Elle's format
- [ ] 8.2 Scenarios: fault-tolerance; Dgraph's Jepsen schedules (kill-alpha, kill-zero, partition-ring)
- [ ] 8.3 `elle-cli` runs under each schedule: `strict-serializable` for the read-set and upsert-block forms, `snapshot-isolation` for write-key requests, tagged `integration_test`

## 9. Close the stage

- [ ] 9.1 No scenario of this stage pending in the traceability test; `scripts/bdd_counts.sh` reports equal counts; `make test` green; the fault schedules pass
- [ ] 9.2 Measure commits per second for a three-replica group against stage 3's single replica, against Go Dgraph v25.5.0 stock (11.8k) and patched (14.5k) with one Zero and one Alpha, and against Go Dgraph with three Alpha replicas measured the same way; results in the overview
- [ ] 9.3 Attribution headers; `AGENTS.md` crate table updated
