> **Superseded, 2026-10-25, not implemented.** DQL was replaced by SPARQL and GQL, and the roadmap was renumbered to
> nine stages. This change is closed unapplied: its requirements were not synced into `openspec/specs/`. Its work
> moves to stage 9 (distributed tiles), which replicates the ordered log with VSR; Raft is not built, specified anew from `openspec/notes/UltraGraphDB Dual Query Engine Plan.md`.

## Why

Distribution is a requirement, and replication comes before sharding: a group must survive the loss of a replica
before predicates are spread across groups. Stage 6 replicates the single group of stage 5, and Zero, as Raft
groups of three, with raft-rs in place of etcd raft and the loop model the overview fixes: a fixed pool of
threads driving Raft groups, one apply thread per group, tonic at the network edge.

## What Changes

- Raft log storage implementing raft-rs's `Storage` in an append-only log of its own: segments of framed, checksummed
  records and a two-copy superblock for hard state, configuration, snapshot metadata and Dgraph's per-node
  identifiers. Its file I/O runs through an asynchronous storage trait, on compio (io_uring on Linux) in production and
  on a simulated implementation that injects torn writes, crashes and power loss.
- A deterministic simulator running a group and its Zero in one process on a seeded executor, over the simulated log
  storage, network and clock, checked against a shadow model after each step.
- The Raft transport over tonic: batched message streams per peer, connection pools, health heartbeats,
  unreachable reports, `JoinCluster` and `IsPeer`.
- The Alpha's Raft node ported from `worker/draft.go` and `proposal.go`: the Ready loop, proposals with retries,
  deduplication and the pending-proposal limit, the apply thread fed from committed entries.
- Commit timestamps, blank-node uids and Zero's done watermark carried in the entries, so every replica applies the
  same values; the applied index and applied proposal keys kept in the store's batches.
- Snapshots ported from `draft.go` and `snapshot.go`: their timestamp, their proposal, and the stream that brings a
  lagging replica up to date, in full or since its last snapshot.
- Membership: joining, learners, removal through Zero, leadership transfer at shutdown.
- Zero as a Raft group: membership state, the leases' high-water marks and the cluster ID replicated.
- Leadership read from an atomic the loop publishes, in place of Dgraph's two `Status()` round trips per check,
  for Zero and the Alpha.
- A fault-injection harness: killed processes, partitions, slow links, with Elle checking histories under faults.

## Capabilities

### New Capabilities

- `raft-log-storage`: the Raft log, hard state and snapshot metadata in a log of its own, over compio.
- `raft-transport`: messages between replicas, connections and their health.
- `group-replication`: a group of replicas committing, applying and recovering.
- `group-snapshots`: snapshots and bringing a lagging replica up to date.
- `group-membership`: replicas joining, leaving and handing over leadership.
- `zero-replication`: Zero's state, leases and watermark across Zero replicas.
- `fault-tolerance`: behaviour under crashes and partitions, and the harness that checks it.

### Modified Capabilities

None.

## Impact

- New crates `ultragraphdb_raft_*`; `raft` (raft-rs) and `compio` join `[workspace.dependencies]`.
- `ultragraphdb_server_alpha` and `ultragraphdb_server_zero` run their state through Raft; the apply thread takes committed entries
  instead of requests.
- Commit timestamps, uids and the watermark travel in the Raft entries; Zero refuses commit leases to non-leaders.
