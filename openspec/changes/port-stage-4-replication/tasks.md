Every task group runs in the same order: write the feature scenarios and port the Go tests (they fail), port the
code (they pass), refactor with both green. Multi-process scenarios are integration tests on the fault harness.

## 1. Log storage → `raft_storage`

- [ ] 1.1 Add `raft` (raft-rs); port `raftwal/storage_test.go` (11) and `encryption_test.go`'s format-independent cases; scenarios: raft-log-storage
- [ ] 1.2 Implement raft-rs `Storage` over fjall: entries, terms, first and last index, hard state, configuration, snapshot metadata, compaction
- [ ] 1.3 The node identifiers and checkpoint; the two durability modes; the kill-during-append test
- [ ] 1.4 The entry payload codec (`key ‖ pb.Proposal`, `pb.ZeroProposal`), proposal key generation, the log dump tool

## 2. Transport → `raft_transport`

- [ ] 2.1 Port `conn/node_test.go`; scenarios: raft-transport
- [ ] 2.2 Port the per-peer batched stream, the first-batch Raft context, dropping on a full queue, unreachable reports
- [ ] 2.3 Port connection pools, heartbeats, health, `IsPeer`, `JoinCluster` endpoints

## 3. Raft node core → `raft_node`

- [ ] 3.1 Scenarios: group-replication (election, leadership reads)
- [ ] 3.2 The Raft thread pool, batched mailboxes, the Ready handling order, the step-function loop, the leadership atomics
- [ ] 3.3 Configuration changes and their waiters

## 4. The Alpha group → `server_alpha`

- [ ] 4.1 Port `draft_test.go`, `proposal_test.go`; scenarios: group-replication (quorum, determinism, proposals, restarts, leased timestamps)
- [ ] 4.2 Proposals: keys, retries, the limiter, deduplication; leased commit timestamps in the entries
- [ ] 4.3 The apply dispatch over committed entries (mutations, schema, drops, timestamp reports), feeding stage 3's apply thread; checkpoints
- [ ] 4.4 Restart from snapshot and log, skipping entries at or below the checkpoint

## 5. Snapshots → `server_alpha`

- [ ] 5.1 Port `TestCalculateSnapshot`; features from `worker/snapshot_test.go` and `systest/integration2/snapshot_test.go`; scenarios: group-snapshots
- [ ] 5.2 Port the snapshot thresholds, `calculateSnapshot`, the proposal, the discard timestamp
- [ ] 5.3 Port the snapshot stream: sender waits for its timestamp, full and incremental streams, the acknowledgement, stale-predicate cleanup, retries

## 6. Zero as a Raft group → `server_zero`

- [ ] 6.1 Port `zero_test.go`'s replication cases; scenarios: zero-replication
- [ ] 6.2 Port Zero's state machine: cluster ID, Raft IDs, members, leases, the done watermark, Zero snapshots
- [ ] 6.3 The leader-only services with quorum contact, read index for linearizable state, the watermark stream across leader changes

## 7. Membership → `server_alpha`, `server_zero`

- [ ] 7.1 Features from `systest/integration2/zero_addr_test.go` and `systest/group-delete`; scenarios: group-membership
- [ ] 7.2 Port joining, learners, removal through Zero, the membership stream and its counter, membership reports, leadership transfer at shutdown

## 8. Fault harness

- [ ] 8.1 The process harness: start, kill, restart, partition and slow links by proxy; history recording in Elle's format
- [ ] 8.2 Scenarios: fault-tolerance; Dgraph's Jepsen schedules (kill-alpha, kill-zero, partition-ring)
- [ ] 8.3 Elle runs of both transaction forms under each schedule, tagged `integration_test`

## 9. Close the stage

- [ ] 9.1 Traceability green; `make test` green; the fault schedules pass
- [ ] 9.2 Measure commits per second for a three-replica group against stage 3's single replica, and against Go Dgraph's measured 11.8k and 14.5k; results in the overview
- [ ] 9.3 Attribution headers; `AGENTS.md` crate table updated
