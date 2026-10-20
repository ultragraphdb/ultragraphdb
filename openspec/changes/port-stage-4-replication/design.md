## Context

Stage 4 replicates stage 3's group and its Zero as Raft groups of three. The Go code is `raftwal` (the WAL),
`conn` (the Raft node wrapper `node.go`, the transport `raft_server.go`, connection pools `pool.go`),
`worker/draft.go` (the Alpha's Ready loop, apply, snapshots, checkpoints), `proposal.go`, `snapshot.go`, `queue.go`,
the membership half of `groups.go`, and Zero's `raft.go`, `zero.go` and `run.go`.

Dgraph configures etcd raft with 100 ms ticks, an election after 20, heartbeats every tick, 256 messages in flight,
256 KB per message, 64 MB of committed entries per Ready, read-only safe mode, PreVote on and CheckQuorum off
(`conn/node.go:91-130`). Its Raft snapshots carry metadata only: a follower behind the log fetches a node's store at
the snapshot's timestamp as a stream before recording the snapshot (`draft.go:1405-1473`, `snapshot.go`). Zero
applies its committed entries inline and replicates membership, tablets, the leases' high-water marks (`MaxUID`,
`MaxTxnTs`, `MaxNsID`, `MaxRaftId`), the cluster ID, snapshot timestamps, namespace deletes and its commit decisions
(`pb.proto:138-154`, `zero/raft.go:399-488`); its done watermark lives in the leader's memory (`oracle.go:42`).
Each Alpha group's leader proposes Zero's deltas into its own log, and every replica's reads wait on the watermark it
applied from that log (`draft.go:1023`, `posting/oracle.go:296-339`). Leadership checks call `Status()` twice, each a
round trip to the Raft goroutine, in Zero and in the Alpha (`zero/raft.go:56-62`, `draft.go:2027-2033`).

## Goals / Non-Goals

**Goals:**

- Dgraph's replication behaviour with raft-rs, on the overview's loop model.
- Determinism: every replica applies the same entries to the same state, upsert blocks, checks, uids and
  deduplication included.
- Leadership checks that never wait on a Raft loop.
- A harness that kills, partitions and slows replicas and has Elle check the histories.

**Non-Goals:**

- Several groups and tablets: stage 5.
- Reading Go-written WALs: data moves through export and the loaders.
- Re-architecting the replication path; the measured bottlenecks stay in step 5 of the plan, except the leadership
  check, which raft-rs's API makes free to fix.

## Decisions

### Crates

| Crate | Path | Ports |
|---|---|---|
| `raft_storage` | `ugdb_raft/raft_storage` | `raftwal` as raft-rs `Storage` on fjall; the log dump tool |
| `raft_transport` | `ugdb_raft/raft_transport` | `conn/raft_server.go`, `pool.go`, the batching of `node.go` |
| `raft_node` | `ugdb_raft/raft_node` | `conn/node.go`'s node core, the Ready loop, proposals and their waiters, configuration changes |

`server_alpha` gains `draft.go`'s apply dispatch, snapshots and membership; `server_zero` gains Zero's Raft state
machine.

### The log in fjall

One keyspace per group, keyed by index, beside the store's keyspaces; hard state, configuration, snapshot metadata
and the node's identifiers in a metadata keyspace. One fjall batch per Ready holds that Ready's entries and hard
state. Dgraph's on-disk WAL layout (`wal.meta`, slot tables) is not reproduced; the entry payload is
(`key ‖ pb.Proposal`), so a dump tool can compare logs of both implementations.

raft-rs (`raft` 0.7) encodes its messages with rust-protobuf 2, or prost 0.11 through `prost-codec`; neither is tonic
0.14's prost, so Raft messages cross the transport as encoded bytes.

Alternative: `raft-engine`, TiKV's log store. A second storage engine to operate, where fjall already provides
atomic batches and its own journal.

### Durability modes as Dgraph has them

`--survive process` (default) writes to OS buffers, which a crashed process cannot lose, and syncs the log and the
store once a minute (`x/x.go:1281`). `--survive filesystem` syncs the log, when a Ready must sync, before a follower's
messages leave (`draft.go:1483`), and syncs the store after each applied batch (`draft.go:1005-1009`). The leader
sends its messages before persisting in both modes, as Dgraph's does.

### The loop model

- A fixed pool of Raft threads; each owns a set of `RawNode`s and, per turn, for each node it holds: takes the
  Ready, sends `messages`, persists the Ready in one batch, sends `persisted_messages`, calls `advance_append`,
  handles the returned `LightReady` (its commit index, committed entries and messages), hands committed entries to
  the group's apply thread, and calls `advance_apply_to` as the apply thread reports.
- Proposals, ticks and incoming messages reach a Raft thread through batched channels; a thread takes everything
  waiting in one receive.
- Each group's apply thread is stage 3's, fed from committed entries in place of requests. A snapshot stream runs off
  the Raft threads, so one group catching up does not stall the others a thread holds; Dgraph blocks its loop there
  (`draft.go:1449-1457`).
- Each loop is a step function over its inputs. raft-rs draws election timeouts from `rand::rng()`, so a simulation
  sets `max_election_tick` to `min_election_tick + 1` to make runs reproducible.
- On every soft-state change a Raft thread publishes the leader ID, the term and the node's role into atomics;
  `AmLeader` reads them. Zero adds the time of its last quorum contact.
- CheckQuorum stays off, as in Dgraph; Zero refuses commit-timestamp leases to a node its membership does not name as
  its group's leader, which keeps an isolated former leader from leasing timestamps that never commit.

### What travels in an entry

With replication, apply runs on every replica, so whatever the apply thread would otherwise draw from Zero is drawn
by the leader and carried in the entry:

- **Commit timestamps.** The leader leases them under its group and term and assigns them as it appends proposals,
  so timestamp order is log order; the apply thread aborts an entry whose timestamp is not above the last it applied.
  Zero holds a leased timestamp open until the group reports it. Once a new leader has applied an entry of its own
  term, every earlier-term entry still in the log has been applied before it and the others can never commit, so it
  reports every earlier-term lease not applied as abandoned. Abandonment follows the log, not a clock.
- **Blank-node uids,** upsert blocks included.
- **The done watermark.** The leader proposes Zero's watermark into the log, as Dgraph proposes deltas, and each
  replica serves a read at `t` only after applying a watermark entry at or above `t`.

A retry carries the same bytes as its first attempt, timestamp included, so deduplication by key and size holds.

### The applied index and deduplication in the store

The apply thread writes the applied index, and the keys of the proposals it applied, in the same store batch as the
entry's writes. A restarted replica skips entries at or below the applied index, and every replica skips a duplicate
key from the store, so a restarted or lagging replica decides as the others did. Dgraph keeps its deduplication map
in memory with a wall-clock expiry (`draft.go:874-963`) and skips entries below a checkpoint it records once a minute
(`draft.go:1173-1192`, `1513`).

### Snapshots as Dgraph takes them

The leader checks once a minute and proposes a snapshot when there is none, when the log exceeds a size bound (Dgraph
counts more than 4 WAL files), or when 30 minutes have passed and the applied index is 10,000 entries past the first
(`draft.go:1210-1262`). The snapshot is at the latest applied index: Dgraph's `minPendingStart` rule has no pending
transactions to bound it. Its read timestamp is the highest commit timestamp at or below that index, which the order
rule makes the last one. The stream, its full and incremental forms, the acknowledgement and the stale-predicate
cleanup are ported; the stream reads one store snapshot and writes with stage 2's batches.

### Proposals as Dgraph retries them

One key per proposal across retries, timeouts of 4, 8 and 16 s, three attempts and only while the entry was not
found among committed entries, the 256-proposal limit weighted by attempt, watermark entries and snapshot proposals
exempt (`proposal.go:125-312`, `draft.go:1108-1153`); schema proposals take one 3-minute attempt
(`proposal.go:192-195`). Zero's proposals retry with fresh keys and doubling timeouts up to 32 s
(`zero/raft.go:97-185`).

### The fault harness

Processes on one host or in containers, controlled by a Rust harness: kill and restart, network partitions by proxy,
latency and loss by proxy. Workloads record Jepsen histories (EDN or JSON) for `elle-cli`, which checks `list-append`
and `rw-register` histories: `strict-serializable` for the read-set and upsert-block forms, `snapshot-isolation` for
requests checked on their write keys. Dgraph's Jepsen nemeses (`contrib/jepsen/main.go`: kill-alpha, kill-zero,
partition-ring, plus move-tablet in stage 5) are its first schedules.

## Risks / Trade-offs

- [raft-rs and etcd raft differ in details] → Configuration changes, read index and PreVote behave alike in intent
  but not in every edge. The ported tests and the fault harness check behaviour, not code shape.
- [A long apply batch delays the next Ready's entries] → The apply thread runs apart from the Raft threads; Raft
  keeps persisting and replicating while it applies.
- [Leases held open by a slow new leader delay readers] → Readers above an open lease wait until the new leader
  applies an entry of its term, about one election timeout after a leader loss.
- [`process` durability loses acknowledged writes when every replica's host loses power together] → As in Dgraph.

## Migration Plan

New crates; the Alpha and Zero processes gain Raft. A single-replica group is still valid, so stage 3's deployments
keep working.

## Open Questions

- Should `filesystem` durability be the default, given that the Raft log alone carries durability once the store
  syncs rarely?
- What size bound on the fjall log replaces Dgraph's four-WAL-file snapshot trigger?
