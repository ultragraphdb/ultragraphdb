## Why

Distribution is a requirement, and replication comes before sharding: a group must survive the loss of a replica
before predicates are spread across groups. Stage 4 replicates the single group of stage 3, and Zero, as Raft
groups of three, with raft-rs in place of etcd raft and the loop model the overview fixes: a fixed pool of
threads driving Raft groups, one apply thread per group, tonic at the network edge.

## What Changes

- Raft log storage on fjall implementing raft-rs's `Storage`: entries, hard state, configuration, snapshot
  metadata, compaction, and Dgraph's per-node identifiers and checkpoint.
- The Raft transport over tonic: batched message streams per peer, connection pools, health heartbeats,
  unreachable reports, `JoinCluster` and `IsPeer`.
- The Alpha's Raft node ported from `worker/draft.go` and `proposal.go`: the Ready loop, proposals with retries,
  deduplication and the pending-proposal limit, the apply thread fed from committed entries, checkpoints.
- Snapshots ported from `draft.go` and `snapshot.go`: their timestamp, their proposal, and the stream that brings a
  lagging replica up to date, in full or since its last snapshot.
- Membership: joining, learners, removal through Zero, leadership transfer at shutdown.
- Zero as a Raft group: membership state, leases, the done watermark and the stream to Alphas replicated.
- Leadership read from an atomic the loop publishes, in place of Dgraph's two `Status()` round trips per check,
  for Zero and the Alpha.
- A fault-injection harness: killed processes, partitions, slow links, with Elle checking histories under faults.

## Capabilities

### New Capabilities

- `raft-log-storage`: the Raft log, hard state and snapshot metadata on fjall.
- `raft-transport`: messages between replicas, connections and their health.
- `group-replication`: a group of replicas committing, applying and recovering.
- `group-snapshots`: snapshots and bringing a lagging replica up to date.
- `group-membership`: replicas joining, leaving and handing over leadership.
- `zero-replication`: Zero's state, leases and watermark across Zero replicas.
- `fault-tolerance`: behaviour under crashes and partitions, and the harness that checks it.

### Modified Capabilities

None.

## Impact

- New crates under `ugdb_raft/`; `raft` (raft-rs) joins `[workspace.dependencies]`.
- `server_alpha` and `server_zero` run their state through Raft; the apply thread takes committed entries
  instead of requests.
- Leased commit timestamps travel in the Raft entries, so every replica applies the same timestamps.
