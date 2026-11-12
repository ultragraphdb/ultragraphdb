# UltraGraphDB Role Merge and Availability Feasibility

Nov 12, 2026 · a feasibility study following `UltraGraphDB Architecture Reviw.md`

**The question.** Does the role architecture need a refactor before the tile store (stage 6.5) is built? Three
proposals were put to the study:

1. Merge the frontend and compute into one node, to remove network hops and a Kubernetes workload.
2. Move to an availability regime in which any one node may fail and the cluster keeps running, for the merged node
   and for storage.
3. Treat S3 support as mandatory.

Everything below is read from the specifications, the designs and the sources. Nothing was built or measured for it.
A figure is quoted from the document that measured it.

## Answer

**The tile store can start now. Two decisions must land before two of its task groups, and replication waits for it.**

| Proposal | Verdict | When |
| --- | --- | --- |
| Merge the client endpoint and compute | Feasible, and recommended | Before the tile store's group 9, which rewrites both crates and the protocol between the roles |
| Keep the metadata group and the sequencer in the merged process | Feasible today, advised against from stage 7 | The owner leaves the merged process before the query parsers enter it |
| Any one node may fail | Feasible on the designs that exist; three parts are unspecified | Partitions after the tile store; the control group may come sooner |
| Literal multi-master, several writers of one partition | Rejected: it contradicts strict serializability and the one commit step per log | — |
| S3 support | Feasible on the tile format as designed; four decisions are missing from the tile store | The decisions before group 5, the grid. The build after replication |

The tile store's groups 1 to 8 change the storage side alone: the engine, the log's record, the durable seam, the
format, the grid, the simulator and the storage runtime. Only group 9 changes the frontend and compute. So the role
refactor delays no part of the tile store that is ready to start, and it must be decided before group 9 so that the
work of that group is done once.

## 1. Merging the frontend and compute

### What the split costs today

| Path | Hops today | Hops merged |
| --- | --- | --- |
| A write or a commit | Client, frontend, partition | The same. Compute is no part of it |
| A read | Client, frontend, compute, partitions, and back through both | Client, node, partitions |

- **Every read and every row of its answer crosses one more process.** The frontend relays the read to a compute
  process and relays each Result frame and each Credit back. The one hop cost that was measured is on the write path:
  about 1.1 ms for two hops between two virtual machines (`../../benches/benches-2026-11-09.md`). No read was
  measured through the binaries, so the saving on reads is unmeasured.
- **The split creates an ordering problem and then solves it.** A read of a read-write transaction reaches its
  partition through compute, on a connection apart from the transaction's Bind stream. The frontend therefore holds
  the read until the client's earlier writes are answered (stage 5.5's design, *Roles and their protocol*). In one
  process the same core holds both.
- **The split creates a failure class and then handles it.** The frontend detects a hung compute process by a timer,
  retries a read on the next compute process when no frame has left, and answers code 17 when one has. That handling
  lives in `client_session_reads.rs`.
- **Both roles hold a connection to every partition.** Each frontend endpoint core and each compute core keeps its
  own. Merged, one set remains.
- **Stage 7 would build a plan format for the wire.** The lanes of the tile store are attached by "the frontend and
  the planner", and execution sits on compute. A planner and an executor in two processes need a serialized plan. In
  one process they share memory.

### What the split buys today

- **A compute crash loses no client connection,** and a read that has sent no frame is retried unseen.
- **A compute crash cannot take the metadata group and the sequencer with it.** This is the benefit that matters.
- **Heavy analytical work stays off the process that authenticates clients.** The tile store places analytical
  projections on compute, and the Shard-per-Core note places GPU work there.
- **Compute scales apart from the endpoint.** A stateless merged node scales the same way, so this one is no loss.

### The precedent in the repository

The Dual Query Engine Plan, the design before the distribution architecture, already has this cut: compute nodes that
"run the engines", an ordering service holding the sequencer and the log, and storage nodes. It has no relaying
frontend. The frontend as endpoint plus metadata group came with the distribution architecture, after StarRocks,
whose frontends plan and do not execute.

### Feasibility in the code

Both roles are thread per core on compio, and a compute read already runs "wholly on the core it arrived on". An
endpoint core would own the read's work where it now owns a relay. The frontend's owner sits on its own thread behind
a message handle, so the endpoint cores change and the owner does not.

| What changes | How |
| --- | --- |
| `ComputeRead` (523), compute's Join, the member list's compute entries | Retired; the edit touches `PROTOCOL.md` sections 11.12, 13.1, 13.2 and 13.12 |
| The frontend's read relay, its hung-compute timer and retry | Deleted |
| `ultragraphdb_compute`, its image, its test crate, its Helm Deployment, its profile files, its supervisor entry | Removed, after your approval of each deletion |
| The rule "frontend and compute link no storage engine" | Kept as it is; the merged node links none |

No outside user exists, so the wire changes in place.

### The condition: the owner must not stay in the merged process

Merging puts the query engine in the process that hosts the metadata group and the sequencer. From stage 7 that
engine parses text from any client, and the roadmap itself names the parsers and the planner as the surface that
fuzzing must reach. A failure there, an exhausted stack or exhausted memory, would then stop epoch closes for every
partition. Today a compute failure cannot do that.

The merge therefore comes in two cuts, and the second is the same cut the availability regime needs:

- **Cut A, with the tile store's group 9.** The endpoint and compute become one **query node**. The owner stays in
  the process on its own thread, as now. Compute is quad-pattern merging at this stage, so the exposure is small.
- **Cut B, before stage 7's parsers land.** The owner moves to a **control** process: the metadata group, the
  sequencer, membership and the session registry. The query node holds no durable state after it.

After both cuts the cluster has the query node, control and storage, beside the config service. That is the same
number of processes as today. What improves is the read path, the single set of partition connections, and a clean
line between a stateless tier and the stateful ones. Folding the config service into control would bring the count
to three, and is left as a question below.

**Cut B has a price that the regime in section 2 pays anyway.** An endpoint core asks the owner for each commit's
owner check. Out of process that is a round trip per commit. Backlog item 13 already asks whether the check can be
held locally up to a fenced epoch. That question must be answered, and proved, before cut B.

## 2. Any one node may fail

### The requirement, restated

"If any one node goes down and enough spare nodes exist, the cluster keeps running at somewhat degraded
performance." That is tolerance of one failure with every node active. It needs three things: several query nodes
that all take clients, a replicated control group, and every partition as a group of three with its leaders spread.

Literal multi-master, several replicas of one partition accepting writes at once, is a different thing and is not
needed for this requirement. It would give up strict serializability within a group and the single commit step, both
decided. Under P8 every storage node already leads a third of the replicas it hosts, so every node is a writer for
some partitions and a follower for others.

### What exists

- **The replication protocol** (`UltraGraphDB Distribution Protocol.md`): VSR for groups of three. Its guarded mode
  is the requirement's degraded state. With one replica lost, the two that remain sync each round before they
  acknowledge, so a second failure finds the data on disk.
- **The tile store is built for replicas:** grids identical byte for byte (T6), one grid per replica (T21), the read
  barrier after a view change (T28), followers that catch up by tiles (T29).
- **Sessions do not live in a frontend's memory.** The registry is the metadata group's, a partition keeps its own
  session table, and a commit in doubt is resolved on the next connection by a Recall of the partitions
  (`PROTOCOL.md` section 11.5). A client can already change frontends between requests.
- **A partition outlives a silent sequencer by design.** It keeps stamping its open epoch until the epoch is full.

### What is unspecified

| Gap | Why it matters |
| --- | --- |
| **Replication of the control group.** The protocol note gives it one sentence: it acknowledges from disk | Leader discovery by the other roles, epoch continuity across a view change, and learners on query nodes are all unwritten |
| **The owner check with several query nodes** (backlog item 13) | Either every commit waits on the control leader, or a fenced local check is designed and proved |
| **Where a session's commit in flight is held.** Today the owner holds it, which is what refuses a second commit with code 14 | With several query nodes that state needs one home, or a session needs one node |
| **How roles find the control group.** One `frontend_address`, and the settings format has no lists | A name resolving to several addresses puts the resolver on the critical path, which the review found unbuilt |
| **The exit after 60 seconds.** Compute and storage stop when the frontend stays away | In this regime a lost peer makes a role not ready. It does not end it |
| **The config service's replicas** | With one instance, a running cluster survives its loss, and a role that then crashes cannot start again |
| **Connections.** Each query core holds one per partition | The count grows as query cores times partitions. P12 aggregates per node for epochs alone |

### Order of work

- **Partition replication cannot precede the tile store.** The KV store it would replicate is deleted in group 11.
- **Control replication can.** It is the simple variant of the protocol: it acknowledges from disk, at a low rate,
  with no memory mode. Its logs sit in the write-ahead log crate that the tile store's groups 2 and 3 restructure, so
  it follows group 3 at the earliest. Building it first exercises view change, recovery, the Lean model and the
  simulator on the easy group before the partitions need them.
- **The roadmap reaches this regime late.** Stage 8 proves VSR inside one container, where every replica shares one
  machine. Real nodes come at stage 10. If the single frontend is a blocker, the lever is the roadmap's order, and
  the earliest point for control replication is after the tile store's group 3.
- **One change needs no stage.** The 60-second exit can become a retry with the role reporting not ready.

## 3. S3 support

### Where it stands

S3 was deferred, and the deferral is on record: the roadmap places "tiles in S3 mode, cached on compute" at stage 10,
and the tile store note lists it under *Later*. The tile store gives it one sentence. It has no decision, no
scenario and no experiment. So the format allows S3, and no requirement holds the tile store to it.

### What already fits

- **Tiles are immutable, checksummed and self-describing,** and every write is out of place.
- **An extent holds whole tiles and is freed whole.** A sealed extent of 1 MiB is a natural object.
- **Grids are identical across replicas.** Any replica's upload is the same bytes, so one upload serves the group
  and an object can be verified against any replica.
- **The engine does no I/O of its own.** It asks its shell to read a tile at an address, and a read already yields
  while its tiles load. A fetch from S3 is another answer to the same request.
- **The format is a crate of its own** (`ultragraphdb_store_tile_format`), so a role can decode tiles without the
  grid or the commit step.

### What is missing from the tile store

1. **The identity of an object.** A tile reference is address, length and checksum, and an extent's address is
   reused after it is freed. An object's key needs more than the address.
2. **Allocated apart from resident.** The grid's capacity is a cluster-wide setting, so that free space is a
   function of the log (T31). With data larger than the local drive, an extent can be allocated in every replica's
   grid and present on none. Presence must be a local fact that no logged decision reads.
3. **Retention for readers elsewhere.** A freed extent is reused once the next checkpoint is durable, which protects
   a reader on the same node. An object in S3 is read by other machines and needs a horizon of its own. The logged
   fold horizon is the candidate.
4. **Publication of the manifest.** The superblock is four copies at the head of the grid file. A reader of S3 needs
   the manifest as an object and a rule for which one is current.

These belong in the tile store's decisions before group 5 builds the grid. They constrain names and bookkeeping and
add no code to stage 6.5.

### Two modes, one a subset of the other

| Mode | Who reads S3 | What it needs beyond the four decisions |
| --- | --- | --- |
| **Tiered.** The local grid is a cache of tiles whose authority is S3 | Storage alone | An uploader, a fetch on a miss |
| **Shared data.** The original design's S3 mode | Query nodes too, with a tile cache on local NVMe | The read kernel on the query node, and a way to fetch from storage the changes not yet in a tile |

The original design's S3 mode calls storage a "log tier only". Under P24 a partition executes transactions and
decides conditional writes, so it reads the data it decides on, and it keeps a tile cache in either mode. Tiered is
the smaller change, keeps S3 write credentials on storage alone, and is what shared data builds on.

**S3 mode depends on replication.** An upload lags a commit, so the newest rounds exist only in the log. With one
replica, losing the node loses them whatever S3 holds. S3 mode is safe from stage 8.

## 4. What changes for the tile store

| Before | Decide or do |
| --- | --- |
| Group 5, the grid | The four S3 decisions of section 3 |
| Group 9, the wire and the read kernels | Cut A, and the edit of `PROTOCOL.md` that retires `ComputeRead`. Task 0.1 already reviewed sections 11 to 13 as they stand, so the edit reopens that review for those sections |
| Stage 7 | Cut B, with backlog item 13 answered and proved |
| Stage 8 | The specification of the control group's replication, discovery and the session in flight |

## Open questions

- [ ] Can the owner check be held on a query node up to a fenced epoch, as backlog item 13 asks, and does exactly-once
  hold under it in the Lean model?
- [ ] Where is a session's commit in flight held when several query nodes serve one session?
- [ ] Does the config service fold into the control group, which ends one service holding several clusters' settings,
  or does it stay apart and take replicas of its own?
- [ ] Does control replication move ahead of stage 8, after the tile store's group 3?
- [ ] Is tiered S3 enough for the first S3 release, or is shared data required from the start?
- [ ] Do the three control voters also take clients in a small cluster, or is control always its own process?
- [ ] Does heavy analytical work get a pool of query nodes of its own, or a role of its own when GPU work arrives?
