## Context

Stage 4 replicates stage 3's group and its Zero as Raft groups of three. The Go code is `raftwal` (the WAL),
`conn` (the Raft node wrapper `node.go`, the transport `raft_server.go`, connection pools `pool.go`),
`worker/draft.go` (the Alpha's Ready loop, apply, snapshots, checkpoints), `proposal.go`, `snapshot.go`, `queue.go`,
the membership half of `groups.go`, and Zero's `raft.go`, `zero.go` and `run.go`.

Dgraph configures etcd raft with 100 ms ticks, an election after 20, heartbeats every tick, 256 messages in
flight, 256 KB per message, 64 MB of committed entries per Ready, read-only safe mode and PreVote
(`conn/node.go:91-130`). Its Raft snapshots carry metadata only: a follower behind the log fetches the leader's
store at the snapshot's timestamp as a stream before recording the snapshot (`draft.go:1405-1473`,
`snapshot.go`). Zero applies its committed entries inline and replicates membership, leases and, in Dgraph,
commit decisions (`zero/raft.go:399-488`). Leadership checks call `Status()` twice, each a round trip to the
Raft goroutine, in Zero and in the Alpha (`zero/raft.go:56-62`, `draft.go:2027-2033`).

## Goals / Non-Goals

**Goals:**

- Dgraph's replication behaviour with raft-rs, on the overview's loop model.
- Determinism: every replica applies the same entries to the same state, upsert blocks and checks included.
- Leadership checks that never wait on a Raft loop.
- A harness that kills, partitions and slows replicas and has Elle check the histories.

**Non-Goals:**

- Several groups and tablets: stage 5.
- Reading Go-written WALs: data moves through export and the loaders.
- Re-architecting the replication path; the measured bottlenecks stay in step 5 of the plan, except the
  leadership check, which raft-rs's API makes free to fix.

## Decisions

### Crates

| Crate | Path | Ports |
|---|---|---|
| `raft_storage` | `ugdb_raft/raft_storage` | `raftwal` as raft-rs `Storage` on fjall; the log dump tool |
| `raft_transport` | `ugdb_raft/raft_transport` | `conn/raft_server.go`, `pool.go`, the batching of `node.go` |
| `raft_node` | `ugdb_raft/raft_node` | `conn/node.go`'s node core, the Ready loop, proposals and their waiters, configuration changes |

`server_alpha` gains `draft.go`'s apply dispatch, snapshots, checkpoints and membership; `server_zero` gains
Zero's Raft state machine.

### The log in fjall

One keyspace per group, keyed by index, beside the store's keyspaces; hard state, configuration, snapshot
metadata and the node's identifiers in a metadata keyspace. One fjall batch per Ready holds that Ready's entries
and hard state. Dgraph's on-disk WAL layout (`wal.meta`, slot tables) is not reproduced; the entry payload is
(`key ‖ pb.Proposal`), so a dump tool can compare logs of both implementations.

Alternative: `raft-engine`, TiKV's log store. A second storage engine to operate, where fjall already provides
atomic batches and its own journal.

### Durability modes as Dgraph has them

`--survive process` (default) writes to OS buffers, which a crashed process cannot lose; `--survive filesystem`
syncs before the Ready's messages leave, once per Ready. This is Dgraph's `HardSync` (`x/config.go:157`,
`draft.go:1483`); without it Dgraph syncs once a minute (`x/x.go:1281`).

### The loop model

- A fixed pool of Raft threads; each owns a set of `RawNode`s and, per turn, collects the Ready of each node it
  holds, persists them, sends their messages (the leader before persisting, followers after, as raft-rs separates
  `messages` and `persisted_messages`), hands committed entries to each group's apply thread, and advances.
- Proposals, ticks and incoming messages reach a Raft thread through batched channels; a thread takes everything
  waiting in one receive.
- Each group's apply thread is stage 3's, fed from committed entries in place of requests.
- Each loop is a step function over its inputs, so a simulation can drive it without a runtime or a clock.
- On every soft-state change a Raft thread publishes the leader ID, the term and the node's role into atomics;
  `AmLeader` reads them. Zero adds the time of its last quorum contact.

### Commit timestamps in the entries

Stage 3's apply thread leases commit timestamps at apply time. With replication, apply runs on every replica, so the
leader leases the timestamps before proposing and writes them into the entries; every replica applies the same
ones. Zero holds a leased timestamp open until the leader reports it applied or abandoned, or until a bound
passes, and then advances its done watermark past it.

### Snapshots as Dgraph takes them

The thresholds, `calculateSnapshot`'s index and timestamp, the snapshot proposal, the stream at the snapshot's
timestamp (full, or since the follower's last snapshot), the follower's acknowledgement and the stale-predicate
cleanup are ported. The stream reads one store snapshot and writes with stage 2's batches; it needs no bulk
writer.

### Proposals as Dgraph retries them

One key per proposal across retries, timeouts of 4, 8 and 16 s, three attempts, the 256-proposal limit weighted by
attempt, timestamp reports exempt, and apply-side deduplication by key and size for 10 minutes
(`proposal.go:125-312`, `draft.go:863-963`). Zero's proposals retry with fresh keys and doubling timeouts up to
32 s (`zero/raft.go:97-185`).

### The fault harness

Processes on one host or in containers, controlled by a Rust harness: kill and restart, network partitions by
proxy, latency and loss by proxy. Workloads record histories in Elle's format. Dgraph's Jepsen nemeses
(`contrib/jepsen/main.go`: kill-alpha, kill-zero, partition-ring, plus move-tablet in stage 5) are its first
schedules.

## Risks / Trade-offs

- [raft-rs and etcd raft differ in details] → Configuration changes, read index and PreVote behave alike in
  intent but not in every edge. The ported tests and the fault harness check behaviour, not code shape.
- [A long apply batch delays the next Ready's entries] → The apply thread runs apart from the Raft threads; Raft
  keeps persisting and replicating while it applies.
- [Leased timestamps abandoned by a crashed leader delay readers] → Bounded by the lease timeout; the timeout is
  measured under the fault harness.
- [`process` durability loses acknowledged writes when every replica's host loses power together] → As in Dgraph.

## Migration Plan

New crates; the Alpha and Zero binaries gain Raft. A single-replica group is still valid, so stage 3's
deployments keep working.

## Open Questions

- Should `filesystem` durability be the default, given that the Raft log alone carries durability once the
  store syncs rarely?
- What timeout should Zero apply to a leased commit timestamp no leader reports?
