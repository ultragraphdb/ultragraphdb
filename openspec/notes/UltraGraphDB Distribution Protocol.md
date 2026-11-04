# UltraGraphDB Distribution Protocol

Nov 4, 2026 · @Marvin Hansen

The replication protocol of UltraGraphDB's replica groups: Viewstamped Replication acknowledging from memory, with a
bounded exposure and no silent loss, and an optional fast path after CURP that only an experiment can admit. This is a
design: nothing here is built, and every performance figure in it comes from another system. *Experiments* says where the
roadmap measures each claim and what each result decides.

The order of the core directive decides every trade-off below: correctness, performance, scale, developer experience.

Stage 8 builds VSR first, on the single container's `dev-ha` profile of two groups of three replicas, with both acknowledgement
rules as one setting. Stage 10 takes it to real nodes, measures it and sets its defaults.

## What each part rests on

| Part | Source | What was taken |
|---|---|---|
| Normal operation, view change, recovery | VR Revisited (Liskov, Cowling, 2012) | The whole protocol; no disk write on the normal path, recovery from peers with a nonce |
| Reacting to failure, fast and guarded modes | SAUCR (OSDI 2018) | Memory while many replicas are up, disk once only a bare majority is; recovery by the pre-crash mode |
| Hash-chained rounds, core-to-core replicas | TigerBeetle | Parent checksums; one thread driving its replicas. TigerBeetle itself persists its VSR state, so this design parts from it on the disk |
| The fast path | CURP (NSDI 2019) | Witnesses, the commutativity check, one round trip for operations that commute |

## The model

A partition is a replica group of `2f + 1` replicas, three in a cluster, with one primary per view (P8). Each core hosts
three replicas and drives them from one thread, and replication runs core to core over one connection per core per peer
node. A replica fails by crashing or by losing messages; it never lies. A disk may lose or corrupt what it holds, which
checksums detect. Replicas are assumed failure independent, the condition VR Revisited names, and *The exposure* states
what happens when they are not.

A group of one replica has no other copy, so it acknowledges from disk, always. That is the development container's default
and stage 5.5.

The metadata group and the sequencer acknowledge from disk. A lost lease reissues an ID, and a lost close rewinds epochs,
and every partition depends on both. Their rate is low enough for this to be affordable, which X5 checks.

## Positions

Each replica tracks four positions in its group's log, each only ever rising within a view's history:

| Position | Meaning |
|---|---|
| `prepared` | The rounds this replica holds in memory, in order |
| `flushed` | The rounds this replica has synced to its disk |
| `committed` | The highest round `f + 1` replicas have prepared |
| `durable` | The highest round `f + 1` replicas have flushed, which every `PrepareOk` lets the primary compute |

`durable` is what a restart of the whole group can count on: any `f + 1` replicas that restart intersect the `f + 1` that
flushed it, so at least one holds it.

## Normal operation

1. The commit step builds a round and appends it. The round is the Round frame the log stores (PROTOCOL 12): its bytes are
   what the primary sends, what a backup holds, and what replay reads, so no serialization stands between them. A view and
   its primary's number travel in the frame's `view` and `replica` fields, naming who prepared the round rather than who
   last sent it, so that a checksum holds on a retransmit.
2. The primary sends `Prepare` to every backup and holds the round in memory. A `Prepare` carries `committed`, so the common
   case needs no separate commit message.
3. A backup accepts a `Prepare` in order, checking its parent checksum, holds the round in memory, and answers `PrepareOk`
   with its `flushed`.
4. With `f + 1` holding the round, counting itself, the primary advances `committed`, applies the round, and acknowledges
   its transactions. The acknowledgement rule of commit-rounds holds with "durable in the log" read as "committed":
   the visibility watermark still covers a contiguous prefix of applied rounds.
5. Every replica flushes in the background, continuously and without waiting on the protocol, one flush covering every round
   ready at that moment (P10). A backup applies a round once a `Prepare` or `Commit` names it committed, so a store never
   holds a round a view change could replace. A divergent log suffix is truncated by the parent chain.

`Prepare`, `PrepareOk`, `Commit` and the view change's messages take numbers from 256 to 511, kept for them.

**The exposure bound.** The primary acknowledges round `p` only while `p ≤ durable + W`, where `W` is a setting in bytes. A
slow disk therefore slows acknowledgements instead of widening what a whole-group failure can lose. `W` is what the
operator trades: at 0 the protocol waits on a quorum's disks, the classical behaviour.

## Modes

A group runs in one of two modes, per the replicas it can reach:

- **Fast**, more than `f + 1` replicas reachable: `PrepareOk` means held in memory.
- **Guarded**, exactly `f + 1` reachable: each replica syncs a round before `PrepareOk`, having first flushed everything it
  held. In a group of three, one failure means guarded, so a second failure finds the data on disk.

With fewer than `f + 1` reachable, the primary commits nothing.

A replica missing the primary, or a primary missing a backup, flushes at once, before anything else: the gap between
failures is what this design spends. SAUCR measured a bare minority of its followers always flushing in time once failures
came more than 30 ms apart, at a 50 ms heartbeat, on ZooKeeper; X3 and X5 measure it here.

A replica syncs a mode marker before it acts under a mode: "guarded from `p`" once it has flushed through `p`, and "fast from
`p`" before its first in-memory `PrepareOk`. Recovery reads this marker, since it decides what the replica may have promised.

## Recovery of one replica

A replica restarting reads its mode marker.

- **Marker guarded, or a clean shutdown's:** it promised nothing past its disk. It rejoins from its disk as a replica that
  has been out of touch, and catches up.
- **Marker fast:** it may have promised rounds it no longer holds. It takes status `recovering`, and neither votes nor
  answers until it holds a state at least as recent as when it failed, or a round it prepared could be forgotten in a view
  change. It sends `Recovery` with a fresh nonce and waits for `f + 1` responses in the latest view from replicas whose status
  is normal, one of them the primary, as VR Revisited specifies.

VR's recovery sends the whole log. Here a replica holds its store's checkpoint and its log through `flushed`, so it fetches
only what follows: the chain of parent checksums finds the point where its log leaves the primary's, and it truncates to
that point. A replica behind the primary's trimmed log transfers state.

A view change proceeds as VR Revisited specifies, among replicas whose status is normal. A `recovering` replica casts no vote,
which is what preserves the intersection of quorums.

## The exposure

**What the promise is.** An acknowledged commit survives any failure leaving `f + 1` replicas with their memory or their
disk. A failure taking the memory of `f + 1` or more replicas before they react, a truly simultaneous crash, can lose
acknowledged rounds: at most those above `durable`, so at most `W`.

**Such a loss is never silent.** `f + 1` replicas in `recovering` cannot gather `f + 1` normal responses, so the group stops,
which follows from recovery's own rule and asks for no added check. The group reports its highest flushed position and the
bound. It serves again only when an operator declares the loss, naming the position it continues from.

Correlated failures are not rare, and their shapes differ:

| Failure | What protects it |
|---|---|
| One replica crashes | Its peers' memory; guarded mode for the next |
| A rack loses power, replicas in one rack | Nothing: this is the case failure independence excludes. Place replicas across failure domains, or use `W` = 0 |
| Failures seconds apart | The flush on missing a peer |
| A planned restart of everything | Clean shutdown flushes, and a clean marker rejoins from disk |
| A deterministic crash on one round, on every replica | The primary applies before acknowledging, so that round is unacknowledged; earlier rounds above `durable` are exposed. X3 tests this directly, since replicas run the same code on the same bytes and fail close together |
| Disk corruption on one replica | Checksums; the replica recovers from peers |

## Reads

A primary a view change has deposed does not know it, and a read of its latest state would go back in time. Reads at a closed
epoch and monotonic-fast reads are unaffected, since any replica that has closed the epoch answers them. *Open questions* holds
the latest-state read.

## What this asks of the layers below

- `OrderedLog` keeps its shape. `committed` reads "journaled by a quorum", and it becomes "held by a quorum, in memory or on
  disk by the mode"; `sync` completes a ticket at commit, and flushing moves behind the interface. Stage 5.5's hosts are
  generic over this trait, so the commit step does not change.
- commit-rounds' acknowledgement requirement and its "failed sync" scenario gain the replicated reading.
- `lean/RoundModel` and `lean/PlacementModel` model crashes after durable steps. Under this protocol a replica's crash loses
  what it held in memory, and "durable" means committed by a quorum. The epoch inequality and reverse-read completeness are
  proved again under that crash.
- A `trim_through` stays below the store's synced checkpoint and `committed`.

## The fast path, an experiment

What follows is a hypothesis to test. The base protocol stands on its own without it.

**The idea.** A qualifying commit skips the primary's round trip to its backups: the frontend sends the frame to the primary
and to every witness at once and answers the client once all accept, one internal round trip in place of two. Ordering follows
in the background through normal rounds.

**What qualifies.** A DirectWrite (PROTOCOL 6.6), which reads nothing by construction and refuses unique predicates, whose changes write only statements the schema's classification marks
coordination-free, and needs no first-time registration, no new blank node, no single-valued or unique predicate and no
commit-time operation. The frontend decides from the catalog and the frame; the primary may still decline.

**Witnesses.** Each replica's core hosts a witness: a table in memory of the frames it accepted, with a key per statement
derived from its text, since the frontend has text and no IDs yet. A witness accepts a frame only if its keys commute with
everything it holds. The primary applies the same test against its unordered commits. A witness stores the frame's bytes as
they arrived, a literal path from the wire to memory.

At `f = 1` CURP requires `f + ⌈f/2⌉ + 1` witnesses, all three, so the fast path runs only in fast mode, and any failure turns
it off. A rejection or a missing witness falls back to the normal path through the primary.

**The reply.** A fast acknowledgement names no version, no position and no epoch, since failover could give the commit
different ones. Its session token is marked pending. This changes the Commit reply and `SessionToken`.

**Reads and transactions around it.** The session waits for its pending commit to be ordered before its next request. A read of
another session whose keys meet an unordered commit forces that commit's ordering first, as CURP's masters do. A read-write
transaction binding at the watermark waits for unordered commits to be ordered, or the requirement "No acknowledged commit is
invisible" of commit-rounds breaks. An epoch closes on a partition only once commits acknowledged before the close was delivered
are ordered at or below it, which needs proving against the epoch inequality.

**Failover.** A witness stops accepting when a view change reaches it. The new primary gathers witness tables with the
`DoViewChange` messages, and replays what the log lacks. Exactly-once comes from the session table the partition already
keeps, and IDs may differ on replay because no system ID leaves the database. After ordering, a primary sends batched garbage
collection to its witnesses. A witness that restarts holds nothing, so it answers nothing until a new configuration admits it.

**Costs known already.** Each qualifying frame crosses the network `2f + 1` times from the frontend in addition to the rounds
that later carry it, the frontend fans out per commit, and rounds already batch, so a fast acknowledgement bypasses the batching
the normal path uses to amortise consensus. The gain is unknown until measured against all of these.

## Experiments

Nothing in this note can be decided by reasoning from other systems' numbers: their stacks, drives and networks differ. Correctness
is settled by proof and simulation, which need no hardware, so it comes early. Performance needs real disks and real networks, so
it comes late. Each result below feeds one gate.

| # | Question | Where | How | What it decides |
|---|---|---|---|---|
| X1 | How much of a commit is the fsync, on which drives? | Stage 5.5, task 12.5b | Time per phase of a commit through the three binaries, at 1 and at many relations per commit, on the VM disk and on a drive without power-loss protection | The ceiling on what memory acknowledgement can gain. A small share here means the base protocol's main benefit is smaller than assumed |
| X2 | How many writes could qualify, and how often would they conflict? | Stage 6.5, against its chaos schemata; repeated on DeepBrain's writes at stage 8 | Offline. Count each commit by its type: DirectWrite, ConditionalWrite, read-write. Classify the first by the UGS schema, and replay the log's conflict keys (write-set record sections 6 to 8) through a model of a witness window | Whether the fast path has a workload at all. It costs no protocol work and can end the idea here |
| X3 | Is memory acknowledgement correct? | Stage 8, with VSR on `dev-ha` | A Lean model of replicas, modes, amnesia and `W`, with the theorems below; the simulator injecting crashes, memory loss, simultaneous and spaced failures, and a round that crashes every replica | Gate G1 |
| X4 | What does the protocol itself cost? | Stage 8, `dev-ha` | Disk-acknowledge against memory-acknowledge on one build, over Unix sockets with injected delay: CPU per round, bytes per round, throughput. Loopback measures cost, never real latency | Finds regressions early; decides nothing about latency |
| X5 | What does memory acknowledgement gain on real hardware? | Stage 10, first, on physical Linux nodes | Median and p99 commit latency and throughput against `W` = 0, on drives with and without power-loss protection, pinned and unpinned; the time to reach guarded mode against failure spacing; metadata and sequencer costs | Gate G1, and the default `W` |
| X6 | Is the fast path correct? | Stage 10, after G1, on stage 8's classification | The Lean model extended with witnesses, against "No acknowledged commit is invisible", the epoch inequality and exactly-once; simulator differential against the base protocol on the same history; Jepsen | Gate G2 |
| X7 | What does the fast path gain? | Stage 10, on X5's harness and X2's workload | Median and p99 latency and throughput, with X2's real mix and its conflict rate, against the base protocol at the same load, with and without batching | Gate G2 |

**Theorems X3 proves.** An acknowledged round is lost only if `f + 1` replicas lose their memory before reacting; such a loss is at
most `W`; a group with `f + 1` amnesiac replicas serves nothing until a loss is declared; a `recovering` replica's vote is never
counted; the epoch inequality and reverse-read completeness hold.

**Gate G1, memory acknowledgement.** Taken at the start of stage 10, on what stage 8 built. X3 must hold before X5 runs. A group
stays at `W` = 0 until G1 passes, the same code with a different setting, and that is the fallback if it fails.

**Gate G2, the fast path.** Taken in stage 10, before commands 256 to 511 and the Commit reply freeze. X6 must hold before X7 counts.
A failed gate removes the prototype from the tree, since nothing depends on it.

## What stages before 8 keep open

Nothing needs building before stage 8. The conflict keys stay in the record, the log stays behind `OrderedLog`, rounds keep their `view` and
`replica` fields, and commands 256 to 511 stay free. With no outside users, the Commit reply can change when G2 passes.

## Open questions

- What thresholds should G1 and G2 require? A gain that does not repay a recovery protocol and a changed reply should stop the fast
  path, and the numbers should be fixed before the measurements are seen.
- Should the acknowledgement rule be a per-deployment setting beyond `W`, such as a named `durability` mode, or is `W` = 0 enough?
- A declared loss is not local: other partitions hold marks and referrer-set memberships from registrations the lost group
  answered, clients hold tokens above the position, and reverse-read completeness rests on those sets. What must a declaration do
  across the cluster to restore P24's rules?
- How does a latest-state read avoid a deposed primary: a leader lease, which depends on bounded clock drift, or a round of
  messages per read, which costs what memory acknowledgement saved?
- Should flexible quorums join this protocol, as TigerBeetle uses them? A smaller replication quorum held only in memory means fewer
  copies of an acknowledged round, so the exposure needs restating first.

## Sources

- [Viewstamped Replication Revisited](https://www.cs.princeton.edu/courses/archive/fall19/cos418/papers/vr-revisited.pdf), Liskov and Cowling, MIT-CSAIL-TR-2012-021: sections 4.1 to 4.3, read directly.
- [Fault-Tolerance, Fast and Slow: Exploiting Failure Asynchrony in Distributed Systems](https://usenix.org/conference/osdi18/presentation/alagappan) (SAUCR), OSDI 2018: introduction and evaluation, read directly. Results are for ZooKeeper.
- [Exploiting Commutativity For Practical Fast Replication](https://www.usenix.org/conference/nsdi19/presentation/park) (CURP), NSDI 2019: the protocol's description, read directly. Results are for RAMCloud and Redis, on key-value operations.
- [TigerBeetle VSR internals](https://raw.githubusercontent.com/tigerbeetle/tigerbeetle/main/docs/internals/vsr.md).
- [Exploiting Nil-Externality for Fast Replicated Storage](https://par.nsf.gov/biblio/10299722) (Skyros), SOSP 2021: abstract, for the alternative of deferring ordering on a fork of VR.
- [Optimizing Distributed Protocols with Query Rewrites](https://arxiv.org/abs/2404.01593), SIGMOD 2024: decoupling and partitioning, which raise throughput, not latency, and which this design leaves out until a core-level bottleneck is measured.
- `openspec/notes/UltraGraphDB Shard-per-Core Write Path.md`: P8, P10, P18, P24. `openspec/notes/UltraGraphDB Distribution Architecture.md`: *Write classification*. `openspec/notes/UltraGraphDB Single-Container Deployment.md`: `dev-ha`. `ROADMAP.md`: Aeron's stream position, whose open questions on the Round frame's position, repair and flow control apply to `Prepare`. `openspec/notes/backlog.md`: item 12.
