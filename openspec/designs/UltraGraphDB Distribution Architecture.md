# UltraGraphDB Distribution Architecture

Nov 12, 2026 · @Marvin Hansen

UltraGraphDB is a distributed graph database of two roles, **compute** and **storage**. Compute is the client's
endpoint and the place queries run, and it holds nothing that must survive. Storage owns the data, decides every
commit, and hosts the **system group**, the replica group that keeps the cluster's books. No outside coordination
service exists: each replica group decides its own failover, and the books are one more replica group.
Its defining choice is **schema-driven coordination avoidance**: the
typed schema declares every invariant, so the database decides per write whether more than one partition must agree.
Every fact has one owner, which writes it in one local commit. Data lives in a graph-native tile store with two
bindings, direct-attached NVMe and an object store. Replication is Viewstamped Replication (VSR), built at stage 8.

The design of Oct 31 that this one replaces is `archive/UltraGraphDB Distribution Architecture.md`. What led from that
design to this one is in `archive/UltraGraphDB Architecture Reviw.md` and
`archive/UltraGraphDB Role Merge and Availability Feasibility.md`.

**Status of each part.**

| Part | Status |
| --- | --- |
| Frontend, compute and storage as three binaries, one replica each, on the KV store | Built (stage 5.5) |
| The frontend and compute as one role, named compute | Decided 2026-11-12; stage 5.9 |
| The tile store | Stage 6.5; specified (`../changes/stage-6-5-tile-store`), three of its tasks done |
| The object-store binding of the tile store | Decided 2026-11-12; enters the tile store's specification before stage 6.5 is built |
| The system group hosted by storage | Decided 2026-11-12; stage 5.8, the next stage. It runs inside the frontend today |
| Client discovery by seed addresses and a versioned node list; compute tags | Decided 2026-11-12; the protocol with the merge, the drivers when several compute nodes run |
| Replication of every group | Designed (`UltraGraphDB Distribution Protocol.md`); stage 8, then stage 10 |
| Several compute nodes | From stage 5.9, with the node list (`PROTOCOL.md` section 11.14); to be confirmed (review of 2026-11-12, U3) |

## Cluster architecture

**Two roles.** A request passes through compute and storage and through nothing else.

| Role | Does | State | Scales by |
| --- | --- | --- | --- |
| **Compute** | The client endpoint over the binary protocol; authentication and sessions; the query engines, their planning and execution; reads across partitions, merged; the relay of writes and commits to their partition | None that must survive | Adding compute nodes |
| **Storage** | Partitions, one per core: each executes transactions, runs the commit step, writes its log and keeps its tiles; answers compute's scans and frontier requests. Three storage nodes also host the system group | Durable: the logs, the tiles and the system group's state | Adding partitions and nodes |

The design this one replaces had three roles on a read's path: the frontend took the request, compute executed it and
storage answered. Here a read and a write each pass through two.

**The system group keeps the cluster's books, and it is no role.** It holds the metadata state machine (membership,
placement, catalog versions, ID sequences, principals, the session registry) and the sequencer, which closes epochs.
It is a replica group like a partition's: it runs on storage nodes, over its own logs, under the replication protocol
the data uses, with one replica until stage 8 and three from then (decided 2026-11-12). No request's body passes
through it. CockroachDB keeps its metadata in ordinary replicated ranges in the same way.

What its home gives:

- **Compute is stateless.** It needs no volume, grows and shrinks freely, and runs as groups by tag, one of them
  on special hardware.
- **Storage is a whole cluster by itself.** Storage nodes start, recover, fail over and close epochs with no compute
  node running, and compute attaches to a cluster that is already complete.
- **A failure of the query engine cannot stop the books.** The parsers and planners, the largest code surface, share
  no process with them.
- **The smallest cluster that survives a failure is three machines,** each running storage and compute.

| Cluster | Processes | Survives |
| --- | --- | --- |
| Development | One container: compute, storage with one replica of every group | Nothing; it is for development |
| Smallest available | Three machines, each with storage and compute | The loss of any one machine |
| Larger | Storage nodes for more partitions, compute nodes for more clients; three storage nodes host the system group | One machine in every group |

Until it moves, the system group runs where it runs today, inside the process that takes clients, with one replica.

**One service beside the cluster.** The config service holds every target's settings as numbered revisions, validated
whole, and a role fetches its settings from it at start. It holds a few kilobytes and takes replicas from stage 8.

The supervisor starts the processes of the development container, and the config CLI validates a settings folder
before anything runs. Neither is part of a cluster.

```
                    clients
                       |   binary protocol, TLS 1.3
                       v
        +-------------------------------+   settings at start   +----------------+
        |            compute            | --------------------> | config service |
        |  endpoint cores: sessions,    |                       +----------------+
        |  query engines, read merge,   |                               ^
        |  relay of writes              |                               |
        +-------------------------------+                               |
           |   ^  writes, commits,     |  Join, placement, catalog,     |
           |   |  scans, frontiers     |  logins, sessions, close       |
           v   |                       v  requests                      |
        +---------------------------------------------+                 |
        |                   storage                   |  settings       |
        |  a partition per core:   | system group:    | ----------------+
        |  log, commit step,       | metadata,        |
        |  tile engine             | sequencer        |
        +---------------------------------------------+
            |                      |
       local NVMe            object store
       (direct binding)      (object binding)
```

### One role serves clients and executes

The frontend and compute were two processes: one took a client's requests and relayed them, the other executed its
reads. They are one role, named compute (decided 2026-11-12). The name is the industry's: the design separates
storage from compute.

- **A read crosses one process fewer.** The client's core routes the read, asks the partitions concerned, merges their
  rows in key order and answers under the client's credit. The relay of every Result frame and every Credit is gone.
- **A transaction's reads and writes meet on one core.** The core holding a read-write transaction's Bind stream also
  sends its scans, so their order is local.
- **Planner and executor share memory.** The query core of stage 7 needs no plan format for the wire.
- **One set of connections.** An endpoint core keeps one multiplexed connection per partition, for reads and writes
  alike.
- **A write's path keeps its length.** Client, compute, partition: only a read crossed the second process.

A read runs wholly on the core its connection arrived on. Read work on a core is bounded per turn, so a long merge
never starves the relay of a commit on the same core.

Two things were given up, and both are accepted. A read that had sent no frame was retried on another
executing process unseen by the client; a failed compute node is now the client's reconnect. A failure of the
executing process could not reach the cluster's books; that protection returns when the system group moves to
storage.

The Dual Query Engine Plan already has this shape: nodes that "run the engines", an ordering service and storage
nodes. StarRocks, the model for the earlier split, plans in its own frontends and executes elsewhere; here planning and
execution stay together, as in TiDB's stateless SQL layer.

### Who talks to whom

| From | To | Over | For |
| --- | --- | --- | --- |
| Client | Compute | Binary protocol; TLS 1.3 or declared clear text; SCRAM against the system group's principals | Every request |
| Compute | Storage partition | Binary protocol, role commands | Writes and commits relayed with their bodies unchanged; scans and frontier requests |
| Compute | System group | Binary protocol, role commands | Join, the placement map, catalog versions, logins and sessions, a request to close an epoch |
| Storage partition | System group | Binary protocol between nodes, messages inside one | Placement, catalog versions, epoch closes by subscription, sequence leases, reports |
| Storage partition | Storage partition | Binary protocol between processes, messages inside one | Registering a reference at an entity's home; reverse reads fanned out |
| Compute, storage | Config service | HTTP/1.1 with a read key | Settings at start and at a load |
| Operator, orchestrator | Every process | HTTP/1.1 | `/health`, `/ready`, `/admin/load-config` |

The data plane speaks one protocol, the client's. The control plane speaks a minimal HTTP/1.1.

## Clients: discovery, tags and load

**A client starts from seed addresses and learns the rest from the cluster** (decided 2026-11-12). The cluster keeps
its own books, so the system group knows every compute node at all times, and no service beside the cluster holds a
list. Kafka, Cassandra, ScyllaDB, MongoDB and Neo4j drivers find their clusters this way.

1. A client is given seed addresses: one or a few compute nodes, or a name that resolves to several. Seeds serve the
   first contact alone.
2. Once the connection has authenticated, the compute node sends it the **node list** and its **version**. A peer
   that has not authenticated learns nothing of the cluster.
3. The developer picks a tag, and the driver spreads requests over the nodes carrying it.
4. When membership changes the version rises, and every connected client replaces its list.

### The node list

| Field of an entry | Meaning |
| --- | --- |

| Advertised address | The address a client reaches the node at, which may differ from the one it listens on. The node's certificate names it |
| Tags | The groups the node belongs to |
| State | Serving, or draining before it stops |

A compute node states its advertised address and its tags in its settings and names them in its Join. The system
group records them, and every compute node serves the list from what the system group holds.

**The list version** is one number for the whole cluster: the epoch of the close that announced the list's latest
change. It rises with every change of a member, a tag or a state, so two lists from any two nodes compare, and it
needs no state of its own.

- A node that learns a new version tells its connected clients.
- A client replaces its list whole, and only by a higher version. An equal or a lower one is dropped.
- Connections to nodes the new list still names stay open. A node the list names no longer, or names as draining,
  gets no new request, and its open requests finish.

### Tags pick the node, the classification picks the path

A tag names a group of compute nodes, and a node carries one to eight. Tags are an open set that an operator
extends. A cluster starts with three:

| Tag | Machines | Serves |
| --- | --- | --- |
| `query` | Ordinary | Queries and transactions. The tag a client uses when it names none |
| `high_memory` | Large memory | Algorithms that need much memory and run better on CPUs |
| `gpu` | With GPUs | Algorithms accelerated by GPUs |

The developer makes the routing decision (decided 2026-11-12): a client handle is opened for a tag, and the driver
spreads its requests over the nodes that carry it. Each group scales by itself, and a long algorithm takes no core
and no memory from an interactive query.

Tags choose the node and nothing more. On the node, the query classification chooses the path as it does for every
request: the read lane, the commit path, and what a request pays for versions, pins and read sets. Routing adds no
class to it.

### A wrong tag is refused, and the refusal names the right one

A developer will send an algorithm to a node that cannot run it. Three checks catch that, and each says what to do.

| Check | Where | What the developer reads |
| --- | --- | --- |
| The tag exists | In the driver, before anything is sent | No node carries this tag, and the tags the cluster has |
| The node can run the request | On the node, when the query is compiled and before any work | The procedure, the tag it needs and the tags this node carries: select that tag |
| The node is what its tags say | On the node, at start | A node tagged `gpu` with no device stops with a configuration error |

Every procedure declares the tag it needs in the procedure registry, which is what the second check reads. A Prepare
runs the same check, so a prepared statement fails before its first Execute. The driver hands the refusal to the
developer and moves the request to no other node: a wrong tag is a mistake to correct in the program.

### Load: credit to send, signals to choose

No client sees what the others send, so no rule a client follows alone keeps a node from being overrun: whatever the
rule, all clients run it at once. Two mechanisms close that gap, and they answer different questions (direction
decided 2026-11-12; neither is built). Both papers were read in full. Breakwater covers one server and Prequal no
admission, so their combination is this design's own, and no published work was found that makes it.

**How much a node accepts: credit, after Breakwater.** The node is the one party that sees all of its clients, so it
does the coordinating they cannot do.

- A client sends a request only against a credit of the core that serves its connection. A credit is one request.
  The Welcome's `max_streams` stays the connection's cap.
- The core keeps a pool of credits and moves it once per round trip by its measured queueing delay. Below the target
  delay it adds a step that grows with the number of its clients. Above it, it cuts by a fraction proportional to the
  overshoot, by half at most.
- Credits ride on replies, and a request carries how many requests wait behind it at the client. From that the core
  estimates demand and issues more credit than its pool holds, because a credit at an idle client is capacity nobody
  uses. It takes credit back by a negative grant.
- A request that arrives while the queueing delay is above a threshold is refused before any work. That refusal is
  the hard bound, and credit keeps it rare.
- A client without credit queues its request, and the request expires at its deadline.

**Which node a client picks: signals, after Prequal.**

- **Two signals,** measured at the node over all of its clients: the core's requests in flight, a counter, and its
  latency, the median of recent requests that arrived at about that many in flight. A refused request counts in
  neither, or a node that fails fast looks idle and draws every client.
- **Carried on every reply, and fetched by a probe.** The protocol gains a probe that returns the two signals, sent
  to a node chosen at random at least once per request, and after an idle time. The driver keeps a pool of at most
  16 recent answers, each expiring with age.
- **The hot-cold rule.** An entry is hot when its requests in flight lie above a quantile of the values the driver
  has seen lately. With every entry hot the driver takes the fewest in flight. Otherwise it takes the lowest latency
  among the cold. Entries of nodes where it holds no credit are left out.
- **Keeping the pool honest.** The driver raises its own copy of a node's requests in flight for each request it
  sends there, and with each request it drops the oldest entry or the worst in turn, so the pool never fills with the
  nodes it avoids.

Everything is per core. A connection belongs to one endpoint core, so the credit, the requests in flight and the
latency are that core's.

**What the papers measured.**

| Paper | Setting | Result |
| --- | --- | --- |
| Prequal | YouTube for over two years and twenty more services at Google, in place of weighted round robin; requests from milliseconds to hours | Tail latency halved, tail requests in flight down five to ten times, errors from imbalance nearly gone. Insensitive to the probing rate down to one probe per request |
| Prequal's comparison of rules | A testbed of 100 clients and 100 servers | The hot-cold rule beat every linear blend of the two signals. Rules on a client's own count of requests in flight did far worse than rules on the server's count, since other clients load a server too. A server's count polled every half second was too stale |
| Breakwater | One server, 1,000 to 10,000 clients, requests of microseconds | Stable within 20 milliseconds when demand jumped to 1.4 times capacity, where two earlier schemes collapsed for half a second and more. A window scheme with a floor of one request per client lost nearly all its throughput at 10,000 clients |

**What reasoning and the papers settle.**

| Question | Reasoning | Result |
| --- | --- | --- |
| Can the drivers' choices overrun a core, however alike they choose? | The core refuses a request that arrives while its queueing delay is above the threshold, and it senses that delay itself with no network between. Credit decides how often that happens and never whether the bound holds | Safety is the core's own rule and no tuning: the work waiting at a core stays within the threshold. Clients choosing alike cost refusals and never collapse. Proved in Lean with the protocol |
| May a connection keep one slot always, so its first request after idling costs no round trip? | With a floor of one, every idle client may send at once: as many requests as connections. A core's delay target admits only its service rate times that delay, a few dozen. Breakwater measured the collapse | No floor. An idle client holds what the core speculated for it, which may be nothing. Its first request then waits for a credit or is sent and may be refused |
| Can the two loops drive each other? | A loop oscillates on a delayed signal. The credit loop senses and acts on one core. The choice loop acts on signals that may be stale, and only where the driver holds credit. Below capacity credit binds nowhere and the choice governs alone, which is Prequal's production case. Above it credit binds everywhere and the choice decides little | Each regime has one loop. Between them, with demand near capacity, nodes can pass load back and forth, bounded by the refusal rule. Its size is measured |
| A point read and a long scan differ a thousandfold. What does a credit count? | Queue length misleads when costs differ, and queueing delay does not, which is why Breakwater senses delay. A node's work is also bounded unit by unit: a read's work per turn, a frame's size, the frames a stream sends before the client grants credit, a transaction's life | A credit is the right to open a stream. Three meters, two in the protocol today: credit to open, the stream's credit to continue, a job slot to start an algorithm |
| A transaction stays on the node it began on | Its state sits on the connection it began on. The driver chooses when a transaction begins and for every one-frame request. `transaction_ms` and a Begin's deadline bound how long it holds its place | The unit of choice is the stream, and the protocol's limits bound a held place in time |
| A node the driver has not heard from keeps its last figures | A stale good figure draws every client and a stale bad one starves the node. Replies refresh only the nodes a driver uses, which are the ones it already favours | Probes of random nodes, entries that expire, and the worst entry dropped in turn. Prequal found replies alone too little |
| One principal opens many connections | Credit per connection rewards opening connections. Breakwater shares credit by demand, lazily, as it sends each reply, and admits it is unfair when credits are fewer than clients. Equal shares per principal can be kept by one core, which knows each connection's principal. A share across the cluster needs an account every node agrees on | Shares per principal on a core: this design's own addition, to be checked. A quota across the cluster cannot be had without coordination, and is a later design |

**Where this design leaves the papers.**

- **A request here costs milliseconds.** Breakwater's cost microseconds, where refusing a request costs as much as
  serving it. Here a refusal before any work is cheap, so the hard bound is affordable, and what credit buys is the
  round trip a refused request would waste.
- **Thread per core.** Breakwater gives each request a thread of its own, because a request holding a core delays the
  others and with them the signal. Here a request runs on its core, so the bound on work per turn is what keeps the
  queueing delay a true signal.
- **Several nodes.** A driver holds credit at several nodes and spends it at one. The credit at the others idles, and
  each node's speculation has to absorb that. Neither paper tested it.
- **Few nodes.** Prequal's protection against clients choosing alike rests on each client seeing a small random part
  of many servers. A cluster of three compute nodes has no small part, and the refusal rule is what protects it.

**What only the simulator and real machines settle.**

- The swing between nodes when demand sits near capacity, and the credit that idles at nodes a driver does not pick.
- The values: the target delay and the refusal threshold, the step and the cut, the quantile of the hot-cold rule,
  the probing rate, the pool's size and an entry's lifetime. The papers give starting points: a refusal threshold of
  twice the target, a quantile between 0.6 and 0.9, one probe or more per request.
- Whether the queueing delay stays a clean signal on a core that also runs long merges.

**What it does not do.** Balancing compute balances no partition. Every compute node routes a subject to the same
partition, so a partition that is full is full from every node. A request bound for it is refused with a retryable
answer that names the partition, and relieving it is placement's work.

**Rules that stay beside it.** Each answers a failure other systems met in production.

- A client keeps the connections a new list still names. It opens new ones after a random delay and reconnects with
  exponential backoff and jitter, and a node bounds the connections it sets up at once.
- A driver retries within a budget, and after the time a refusal names.
- A node about to stop is listed as draining first.

## Consistency and coordination avoidance

**The guarantee (decided 2026-11-01).** Transactions are strictly serializable within a partition. Across partitions
they are serializable at a closed epoch, the default, with session tokens for read-your-writes.

### One owner per fact

An entity's facts live in the partition owning its bucket, and every fact stays with its writer (Shard-per-Core note,
P24). The subject's partition writes an edge's forward and reverse entries, its indexes and its statistics in one
local commit. The object's home keeps a referrer set: the bucket groups that may hold edges to it. A reverse read
consults that set and fans out. A reference to an entity another partition owns registers at the entity's home before
the first write that names it. Nothing is derived in one partition and applied in another, so no change stream runs
between partitions, in the KV store or in the tile store.

### Principle: coordinate only where an invariant could break

*Invariant confluence* (Bailis et al., VLDB 2015) is the criterion: operations run without coordination exactly when
merging concurrent executions always preserves the invariants. The idea stayed in research prototypes because
developers had to state their invariants by hand. UltraGraphDB's schema declares them, so the classification is
computed when the schema compiles, stored in the catalog version, and applied per request.

| Write | Invariant at stake | Path | Runs from |
| --- | --- | --- | --- |
| Insert an edge of an unconstrained relation | Referential integrity | Coordination-free: one local commit in the subject's partition | Stage 5.5 |
| Create an entity | ID uniqueness | Coordination-free: sequences leased per type and partition | Stage 5.5 |
| Update text indexes and statistics | None, derived data | Coordination-free: in the same local commit | Stage 5.5 |
| Set a `@key` or `@unique` value | Uniqueness | Segmented: the partition owning the value's bucket checks | Local in stage 6.6; across partitions in stage 8 |
| Insert into a cardinality-constrained role | Cardinality | Segmented: checked where the constrained endpoint lives | Local in stage 6.6; across partitions in stage 8 |
| Change a bounded numeric value | Threshold | Segmented by escrow: each partition holds a share of the bound | Stage 8 |
| Insert into an `@acyclic` relation | Acyclicity | Segmented by rank within a colocated hierarchy; coordinated across hierarchies | Stage 8 |
| Delete an entity | Referential integrity | A tombstone at the home; sequenced shares fence it at the members of its referrer set | Tombstone in stage 6.6; shares in stage 8 |

The coordinated remainder is ordered by the sequencer and executed deterministically by every partition it names,
after Calvin: no two-phase commit and no distributed deadlock. Until stage 8 a request reaching a second partition is
refused with `CrossPartition`.

A request declares its type by its command: read-only, read-write, DirectWrite and ConditionalWrite, and Ingest with
the tile store. The tile store turns the classification into lanes: a read takes lane L, S or X, and a write takes
path A, V or C in the commit step, so append-only and immutable data pay for no version copies, no pins and no read
sets (`../notes/UltraGraphDB Scalable Tile Store.md`, T9).

### Read levels

| Level | A read-only transaction's reads | Waits |
| --- | --- | --- |
| Snapshot, the default | One snapshot at a closed epoch across every partition | Until each partition asked has closed the epoch |
| Latest | The latest state of one partition | None |
| Monotonic-fast | Each read at one closed epoch, never below the session's floor; exact for append-only data | Only for a partition behind the floor |

Non-monotonic queries run at snapshot level. All three levels are in the binary protocol; GQL and SPARQL expose them
as implementation-defined extensions.

### Epochs

Cross-partition snapshots are sequencer epochs. A timestamp service would put a round trip and one leader on every
commit, and hybrid logical clocks depend on bounded clock skew.

1. The sequencer closes an epoch every few milliseconds and delivers the close to every partition. Each close records
   the sequencer's wall-clock time, never below the previous one, so `as of time t` resolves to an epoch and ordering
   never reads a clock.
2. A partition stamps each commit exactly one above the latest close delivered to it and never advances on its own.
3. A partition has closed epoch *e* once it has received the close and applied every round stamped at or below *e*.
4. A snapshot is an epoch: a read at *e* reads each partition once it has closed *e*.

A request carries the highest epoch its session has seen, and a partition holds a commit until the close below that
epoch has reached it, so a causally later write never falls in an earlier snapshot. A commit returns a session token
of partition, log position and epoch. A snapshot read waiting on the open epoch asks the sequencer for an early
close.

### Prior art and lessons

| System or work | What it showed | Lesson applied |
| --- | --- | --- |
| RedBlue / Gemini (OSDI 2012) | Synchronized and asynchronous operations, labelled by hand | Derive the labels from the schema |
| Coordination avoidance (VLDB 2015) | 25-fold TPC-C throughput by coordinating only where needed | Classify every write |
| Lucy (VLDB 2019) | Segmented confluence | Segment by constraint ownership |
| Hamsaz (POPL 2019) | A conflict graph gives minimal synchronization | Analysis at schema compile time |
| IPA (VLDB 2019) | Operations changed so concurrency cannot break invariants | Declared delete semantics |
| Homeostasis (SIGMOD 2015) | Treaties let sites run independently within bounds | Escrow for numeric bounds |
| RAMP-TAO (VLDB 2021) | 1 in 1,500 batched reads saw a partial transaction | Read atomicity built in: closed epochs |
| Cassandra materialized views | Derived copies drifted: no resynchronization, unbounded lag | One owner per fact; nothing derived across partitions |

## The system group

The system group holds two state machines over ordered logs, the metadata state machine and the sequencer, neither
with I/O of its own. Both stay in this one group at every cluster size (decided 2026-11-12): the metadata state
machine learns each catalog version's effective epoch from the sequencer's close, so the two fail over together.

- **Membership:** which processes exist and where each partition listens.
- **Placement:** bucket ranges per partition, tenant runs and bucket groups, each layout from the epoch it applies at.
- **Identity:** sequence blocks leased per type and partition. A system ID is 128 bits: a 16-bit type prefix, a 64-bit
  sequence, a 24-bit bucket and 24 reserved bits.
- **Versions:** catalog versions, with the classification and the compiled layout each carries.
- **Principals and sessions:** the principals clients authenticate against, and the registry of sessions that makes
  each commit exactly-once.
- **Epochs and order:** the sequencer's closes, and from stage 8 the order of work that spans partitions.

**It stays small.** Placement is a function of the bucket in every ID and a short map of ranges. Sequences are leased
in blocks. A snapshot is an epoch. So membership, the placement map, versions and sessions are all that needs
agreement.

**No outside coordination service.** Clusters of the last generation asked a separate service two questions: who
leads, and what the configuration is. Here each replica group answers the first for itself (*Replication and
availability*), and the system group answers the second, replicated by the protocol the data uses. One implementation
of replication, one failure detector and one kind of log serve both. Kafka, ClickHouse and Redpanda removed their
outside services the same way, CockroachDB stores its metadata in replicated ranges, and TigerBeetle runs with none.
What remains is the vote itself: surviving one failure takes three members.

**What waits while the system group changes its primary.**

| Waits | Goes on |
| --- | --- |
| Epoch closes, and with them snapshot reads across partitions | Commits through no session, within each partition's open epoch |
| Commits through a session, which wait on their Check until backlog item 13 is answered | |
| Logins, new sessions, schema installs, placement changes | Reads of one partition's latest state, monotonic-fast reads |
| New sequence leases | ID allocation from the blocks leased ahead |
|  | A partition's own failover, which never asks the system group |

**It acknowledges from disk,** since a lost lease reissues an ID and a lost close rewinds epochs.

**One check is on every commit's path.** Compute checks each commit's session and principal against the registry
before it routes it, so that no partition takes an evicted session for a new one. With the system group on storage
that check is a round trip per commit unless it is answered differently. Two answers are weighed in *The path*: a
check compute holds up to a fenced epoch, and the session's state kept where the commit is decided, in its partition
group's replicated state, as TigerBeetle keeps its client table.

## The storage layer

### Partitions and the tile store

A storage node gives each core a partition. A partition is the one owner of its subjects' facts and holds its own log,
commit step and engine. The engine is the tile store (`../notes/UltraGraphDB Scalable Tile Store.md`, T1 to T48):

- **Tiles of three shapes:** adjacency blocks of a relation's sparse matrix, column chunks of a type's attributes, and
  sorted record runs for dictionaries and small indexes. Each is an immutable, checksummed base with a delta in memory.
- **The log is the only stream synced per commit.** Deltas fold into new tiles written out of place, and a checkpoint
  publishes them under a superblock written last.
- **The schema compiles the layout** and gives each relation and attribute a storage class.
- **A grid of extents per replica.** A tile reference is address, length and checksum. An extent holds whole tiles and
  is freed whole.
- **Compaction and placement are deterministic.** The grids of a partition's replicas are identical byte for byte at
  every checkpoint, so a tile is repaired or copied from a peer by address and checksum.
- **Shared nothing per core.** The engine does no I/O of its own: it asks its shell to read a tile, write an extent or
  sync, and the simulator answers the same requests with faults.

The GSL levels are: G the replica group, S the bucket group, which is the unit a move carries, and L the tile.

### Two bindings

The engine is one. Its grid's shell binds it to a medium, and a cluster chooses the binding in its settings
(decided 2026-11-12).

|  | Direct-attached | Object store |
| --- | --- | --- |
| Authority for tiles | The replica's grid on local NVMe | The object store |
| The local grid | Holds every extent | Holds every open extent, every extent of the manifest and hot streams, every extent not yet published, and as many published cold extents as the node keeps |
| The log | Local, synced per round | Local, synced per round |
| A commit is acknowledged | Once its round is durable in the log; from stage 8, committed by its group | The same. A commit never waits on the object store |
| Data is bounded by | The local drive | The object store, with the local drive bounding the cache, once the grid's size under this binding is decided (review of 2026-11-12, U1) |
| Object-store credentials | None | Held by storage alone |
| A lost node recovers | From its group's replicas, from stage 8 | Tiles from the object store; the log's tail from its group's replicas, from stage 8 |
| Suits | Lowest latency, on premises, air-gapped | Elastic cloud, large data, low storage cost; MinIO or Ceph on premises |

In the object binding the storage node remains a full participant: under one owner per fact a partition executes
transactions and decides conditional writes, so it reads the data it decides on and keeps a tile cache.

**The paths in the object binding.**

- **Write.** A round is logged, synced and acknowledged as in the direct binding. A checkpoint closes extents; closed
  extents are uploaded, and then the checkpoint's record, last, so a record names only objects that exist. The
  checkpoint counts as published once its record is there. A published extent of the cold stream may leave the local
  grid.
- **Read.** A read of an extent the node holds is a local read. A read of a punched extent fails its tile's check and
  is repaired from the object store: the extent is fetched by the key its extent record gives, verified by the
  checksum in that key and written back at its address. The read yields while the fetch runs, as it yields today
  while a tile loads.
- **Cold reads pay the object store's latency at every hop.** A frontier request names the next hop's sources, so
  their extents are fetched in parallel; hot extents are pinned.

**What the tile store's specification holds for it.** These are specified in `specification/storage/STORAGE.md`
sections 12.5, 17.4 and 17.7 (2026-11-12), before the grid is built.

1. **One engine, two shells.** Nothing above the grid's shell knows the binding.
2. **An object is a closed extent, and its key outlives the address.** An address is reused after an extent is freed,
   so the key carries the extent's number and the checksum of its placed blocks. Both come from the extent's record,
   which the manifest's checkpoint holds in both bindings (record kinds 21 and 22).
3. **Allocated is apart from held.** Which extents exist is a function of the log, identical on every replica. Which
   extents a node holds is a local fact that no logged decision reads and no durable record states: a punched extent
   reads as zero, fails its check and is repaired by a fetch, the path that repairs a damaged tile.
4. **Timing never reaches the log.** An upload, an eviction or a fetch changes when a read answers and never what a
   round decides, so the replicas' grids stay identical wherever they hold bytes.
5. **Retention follows published positions.** A partition retains its latest published checkpoints by count, and an
   object is removed when the last record naming it expires, so a reader on another machine never finds a reference
   dangling.
6. **A checkpoint is published as a record,** the superblock's bytes with the records of the extents the checkpoint
   itself closed, keyed by its log position. The record with the highest position is current.
7. **Identical grids make one upload enough.** Any replica's extent is the same bytes, so an upload is idempotent and
   an object verifies against any replica.
8. **The object store sits behind an interface with a simulated implementation.** The fault simulator injects
   latency, lost acknowledgements and partial failure. A real S3-compatible store appears in container tests alone.
   The same interface hides the client: a dependency first, a client of the project's own later (decided
   2026-11-12), and the engine sees neither.

**What the object binding promises with one replica.** Tiles survive the loss of the node up to the last published
checkpoint. The log's tail above it is on the node alone. That is a restore to a checkpoint, and it is named so. From
stage 8 the log is replicated, and the object binding then loses no acknowledged commit to a single failure.

### Reads and multi-hop execution

Compute sends frontiers, not vertices: one request names many sources of one relation and direction, with a type
range, inline filters and a limit. Depth beyond two hops stays fast when filters are pushed into the expansion as
range reads, the frontier is deduplicated, a query asks for reachability or an aggregate in place of every path, and
the planner picks the selective end to start from.

Whole-graph analytics read projections: compute builds an immutable two-dimensional projection at a closed
epoch from tiles streamed from storage, and caches it. The store keeps the one-dimensional placement that serves
transactions. A projection is also what a graph algorithm loads.

## Graph algorithms and GPU compute

**A later requirement.** Graph algorithms accelerated by GPUs are added after stage 10 (stated 2026-11-12). Nothing
of them is built before. This section fixes where they run and what the earlier stages keep open for them.

**They run in compute.** Compute takes the client's request and executes it, so a graph reaches an algorithm straight
from storage and its result leaves straight to the client. With execution in a role of its own, the graph would move
from storage to that role, the result back to the endpoint, and nothing could stream.

| Pathway | What flows | Where the answer goes |
| --- | --- | --- |
| Query | Rows from the partitions, merged | Streamed to the client |
| Algorithm | A subgraph loaded into flat arrays, on a GPU where the node has one | Rows streamed to the client |
| Write-back | An algorithm's result for every vertex | Into the graph as an attribute, through bulk ingest |

**Loading is what streams.** An algorithm such as PageRank or community detection needs its whole subgraph before it
iterates, so batches fill its arrays as they arrive from storage. A graph larger than memory is processed batch by
batch, by bucket group, the unit tiles are cut by. Work per row, such as scoring vectors, runs on each batch as it
arrives.

**Compute nodes with GPUs are a group by tag.** A GPU node is a compute node: the same role and the same binary, on a
machine with GPUs, tagged `gpu` (*Clients: discovery, tags and load*). Nodes tagged `high_memory` take the algorithms
that need much memory and run better on CPUs.

- **The query path stays clear.** A long algorithm takes no core and no memory from an interactive query, however
  the cluster's load grows.
- **A failure stays in its group.** An algorithm that exhausts memory drops no interactive client.
- **No role of its own.** A GPU role would bring back the transfer between processes that one compute role removed.

The developer picks the tag, and an algorithm procedure declares the tag it needs, so a call that reaches a node
without it is refused with the tag named.

**What the earlier stages keep open.** Each item costs little now and is expensive to retrofit.

| Stage | Keep open | Because |
| --- | --- | --- |
| Stage 5.9 | Work that outlives a turn runs off the endpoint core | An algorithm runs for minutes and must hold up no relay of a commit |
| Tile store | Tiles, slot maps and identity columns streamed to compute for a projection carry slots with their bucket group | An algorithm needs vertices numbered densely. The wire carries 128-bit IDs, and renumbering billions of them through a hash map is the load's largest cost. Decided 2026-11-12 (`STORAGE.md` section 14.6): this is the one place a slot leaves its replica. The projection is immutable, held in memory, named by its checkpoint identity, and no answer, resume key or durable state names a slot |
| Tile store | A source that holds for minutes: a published checkpoint, whose tiles never change | Retention is set for interactive reads. The retention and manifest rules of the object binding count such readers |
| Tile store | The format's decoders linkable by compute | The format crate does no I/O. The rule that compute links no storage engine admits it by name |
| Query core | Operators pass columnar batches of IDs, and text is resolved in the last step | A client's result names every node by its text; an algorithm plugs in as one more operator over IDs |
| Query core | Procedures are a registry | `CALL … YIELD` exists for vector and text search, and an algorithm takes the same surface |
| Query core | A procedure declares the tag it needs | The refusal of a wrong tag reads it, and names it to the developer |
| Stage 5.9 | Join names a compute node's tags and advertised address, and a `Nodes` frame after authentication carries the node list and its version | Join, EpochClose and the new Nodes and Roster are edited once, with the merge's edit of the protocol |
| Stage 8 | The tile fetch of a replica's catch-up serves compute too | A frontier reply carries 32 bytes an entry, right for interactive reads. A projection takes whole tiles by address and checksum |
| After stage 10 | The GPU binding sits behind an interface in a crate of its own | It needs foreign code, and the workspace forbids `unsafe` |

## Replication and availability

**The target.** Any one node may fail and the cluster keeps serving, at reduced performance. Three things give it:
several compute nodes that all take clients, the system group as three replicas, and every partition as a group of
three replicas with its leaders spread, so every storage node leads a third of the replicas it hosts.

**The protocol** is `UltraGraphDB Distribution Protocol.md`: Viewstamped Replication over the log's interface. A
partition's group acknowledges from memory on a quorum with a bounded exposure. With one replica lost the group runs
guarded: the two that remain sync each round before they acknowledge. That is the reduced performance of the target.
The system group acknowledges from disk.

### Failover is decided inside the group

These rules are Viewstamped Replication's, as TigerBeetle runs them in production with no outside service
(read in its sources, 2026-11-12).

- **The primary is arithmetic.** A group's members are an ordered list, and the primary of a view is the view number
  modulo the list's length. No election runs and no process records who leads.
- **A view changes only by a quorum.** A backup that stops hearing its primary asks to leave the view, and the view
  changes once a quorum asks. A replica that can send but cannot hear therefore deposes no healthy primary. A primary
  that cannot gather a quorum stops leading.
- **Compute is each group's client.** The placement map names a group's members. Compute learns the group's view from
  them and sends to the primary; a member that is no primary forwards the request or answers with its view.
- **A group's members are fixed.** The list is settled when the group is formed. A cluster grows by forming new
  groups and moving bucket groups onto them, and a lost node is replaced under its place in the list. TigerBeetle runs
  on fixed members: its reconfigure operation validates a request and changes nothing.
- **A version switch goes through the log.** A replica advertises the versions it holds, and the primary commits the
  switch once every member holds the new one.

| Members | To commit | To change the view |
| --- | --- | --- |
| 3 | 2 | 2 |
| 6, TigerBeetle's recommendation | 3 | 4 |

**Where this design leaves TigerBeetle.** TigerBeetle persists its protocol state before it answers, so a restarted
replica forgets no promise, and it rebuilds a replica whose disk was lost two views ahead for the same reason. This
design acknowledges from memory in its fast mode. The forgotten promise is therefore the case its proof and its
simulation carry (experiment X3 of the protocol note). TigerBeetle also orders every read through its primary; here a
replica that has closed an epoch answers reads at that epoch.

**One primary per partition.** Several replicas of one partition accepting writes at once would give up strict
serializability and the single commit step, and the target does not ask for it.

**Until stage 8** every group has one replica behind the interface that replication fills: the system group's two
state machines, each partition and the config service. A partition's directory already names its replica.

**Stage 8** builds VSR in one container, on groups of three whose replicas share a machine. It proves the protocol.
**Stage 10** puts the replicas on separate nodes, and the target is met there.

What already serves the target: a session lives in the registry and in each partition's session table, a commit in
doubt is resolved on the next connection by asking the partitions, and a partition keeps stamping its open epoch
while the sequencer is silent.

## Runtime model

- **Thread per core on compio.** An endpoint core per CPU in compute, a runtime per partition in storage, each
  owning its listener and its ring, pinned where the platform allows.
- **No I/O in the core logic.** The state machines, the engine and the transport's framing take I/O through traits;
  their shells are compio and the deterministic simulator.
- **Zero copy on the wire and in the log.** A frame is read in place. Compute relays a write's body unchanged,
  and a round's bytes are what the log stores and what replication sends.
- **Batching amortizes agreement.** Commits batch into rounds and reads into frontier requests.
- **Settings come from the config service.** A role names its target and instance and fetches its settings at start.
  A load is a restart at a named revision.
- **Fail-stop on a violated invariant,** with an exit code that names the cause.

## Deployment and node runtime

Deployment targets are Kubernetes and Docker, bare metal later (decided 2026-11-01).

| Process | Image | Under Kubernetes |
| --- | --- | --- |
| Compute | glibc with mimalloc on the Wolfi base | A Deployment; a StatefulSet with a volume while it still hosts the system group |
| Storage | The same | A StatefulSet with a volume per instance, the system group's logs on three of them |
| Compute, tagged `gpu` or `high_memory` | The compute binary; with the GPU binding after stage 10 | A Deployment per tag on machines of that kind |
| Config service | Static musl on scratch | A StatefulSet of one replica |
| Development container | The Wolfi base with every binary and the supervisor | One container, roles joined over Unix-domain sockets |

Every role runs as a non-root user, read-only, under a seccomp profile that allows io_uring.

**Tracked options, none built.** A static musl storage binary on a minimal Linux kernel in a microVM; io_uring
restricted to the operations compio uses; a resolver of the project's own in place of the C library's; a build that
fails on an unapproved C library symbol. The object binding makes name resolution and trust roots for an outside
endpoint a requirement of storage, which moves the resolver from an option to a question in *The path*.

## Security by tier

| Tier | Exposure | Controls |
| --- | --- | --- |
| Compute | Client-facing parsers and protocols; the largest code surface; no durable state | TLS 1.3 and SCRAM in the handshake; every inbound frame bounds-checked before a view is handed out; clear text only as a declared setting |
| Storage | The durability boundary; holds all data, the system group with the principals' verifiers and, in the object binding, the object store's credentials | No client reaches it; role connections authenticated; the object store's write credentials on these nodes alone |
| Config service | Every target's settings; never a secret's contents | A read key and an admin key, each node verifying the admin key by its digest |

Roles authenticate each other with one shared role secret over connections in clear text. Stage 8 gives each role a
certificate and encrypts role connections.

## Rolling upgrades

Storage upgrades first, compute last, and new formats activate only after a cluster-wide finalize step.

- **Storage,** one replica at a time per group, the system group's among them, never more than a group can lose,
  waiting for catch-up.
- **Compute nodes,** drained and replaced.
- **The state-machine version is agreed through the log.** A binary holds the current and the previous logic, and the
  switch is a committed operation, so every replica changes at the same point in history. The primary commits it
  once every member advertises the new version.
- **Binaries first, features later.** A finalize step committed through the system group enables new tile formats,
  plan features and protocol fields. Before it a downgrade is a binary rollback; after it the change is one-way.
- **Readers handle the current and the previous format,** and the protocol negotiates one version back.

Storage goes first because it holds the durable formats. Compute nodes go last so the planner emits a new feature only
once every node understands it.

## Verification strategy

- **Formal models first, in Lean:** `../../lean/PlacementModel` for epochs, registration, referrer sets and deletes,
  `../../lean/RoundModel` for rounds and checkpoints; the replication protocol's model before stage 8 builds it.
- **Classification soundness:** for every constraint the schema can express, a write classified coordination-free or
  segmented preserves its invariant under every interleaving the simulator generates.
- **Deterministic simulation:** faults, crashes, partitions of the network and mixed versions, compared against a
  reference run; two simulated replicas byte-identical under every fault class; the object store's faults among them.
- **Conformance on every build:** W3C SPARQL, the openCypher TCK and the GQL scenario suite, against clusters.

## The path

What each stage builds, what must be settled before it starts, and the blind spots it has to close. The order is the
roadmap's (`../../ROADMAP.md`, set 2026-11-12). Stages 5.8, 5.9 and 6.5 each change what the next builds on, so none
starts before the one before it is complete.

### 1. Stage 5.8: the system group moves to storage

The first of the three. The frontend and compute still exist as two processes. After this stage both dial the system
group on storage, compute joining it and the frontend reading its members, the frontend holds nothing durable, and stage 5.9 merges two stateless processes.

Settle before it starts:

- **The protocol for both stages.** Stages 5.8 and 5.9 both change the role sections of `PROTOCOL.md`. Both edits
  are specified together here, so the consistency review of sections 5 and 11 to 13 is reopened once.
- **The session check** is decided: the registry, the check and the mark of a commit in flight stay together in the
  system group, asked by a role command, one round trip per commit batched per turn, until backlog item 13 is
  answered. One hop between two virtual machines measured about half a millisecond
  (`../benches/benches-2026-11-09.md`).
- **Where the group runs on a storage node:** its core, and whether its logs share a flush with the partitions' logs.
  The commit measurements found the sequencer's sync at each close delaying the checks that waited behind it.
- **How the other processes find it.** A fixed, ordered list of addresses as one setting, as TigerBeetle's replicas
  and clients take one. The settings format needs no lists for that.
- **How the client's endpoint follows epochs.** It learns the latest closed epoch and asks for an early close in
  process today.

Blind spots to close:

- **The start order reverses.** Storage starts first and the others attach. The supervisor's order, the readiness
  rules and the Helm chart follow.
- **The 60-second exit ends for storage.** Storage depends on no other process. The others decide what they do while
  storage is unreachable.
- **Settings and secrets move:** the bootstrap administrator's secret, the epoch bounds and the session limits leave
  the frontend's tables for storage's.
- **The data directory.** The metadata and sequencer logs move to a storage volume.
- **Describe** answers from the metadata state machine in process today and becomes a call.
- **Membership is the system group's.** It answers Join and Members. The node list a client receives, its version
  and its tags are stage 5.9's.
- **A baseline of its own.** Reads are recorded before this stage, since the benchmark record holds writes alone.
  The benchmark is rerun after it, so the round trip it adds to a commit is measured apart from what stage 5.9
  removes from a read.

### 2. Stage 5.9: the frontend and compute become one role

Settle before it starts:

- The bound on read work per turn on an endpoint core, and where work that outlives a turn runs.

Blind spots to close:

- **The protocol, as specified with stage 5.8.** `ComputeRead` retires, and Join and the node list are restated
  for two roles. Join carries a compute node's tags and advertised address, and a `Nodes` frame after
  authentication the node list and its version. The golden frames are regenerated.
- **The rename.** The role is compute and its binary `ultragraphdb_compute`. The frontend's binary, its crate, its
  settings table, the `frontend_address` setting and the role's name in Join take the new name or retire.
- **Two open changes are written against the split.** Nine live specifications are written for three roles, and the
  deltas of stages 6.5 and 6.6 modify some of the same requirements. This stage's change is archived and both open
  changes are rebased before stage 6.5 starts.
- **The baseline for the tile store.** The benchmark is rerun after this stage and before stage 6.5, so the tile
  store's comparison measures the tile store.
- **The read scenarios stay.** Fan-out, credit, resume keys and the read levels keep their tests; the scenarios of a
  hung or lost executing process end with the relay.
- **Everything that names three roles:** the settings checker's roles, the profiles, the supervisor's order, the
  images, the Helm chart, `AGENTS.md` and the backlog. Seven documents also cite this design under `openspec/notes/`.
- **Deletions need approval:** the frontend's binary with its image, its test crate and its chart entry, once their
  contents live under compute's name.

### 3. Stage 6.5: the tile store, with the object binding

Settle before the grid is built:

- The eight requirements of *Two bindings*, as decisions and scenarios of the tile store's specification.
- **Which crate the object-store client is.** It is a dependency behind the object store's interface first, and a
  client of the project's own later (decided 2026-11-12). The interface is drawn so that the swap changes one
  implementation. The crate has to fit thread per core: a client that brings an async runtime of its own needs a
  thread beside the cores, where one that only signs requests leaves the I/O to compio. Pure Rust is preferred.
- Where the object binding lands in the task groups: the interface with the grid, the simulated store with the
  simulator, the real client as a group of its own.

Blind spots to close:

- **Credentials and endpoints.** Where storage reads the object store's credentials, how it resolves the endpoint's
  name and which trust roots it verifies it against.
- **Read timing.** A fetch of tens of milliseconds meets rules written for local reads: the one-turn rule of lane L
  and the scan's stall timer.
- **How a restore from the object store relates to backup.** The restore itself is specified (`STORAGE.md` section
  17.7): an operator's act, to the latest published checkpoint.
- **The benchmark compares like with like:** the direct binding against the KV baseline, the object binding recorded
  apart.
- **Three openings for analytics** (*Graph algorithms and GPU compute*): slots inside the tiles streamed for a
  projection, a published checkpoint as a source held for minutes, and the format's decoders linkable by compute. The
  first is decided and specified (`STORAGE.md` section 14.6).
- **Scope.** The stage is the largest on the roadmap, and the object binding adds to it.

### 4. Stages 6.6 and 6.7: UGS on tiles

The whole of UGS: syntax, static semantics, the catalog version, Install and Describe, then instances, their rules at
commit, indexes, the RDF view, the UQL subset for tests and the client, all on tiles. The RDF view and the UQL subset
land in compute. Stage 6.7's chaos testing includes the object store's faults.

- The catalog version carries the tile store's compiled layout, which stage 6.5 has fixed by then.
- A catalog version is capped at one frame; the schema specification asks for continuation frames at this stage.
- The change's first group, the wire, is done. Its groups for syntax and static semantics add crates that no earlier
  stage touches.

### 5. Stages 7 and 7.5: the query core

The SPARQL and GQL core, planned and executed in compute.

- **The decision point of Dec 31.** One query runs through the whole stack: the endpoint, the query engine and the
  tile store (decided 2026-11-12). It is the point at which to decide whether to continue, and it moves if the work
  needs it. Stages 5.8 to 6.6 and the query core's first slice all come before it.
- Operators pass columnar batches of IDs and resolve text in the last step, and procedures are a registry, so an
  algorithm is one more operator and one more procedure later.
- Every procedure declares the tag it needs, and compilation refuses a call the node has no tag for, naming the tag.

### 6. Stage 8: replication in one container

Settle before it is specified:

- **The session check without a round trip** (backlog item 13), if stage 5.8 left it as a round trip: a check compute holds up to a
  fenced epoch, or the session's state in its partition group's replicated state. Either is proved in Lean against
  exactly-once.
- **The home of a commit in flight.** The system group holds it today, which is what refuses a session's second
  commit.
- **Fixed members.** Whether growth by new groups and replacement under a member's place make changing a group's
  members unnecessary. TigerBeetle offers no model for changing them.
- **Leaders under arithmetic.** The view number fixes the primary. How the preferred leaders of P8 hold after a
  failure heals.
- **How compute learns a group's view,** and whether a member that is no primary forwards a request or answers with
  its view.
- **The forgotten promise.** The proof and the simulation of the memory mode, which TigerBeetle avoids by persisting
  its protocol state.
- **The config service's replicas.**
- **The object binding under replication:** which replica uploads, and whether a follower catches up from the store.
- **Certificates per role** and encrypted role connections.
- **The tile fetch** of a replica's catch-up, shaped so that compute may use it for projections.
- **Load control.** Credit, the refusal threshold, the two signals, the probe and the driver's choice enter
  `PROTOCOL.md`. The core's bound, no request admitted above the threshold, is proved in Lean. The simulator runs
  many clients against list changes, draining and demand near capacity, where the two loops meet.



### 7. Stage 10: separate nodes

Replicas on separate machines, measured on physical Linux. A compute core holds a connection per partition, so the
count grows as compute cores times partitions. The tracked hardening options are decided here.

### 8. After stage 10: graph algorithms on GPUs

Settle before it is specified:

- **The tags of algorithm procedures:** which need `gpu`, which `high_memory`, and the device check at a node's start.
- **Jobs.** An algorithm outlives the deadlines of interactive requests: how it is started, watched, cancelled and
  its result paged.
- **The global numbering:** how slots and bucket groups of many partitions become one dense index, and how a result
  maps back to IDs and to text.
- **The GPU binding and its image:** which interface to the device, and whether nodes tagged `gpu` run an image of their own.
- **Write-back:** the algorithm's result as an attribute through bulk ingest, and the snapshot it is stamped with.
- **Affinity for projections.** Prequal lets a probe carry the request, and a server that already holds what the
  request needs reports a lower load to draw it. A node holding a projection in memory could draw the jobs on it.

## Decisions

| Question | Decision | Date |
| --- | --- | --- |
| Roles on the request path | Compute and storage; the frontend and compute are one role, named compute | 2026-11-12 |
| Storage bindings | Direct-attached and object store, one engine; the object binding enters the tile store's specification | 2026-11-12 |
| The cluster's books | The system group, a replica group hosted by storage under the data's replication protocol; no outside coordination service | 2026-11-12 |
| Where an Install compiles | In compute, which offers the compiled version to the system group; the system group checks the base version, repeats the decode and commits | 2026-11-12 |
| The fence of a first schema | Carried with the epoch closes and enforced at the partitions, so it holds with several compute nodes | 2026-11-12 |
| The catalog's requests | Stage 5.8 specifies two transport commands, `Declare` (538) and `Defaults` (539), which carry what the owner answers in process today. Stage 6.6 specifies the offer of a compiled version at 540. The system group links the catalog's format alone | 2026-11-12 |
| Slots outside their replica | Only inside tiles, slot maps and identity columns streamed to compute for a projection at a closed epoch or a published checkpoint. Never in an answer, a resume key or durable state | 2026-11-12 |
| The object binding's format | Extent records in the manifest's checkpoint in both bindings; an extent's object keyed by its number and the checksum of its placed blocks; a checkpoint record published last; no durable record of what a node holds, a punched extent repaired by fetch | 2026-11-12 |
| Order of the next stages | 5.8 the system group to storage, 5.9 the merge into compute, 6.5 the tile store, 6.6 UGS | 2026-11-12 |
| Replication and availability | Stage 8 in one container, stage 10 on separate nodes | 2026-11-12 |
| Graph algorithms on GPUs | A later requirement, after stage 10; they run in compute, on the nodes tagged `gpu`, with no role of their own | 2026-11-12 |
| Load control | Credit to send, granted by the node from its queueing delay, with refusal above a threshold; the node's requests in flight and latency on replies and probes for the driver's choice. After Breakwater and Prequal. A direction, proved and simulated before it is built | 2026-11-12 |
| Client discovery | Seed addresses for the first contact; the node list with tags and a list version sent once the connection authenticated, replaced whole by a higher version | 2026-11-12 |
| Routing among compute nodes | By tag: the developer picks a tag and the driver spreads over the nodes carrying it. The query classification picks the path on the node and gains no class for routing | 2026-11-12 |
| A wrong tag | Refused when the query is compiled, naming the tag the procedure needs; the driver moves it to no other node | 2026-11-12 |
| The system group's move | Stage 5.8, a change set of its own, before the merge of stage 5.9 | 2026-11-12 |
| Sequencer and metadata state machine | One group at every cluster size | 2026-11-12 |
| The object-store client | A dependency behind the object store's interface first; a client of the project's own later | 2026-11-12 |
| The check of Dec 31 | A decision point on a query through the whole stack, on tiles; movable | 2026-11-12 |
| Placement | One owner per fact; bucket ranges per partition, one partition per core | 2026-11-02 |
| Storage engine | The KV store now; the tile store at stage 6.5 | 2026-11-10 |
| Images | glibc with mimalloc on the Wolfi base; the config service static musl on scratch | 2026-11-01 |
| The guarantee | Strictly serializable within a partition; serializable at a closed epoch across them | 2026-11-01 |
| Snapshots | Sequencer epochs | 2026-10-31 |
| The coordinated remainder | Deterministic ordering after Calvin | 2026-10-31 |
| Formal models | Lean | 2026-11-01 |

## Open questions

- [ ] Does a compute node ever read tiles from the object store itself, with a cache of its own, or does storage alone?
- [ ] Does the config service fold into the system group, or take replicas of its own?
- [ ] Do the sequencer and the metadata state machine share one log inside the system group, or keep two?
- [ ] Does a node tagged `gpu` or `high_memory` also serve regular queries, or refuse them to stay free?
- [ ] How is a quota for a tenant held across the whole cluster, if one is wanted?
- [ ] Does a group ever change its members, or do new groups and replacement cover every case?
- [ ] In the object binding, can a partition run on two replicas, with the object store's conditional writes
  deciding its failover?
- [ ] Do groups larger than three take TigerBeetle's flexible quorums?
- [ ] What are the epoch length's bounds for the adaptive sequencer?
- [ ] What extent size suits the object binding?

## Sources

- `UltraGraphDB Shard-per-Core Write Path.md`: P1 to P24. `UltraGraphDB Distribution Protocol.md`: replication.
  `UltraGraphDB Dual Query Engine Plan.md`: the query core. `../notes/UltraGraphDB Scalable Tile Store.md`: T1 to T48.
  `../../specification/binary_protocol/PROTOCOL.md`. `../benches/benches-2026-11-09.md`.
- [Coordination Avoidance in Database Systems (Bailis et al., VLDB 2015)](https://arxiv.org/pdf/1402.2237)
- [Interactive Checks for Coordination Avoidance (Whittaker and Hellerstein, VLDB 2019)](https://mwhittaker.github.io/publications/iconfluence_VLDB19.pdf)
- [Hamsaz: Replication Coordination Analysis and Synthesis (POPL 2019)](https://mohsenlesani.github.io/companion/popl19/POPL19.pdf)
- [IPA: Invariant-Preserving Applications (VLDB 2019)](https://www.dpss.inesc-id.pt/~rodrigo/vldb-2019.pdf)
- [The Homeostasis Protocol (SIGMOD 2015)](https://infoscience.epfl.ch/record/219208/files/final_camera_ready.pdf?version=1)
- [Keep CALM and CRDT On (VLDB 2023)](https://www.vldb.org/pvldb/vol16/p856-power.pdf)
- [Making Geo-Replicated Systems Fast as Possible, Consistent when Necessary (OSDI 2012)](https://web.eecs.umich.edu/~manosk/assets/papers/gemini.pdf)
- [RAMP-TAO (VLDB 2021)](https://www.vldb.org/pvldb/vol14/p3014-cheng.pdf)
- [Instaclustr: Apache Cassandra Materialized Views](https://www.instaclustr.com/blog/apache-cassandra-materialized-view-instaclustr-support/)
- TigerBeetle at revision `a2a3c7064` (2026-11-10): `docs/internals/vsr.md`, `docs/operating/cluster.md`,
  `docs/operating/recovering.md`, `docs/internals/upgrades.md`, `docs/reference/sessions.md`, `src/vsr.zig`,
  `src/vsr/replica.zig`, `src/vsr/client.zig`, `src/vsr/replica_reformat.zig`.
- [Load is not what you should balance: Introducing Prequal (NSDI 2024)](https://arxiv.org/abs/2312.10172)
- [Overload Control for µs-scale RPCs with Breakwater (OSDI 2020)](https://www.usenix.org/conference/osdi20/presentation/cho)
- [Distributed Dispatching in the Parallel Server Model (Goren, Vargaftik and Moses, 2020)](https://arxiv.org/abs/2008.00793)
- [Stochastic Coordination in Heterogeneous Load Balancing Systems (2021)](https://arxiv.org/abs/2105.09389)
- [C3: Cutting Tail Latency in Cloud Data Stores via Adaptive Replica Selection (NSDI 2015)](https://usenix.org/conference/nsdi15/technical-sessions/presentation/suresh)
- [Viewstamped Replication Revisited (Liskov and Cowling, 2012)](https://hdl.handle.net/1721.1/71763)
- [Protocol-Aware Recovery for Consensus-Based Storage (FAST 2018)](https://www.usenix.org/system/files/conference/fast18/fast18-alagappan.pdf)
- [Flexible Paxos (Howard, Malkhi and Spiegelman, 2016)](https://arxiv.org/abs/1608.06696)
- [CockroachDB: Distribution Layer](https://www.cockroachlabs.com/docs/stable/architecture/distribution-layer)
- [KIP-500: Replace ZooKeeper with a Self-Managed Metadata Quorum](https://cwiki.apache.org/confluence/display/KAFKA/KIP-500%3A+Replace+ZooKeeper+with+a+Self-Managed+Metadata+Quorum)
- [StarRocks: Architecture](https://docs.starrocks.io/docs/introduction/Architecture)
- [StarRocks: Upgrade StarRocks](https://docs.starrocks.io/docs/deployment/manage_deployment/upgrade/)
- [Lupine: A Linux in Unikernel Clothing (EuroSys 2020)](https://people.cs.vt.edu/djwillia/papers/eurosys20-lupine.pdf)
