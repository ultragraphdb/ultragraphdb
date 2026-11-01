# Backlog: what the next change sets must specify

This backlog tracks what remains to be specified after stage 5, whose change
(`openspec/changes/stage-5-transactions-and-binary-protocol`) was reduced on 2026-11-01 to what it built: the posting
format, the write-ahead log, the wire protocol and its transport, the Rust client, read sets, the commit step and the
vector indexer, with the stage's verification of them. The distribution architecture
(`openspec/notes/UltraGraphDB Distribution Architecture.md`) and the single-container deployment
(`openspec/notes/UltraGraphDB Single-Container Deployment.md`) supersede the single-server design stage 5 drafted; the
next change sets are scoped from them, starting from the items below. Each item names what is to be specified, where
the architecture notes speak to it, and the stage-5 material it starts from. That material follows the items,
moved verbatim from the change: the single-server draft as it stood, kept as a record of what was considered, not as a
decision.

## Items

### 1. The roles and their process

The three roles (frontend, compute, storage) from one binary with role flags; each role's threads and runtimes; the
profiles (`dev`, `dev-ha`, `dev-net`, one role per process) and their configuration; the data layout per role and
replica; start, readiness, the shutdown order; health and metrics; bounds; exit codes; the acceptance batteries against
the built binary.

- Architecture: *Cluster architecture*, *Replication and runtime model*, *Node runtime and hardening*; the
  single-container note throughout.
- Source material: the server-node requirements; *Threads and queues*, *Bounds*, *Configuration*, *The data directory
  and the cluster*, *Start and shutdown*, *Health*, *Tests*; tasks 10.5, 10.6, 10.7, 12.2 and 12.7.
- Open: where the Welcome's `max_write_set` and `TxnLimits` take their bound from the log's round size once a role
  builds the Welcome (`RoundBounds::max_write_set`, stage 5's commit step).
- Decided 2026-11-01: the three roles are what "3 node clustering" means; the deployment targets are Kubernetes and
  Docker, bare metal later; CI builds, signs and publishes the OCI images with Bazel's `rules_oci`.
- Operator requirements the roles' specification states (from `openspec/notes/review-stages-5.5-and-6.md`):
  - signals per role, exported through OTLP: the age of the latest closed epoch and its close latency, watermark lag
    per group and per incoming change stream, the share of writes on the coordinated path, escrow-share exhaustion and
    renegotiation, VSR view changes, the background verifier's repairs, the sequencer's queue depth;
  - runbooks for losing a group's majority, losing the metadata group, and a group that cannot close an epoch;
  - a cluster backup as a snapshot of every group at one closed epoch with the metadata group's catalog at that epoch,
    addressed by keys so it restores under any placement;
  - a schema version is never rolled back in place: a later version restores earlier definitions (roll forward), and
    a destructive change after finalize is one-way; a tightening's validation scan reports its progress and does not
    block writes to unaffected types;
  - the coordination report also states how much data a placement change moves and how the move is throttled;
  - storage growth: `@append_only` and `@bitemporal` data is kept until a legal purge, so capacity signals include
    their growth rate;
  - secrets through a typical KMS (AWS KMS, Google Cloud KMS, HashiCorp Vault): TLS keys and certificates per tier,
    and the opaque handle's keys, each rotatable without a restart;
  - the settings an operator tunes (epoch length bounds, round sizes, the MVCC retention window, escrow share sizes)
    listed apart from developer settings.

### 10. Identity and the metadata group (stage 5.5)

The system ID, type prefixes and the state machine that hands both out, decided on 2026-11-01
(`openspec/notes/review-stages-5.5-and-6.md`, D1, D2, D4), to be written as a decision record and specified in stage
5.5's change before anything builds on them.

- The system ID is 128 bits: a 16-bit type prefix, a 16-bit bucket, a 64-bit sequence and 32 reserved bits. A bucket
  is a virtual partition the placement map assigns to a replica group, so rebalancing never changes an ID.
- Type prefixes are allocated in declaration order and never renumbered; a supertype covers its descendants' prefix
  ranges.
- The metadata group's state machine is built in stage 5.5 with one replica, holding catalog versions, ID ranges and
  the placement map, catalog contents opaque to it; UGS builds on it in stage 6, and stage 8 replicates it.
- What changes: the key encoding, the posting and packed-UID formats (new version bytes), the set operations, the term
  dictionary's allocation (per (type, bucket) counters), the wire protocol's internal term encodings, HNSW's node IDs;
  the `uid-lists` and `key-encoding` specs stop matching Dgraph byte for byte, so their Go golden vectors are retired
  or regenerated; the `catalog`, `schema-storage` and `schema-changes` specs move to the metadata group.

### 11. Write-path throughput: one partition per core (stage 5.5)

A storage node runs one partition per core, each a shared-nothing replica group with its own log, commit step and
store, after ScyllaDB's shard-per-core model and TigerBeetle's one core per replica (decided 2026-11-01). The
measurements, the model and its consequences for the clustering architecture are in
`openspec/notes/UltraGraphDB Shard-per-Core Write Path.md`; its open questions are this item's to answer.

- One commit thread carries about 38,000 relations per second at 10 or more relations per commit, about 16,700
  commits/s at one: 35–60 µs per commit plus 20–25 µs per relation, the cost of about seven memtable skiplist
  operations per relation at 1–3 µs each (VM disk; the VM's tmpfs `/tmp` overstates every figure).
- The zero-run stored key and the dictionary cache, in `ultragraphdb_store_kv` since 2026-11-01, gain 11–21% per core
  and cut the fjall journal by 23% per relation. The key's width costs little; the operations per relation cost.
- Independent pipelines side by side reach 6.4–7.1× at 12 on 12 vCPUs; each loses 41–47% to the machine at every
  commit size.

To specify, in order:

1. The bucket as placement: derived from a hash of the IRI or `@key`, so that creation and the dictionary's uniqueness
   check stay in one partition; buckets per partition; the placement map in the metadata group (item 10, D1, D4).
2. The writes that stay single-partition under bucket placement and those that cross, with reverse entries and other
   derived indexes by change stream and watermark (stage 8).
3. The partition's runtime: one compio runtime per core running its commit step, execution beside it, the apply on
   the same core or pipelined on a second; the cores a node reserves for fjall's workers, the frontend and the network.
4. One LSM instance per partition, and the node's budget for fjall's workers, memtables and caches.
5. `wal_bench` measuring N partitions in one process with a chosen share of cross-partition commits.
6. The bytes per relation: a logical log record (the quad) in place of physical key deltas, and the design's option C,
   fjall without its own journal, once the CPU no longer bounds the path.

### 2. The client endpoint

The binary-protocol endpoint the frontend serves: a connection's requests turned into the operations of the groups they
reach; request terms converted; result batches built; the error table; statement caches per connection.

- Architecture: the frontend holds the binary-protocol endpoint, query planning, write classification and scheduling
  (*Cluster architecture*); *Transport: one protocol, three carriers* (single-container note).
- Source material: `ultragraphdb_server_session`'s rows and *Crates*; *Reads*' result batches; *The error table*;
  tasks 10.1, 10.2 and 10.8; the nine `wire-protocol` scenarios that need a server answering requests. Their
  requirements stay in stage 5's specs with their other scenarios, so the text of "Frames are validated before use and
  never panic", "Frames are at most 1 MiB", "Data requests, prepared statements and describe", "Result batches are
  columnar and dictionary-encoded", "Requests name terms by their text" and "Connections authenticate" is to be checked
  against the endpoint when it is specified.

### 9. The Rust client against the cluster

What the client library offers once it speaks to frontends: session tokens and read-your-writes across groups, the
query consistency levels, and its own flow control.

- Architecture: commits return a session token (group, operation number, epoch); the per-request consistency flag
  (*Snapshot timestamps: sequencer epochs*, *Query consistency levels*).
- Source material: `ultragraphdb_client_core` and its shells as stage 5 built them; `notes/group-9b.md`, 9b.4's open
  questions.
- Open:
  - Do the shells expose `grant`, so a caller controls a read's credit beyond `next_batch`'s automatic grants, and does
    it take `NonZeroU32`, since a Credit of zero frames is refused on the wire (PROTOCOL.md §9.2)? Or does the core's
    `grant` go, if no caller needs manual flow control?
  - What does `close()` answer for a commit in flight: `ConnectionLost` when its frame was never sent on any connection,
    and `InDoubt` whenever it was sent, at any attempt count? Does `RetriesExhausted` fold into `InDoubt`?

### 3. Authentication and principals

The authenticator over the principals' credentials, throttling, the audit events and their log; the first principal of
a cluster and how it is created; where principal entries live and how they are replicated.

- Architecture: *Security by tier*; the metadata group's responsibilities (*Metadata group*).
- Source material: the requirement "The server authenticates its connections"; *Throttling and the audit*; *The data
  directory and the cluster*'s bootstrap administrator; *Creating a principal*; tasks 10.0 (operation 8), 10.2 and
  10.4; *The server*'s open questions on the bootstrap principal and throttling.
- Open: is `Input::CreatePrincipal` through a storage group's commit step still the path, or do principals belong to
  the metadata group?

### 4. Reads

Matching quad patterns in a transaction: access paths, resume keys, an unbound graph, statement IDs, the empty result.

- Architecture: compute executes queries; storage runs narrow kernels near the data (*Cluster architecture*); stage 6's
  query core.
- Source material: the requirement "Reads match quad patterns in their transaction"; *Reads*; tasks 10.3 and 12.3.

### 5. Transactions across replica groups

How stage 5's transactions and commit step, built for one log, become one replica group's state machine, and what a
transaction spanning groups is.

- Architecture: *Consistency and coordination avoidance*, *Write classification*, *Mechanisms*, *Snapshot timestamps:
  sequencer epochs*.
- Source material: stage 5's archived `transactions` and `commit-rounds` specs and the commit step's code; task 12.6,
  read summaries' false-abort rates (D10).
- Open:
  - Do commit versions inside a group keep following the clock (`max(previous + 2, 2 × clock µs)`), with sequencer
    epochs only across groups, or does the group's version become clock-free too?
  - Do interactive transactions, validated optimistically against their read sets, stay within one group, given that
    Calvin-style ordering needs a transaction's read and write sets before it is sequenced?
  - Answered (D3, 2026-11-01): strictly serializable within a group; serializable at a closed epoch across groups,
    with session tokens for read-your-writes and a per-request option that closes the epoch for a strict read. The
    archived `transactions` and `commit-rounds` specs are restated accordingly in stage 5.5's change.
  - What becomes of the goal of read-write transactions on n−1 worker cores under thread-per-core roles?
  - Answered by UGS (2026-11-01): single-group transactions may be interactive; multi-group transactions are
    submitted whole and ordered Calvin-style.
- To specify with the cross-group protocol: the escrow protocol behind `@limit` (shares, renegotiation, a group that
  fails holding a share) and the rank structure behind `@acyclic` with `@colocate(root)` (maintenance under subtree
  moves, the materialized height for `max_depth`).

### 6. Derived indexes

The vector indexer driven in commit order, its reconciliation after a restart, the vector dimension on the wire, a
declaration contradicting its data.

- Architecture: text, vector and statistics indexes are derived data, maintained coordination-free and asynchronously
  (*Write classification*), through ordered change streams (*Mechanisms*).
- Source material: *The vector indexer under the pipeline*; tasks 10.0 (change record 8), 10.4 and 12.5's indexer lag;
  *The server*'s open question on the vector indexer.

### 7. The log of a replica group

Checkpoints driven by the rounds' bytes (the store synced, then the log trimmed); the write-ahead log behind the
ordered-log interface becoming the VSR log a replica group holds.

- Architecture: storage nodes hold the write log and the store (*Cluster architecture*); *Replication and runtime
  model*; the log tier of S3 mode (*Storage modes*).
- Source material: *Checkpoints*; task 10.4; task 12.4, the sync cost and round sizes on drives with and without
  power-loss protection, from which `WalConfig`'s provisional defaults are to be set.

### 8. Verification beyond stage 5

The simulation of a cluster from a seed, the white-box history checker, Elle through the client, kill tests on the
binary (on Linux as well), the formal model of epochs, change streams, watermarks and deletes, and classification
soundness.

- Architecture: *Verification strategy*.
- Mixed-version clusters: how long a cluster may run two binary versions, and the simulator runs that test the "one
  version back" negotiation before a rollout (the distribution architecture's verification strategy).
- Formal models are in Lean (decided 2026-11-01), beside stage 5's round model in `lean/`.
- Source material: the requirements "The node simulator", "History checkers" and "Kill tests on the binary", with
  Elle's scenario; the Elle risk; tasks 11.3, 11.4, 11.5, 11.6 and 11.9's kill tests.
- Deferred from stage 5's close (2026-11-01), to run once the restructure has landed: mutation testing of
  `core_types` and `core_tok` (ported code stage 5 only removed functions from), `store_apply`, `transactions_txn`,
  `transactions_commit`, `client_core`, the wire's bodies, round and write-set record, and the tool crates; the
  fuzzers' audit for the bodies, the round and the write-set record (tasks 11.7 and 11.8 as scoped); and, stopped at
  the stage's close, the confirmation run for `core_codec`'s 22 survivors (tests added, not re-run) and the runs of
  `store_posting`, `store_quads` and `store_hnsw`.

## Source material: requirements

The change's `server-node` spec, whole:

### Requirement: Cores are divided between workers and the commit step
The server SHALL run the commit step on one core and a worker on each of the remaining cores available to the process,
each worker a compio runtime pinned to its core; with one core available, one worker SHALL share it with the commit
step. A connection SHALL be accepted onto the least-loaded worker and stay
there, and an interactive transaction SHALL run on its connection's worker.

#### Scenario: A node with eight cores runs seven workers
- **WHEN** the server starts with eight cores available
- **THEN** it runs seven workers and one commit step

#### Scenario: A node with one core still serves
- **WHEN** the server starts with one core available
- **THEN** it runs one worker and the commit step on that core, and accepts connections

### Requirement: Queues are bounded
Every queue between workers and the commit step SHALL be bounded; a commit arriving at a full queue SHALL be refused as
busy rather than queued.

#### Scenario: Load beyond the queue is refused, not buffered
- **WHEN** commits arrive faster than the commit step decides them and its queue is full
- **THEN** further commits are refused as busy until the queue drains

### Requirement: Configuration
The server binary SHALL read its configuration once at start, from its environment: the listen and health addresses, the
data directory, whether connections authenticate and the bootstrap secret file, the transaction limits, the
power-loss-protection flag, and overrides of the round bounds and the checkpoint interval. The cluster identity SHALL be
drawn at random when the data directory is created and kept in the superblock, not configured. Library crates SHALL take
their settings as values and read no environment.

#### Scenario: An invalid setting stops the start
- **WHEN** the server starts with a read-write limit of zero
- **THEN** it exits with code 2 and an error naming the setting, before opening the store

### Requirement: Start, recovery and shutdown
The server SHALL open the store and the log, run recovery, and only then accept connections; a recovery that fails SHALL
stop the start with its error. On shutdown, at SIGTERM or SIGINT, it SHALL stop accepting, answer requests with Error
17, abort open transactions, decide and answer the commits already queued within its shutdown grace, sync the log and
the store, and exit with code 0. Telemetry SHALL be installed once, through `ultragraphdb_common_telemetry`.

#### Scenario: Connections wait for recovery
- **WHEN** the server starts with rounds to replay
- **THEN** no connection is accepted until every round is replayed

#### Scenario: A shutdown loses no acknowledged commit
- **WHEN** the server is shut down while commits are in flight and then restarted
- **THEN** each commit in flight was either acknowledged and is present, or aborted and is absent

### Requirement: The server authenticates its connections
A server requiring authentication SHALL hold the bootstrap administrator from the secret file its configuration names,
read once when it creates the cluster and kept as its SCRAM-SHA-256 verifier alone, and SHALL create no cluster without
that file. It SHALL refuse a password shorter than 15 characters or on the operator's list of common or breached
passwords. Failed authentications SHALL be throttled per principal and per source address with a growing delay, and
after 100 consecutive failures of a principal its authentications SHALL be refused from every source for the refusal
interval, 15 minutes by default, after which they are accepted again; a success SHALL reset the count. A server
configured without authentication SHALL listen on the loopback interface only, and SHALL refuse to start on any other
address. Each authentication, its outcome and the principal it named SHALL be written to the audit log, and no password,
proof or token SHALL be written to any log (`ultragraphdb_protocol/SECURITY.md`).

#### Scenario: No bootstrap secret, no cluster
- **WHEN** a server requiring authentication starts on an empty data directory without the bootstrap secret file
- **THEN** it exits with an error naming the missing file, and creates no cluster

#### Scenario: A short bootstrap password creates no cluster
- **WHEN** a server requiring authentication starts on an empty data directory with a bootstrap secret file holding a
  password of 14 characters
- **THEN** it exits with an error naming the password rule and not the password, and creates no cluster

#### Scenario: The bootstrap administrator authenticates
- **WHEN** a server requiring authentication creates its cluster from a bootstrap secret file, and a client
  authenticates as the bootstrap administrator with the file's password
- **THEN** the exchange succeeds, and the data directory holds the password's verifier and not the password

#### Scenario: Repeated failures refuse a principal for the interval
- **WHEN** a principal's authentication fails 100 times in a row, and the principal then authenticates with its
  password at once and again after the refusal interval
- **THEN** the first is refused with code 19 and the second succeeds

#### Scenario: A server without authentication listens on loopback only
- **WHEN** a server configured without authentication is given a listen address that is not loopback
- **THEN** it exits with an error naming the setting, before opening the store

#### Scenario: No secret reaches a log
- **WHEN** a client authenticates once with a wrong password and once with the right one
- **THEN** the audit log holds both attempts with their outcomes and the principal named, and no log holds the
  password, a proof or a SCRAM message

### Requirement: The node reports its health
The server SHALL serve HTTP/1.1 on its health address, apart from its protocol address: `GET /livez` SHALL answer 200
while the commit step and every worker run their loops, recovery included, and 503 once one has stalled for 10 seconds;
`GET /readyz` SHALL answer 200 from when the node accepts connections until its shutdown begins or it stops on a
failure, and 503 otherwise. Every other path SHALL answer 404. The endpoint SHALL hold no data and require no
authentication.

#### Scenario: Readiness waits for recovery
- **WHEN** the server starts with rounds to replay and `/readyz` is asked during the replay
- **THEN** it answers 503, `/livez` answers 200, and `/readyz` answers 200 once connections are accepted

#### Scenario: Readiness ends with the shutdown
- **WHEN** the server receives SIGTERM
- **THEN** `/readyz` answers 503 before the listener closes

### Requirement: Connections are bounded
The server SHALL hold at most its configured number of connections; a connection past it SHALL have its Hello answered
with Error 5 (Busy) on stream 0 and be closed.

#### Scenario: A connection past the limit is told why
- **WHEN** a server configured for two connections holds two and a third connects
- **THEN** the third's Hello is answered with Error 5 and the connection closes, and the first two are served

### Requirement: A store or log failure stops the node
A failed sync, a store or log I/O failure, a corruption found, or a violated invariant SHALL stop the node: every
request from then on SHALL be answered with Error 17 (Unavailable), the failure SHALL be logged at error naming it, and
the node SHALL shut down and exit with code 1, so recovery starts from the log at the next start.

#### Scenario: A failed sync stops the node
- **WHEN** the log's sync fails while commits are in flight
- **THEN** none of them is acknowledged, later requests are answered with Error 17, and the process exits with code 1

### Requirement: Reads match quad patterns in their transaction
A Read or an Execute SHALL return every statement of its transaction's snapshot that its pattern matches, in the order
of its access path, with a read-write transaction's own writes and with its reads recorded for validation; an unbound
graph slot SHALL match in every graph of the namespace. A limit SHALL end the read with a resume key that continues it
without repeating or skipping a row.

#### Scenario: A resumed read neither repeats nor skips
- **WHEN** a read of a predicate holding ten statements is made with limit 4, and resumed twice with its resume keys
- **THEN** the three reads return 4, 4 and 2 rows, the ten statements once each, in the order one unlimited read returns

#### Scenario: A pattern over every graph
- **WHEN** a predicate holds a statement in the default graph and one in the graph `<g>`, and a read leaves the graph
  slot unbound
- **THEN** it returns both statements, each with its graph

From the change's `transaction-verification` spec:

### Requirement: The node simulator
A deterministic simulator SHALL run workers, the commit step, the log over simulated storage, the simulated store, a
network that drops, delays, duplicates and splits frames, and a stepped clock, from a seed; a seed SHALL replay its run.
After every step and every recovery it SHALL check that every acknowledged commit is present, except one in a round
the storage tore while reporting success, which recovery must have reported by name; that the store equals the replay
of the log; and the invariants that restate the Lean model's theorems over the run's trace.

#### Scenario: A power loss mid-round loses no acknowledged commit
- **WHEN** a seed injects a power loss while a round is being synced and the node recovers
- **THEN** every commit acknowledged before the power loss is present

### Requirement: History checkers
Elle's list-append and read-write register workloads SHALL run through the Rust client against the server, with
real-time edges, and SHALL find no anomaly. A white-box checker SHALL read every read's and every commit's version from
the log and the simulator's trace and check the serialization graph with real-time edges.

#### Scenario: Elle finds no anomaly
- **WHEN** Elle's list-append workload runs at 8 concurrent clients against the server
- **THEN** Elle reports the history strictly serializable

### Requirement: Kill tests on the binary
The server binary SHALL be killed during rounds, syncs and checkpoints and restarted; after each restart every
acknowledged commit SHALL be present, and every round SHALL be present whole or absent whole. A commit killed before its
reply MAY be present, since its round may have been synced; its client resolves it through its session.

#### Scenario: A kill during a sync
- **WHEN** the server is killed while a round's sync is in progress and restarted
- **THEN** the round's transactions are all present or all absent, none of them was acknowledged, and each client's
  retry through its session learns which

From the change's `wire-protocol` spec, the scenarios that need a server answering requests:

#### From the requirement "Frames are validated before use and never panic" (`wire-protocol`)

#### Scenario: A refused request keeps the connection
- **WHEN** a client executes a stale statement on one stream while another stream reads
- **THEN** the first stream receives an error reply, and the connection and the other stream continue

#### From the requirement "Frames are at most 1 MiB" (`wire-protocol`)

#### Scenario: A large result streams
- **WHEN** a read returns 5 MiB of results
- **THEN** it arrives as several frames on its stream, the last one flagged last

#### From the requirement "Data requests, prepared statements and describe" (`wire-protocol`)

#### Scenario: A statement belongs to its connection
- **WHEN** a statement prepared on one connection is executed on another
- **THEN** the execution is refused with code 10, UnknownStatement

#### From the requirement "Data requests, prepared statements and describe" (`wire-protocol`)

#### Scenario: A statement ID names the statement a read returned
- **WHEN** a read asks for statement IDs, and a Write deletes by the ID of one row it returned
- **THEN** that row's statement is removed, and every other statement remains

#### From the requirement "Result batches are columnar and dictionary-encoded" (`wire-protocol`)

#### Scenario: A blank node is told from an IRI and named again
- **WHEN** a read returns a blank node, and a later request names the term's text as an IRI
- **THEN** the term carries tag `0x02` and its skolem IRI, and the later request reads the same node

#### From the requirement "Result batches are columnar and dictionary-encoded" (`wire-protocol`)

#### Scenario: A read matching nothing answers one empty batch
- **WHEN** a read's pattern matches no statement
- **THEN** one batch arrives, flagged last, with no rows and a column per unbound slot

#### From the requirement "Requests name terms by their text" (`wire-protocol`)

#### Scenario: A relative IRI is refused
- **WHEN** a Write names the IRI `people/alice`
- **THEN** the Write is refused with code 7, `detail` the term's index in its term table

#### From the requirement "Requests name terms by their text" (`wire-protocol`)

#### Scenario: A skolem IRI no blank node holds is refused
- **WHEN** a request names an IRI under the namespace's `.well-known/genid/` base that the dictionary does not hold as a
  blank node's skolem IRI
- **THEN** the request is refused with code 7, `detail` the term's index

#### From the requirement "Connections authenticate" (`wire-protocol`)

#### Scenario: An expired credential closes the connection
- **WHEN** a connection's credential expires before a new exchange completes, with a transaction open
- **THEN** the server answers code 22 on stream 0 and closes the connection, the transaction aborts, and the
  principal's sessions serve its next connection

## Source material: design

From *Crates and layering*:

| Crate | Folder | Depends on | Holds |
|---|---|---|---|
| `ultragraphdb_transactions_sim` | `ultragraphdb_transactions/` | the above, `ultragraphdb_store_sim` | The node simulator, the white-box history checker |
| `ultragraphdb_server_session` | `ultragraphdb_server/` | `_wire`, `_transport`, `_transport_compio`, `ultragraphdb_transactions_*` | Handshake, sessions, dispatch to workers, describe |
| `ultragraphdb` | `ultragraphdb_server/` | `ultragraphdb_server_session`, `ultragraphdb_common_telemetry`, `ultragraphdb_store_kv` | The server: its library and the `ultragraphdb` binary (renamed from `ultragraphdb_server`, 2026-10-30) |

### The server

Drafted 2026-10-30 for review (task 10.0), after the review of group 10 found the server under-specified. Where an
earlier decision settles a point it is cited; every other choice below is a proposal, listed under *Open Questions*
until the user confirms it. The code follows Ultra Style (`openspec/protocols/ULTRA_STYLE.md`), which applies from
this group on.

#### Crates

| Crate | Holds |
|---|---|
| `ultragraphdb_server_session` | The connection's session layer, sans-I/O: the transport's `ServerConnection` per connection, the requests it hands over turned into transaction operations, the conversions of Write, Describe and Operation bodies and request terms (10.2's), result batches built from rows, the authenticator, throttling and the audit events. A step function over a connection's frames, the time and the commit step's replies, so the node simulator (11.3) drives it without a runtime |
| `ultragraphdb` | The node: configuration, the data directory, cluster creation, recovery, the threads and their runtimes, the acceptor, the commit-step thread, the health endpoint, signals and shutdown, telemetry; the `ultragraphdb` binary |
| `ultragraphdb_tests` | The acceptance batteries: the built binary started by `service_utils` and driven through `ultragraphdb_client_tokio`, as scrith's `smdb_tests` drive their services |

The pattern engine (*Reads*) lives in `ultragraphdb_transactions_txn`, beside the transaction whose reads it records.
Each group-10 crate carries the Ultra lint set in its own `[lints]` table: Cargo lets a crate inherit the workspace's
table or state its own, not both.

#### Reads

A Read or an Execute is a quad pattern matched in its transaction: `Transaction::match_into(pattern, resume, limit,
now_us, rows)`, which fills `rows` with each match's terms in slot order (store encodings) and statement IDs and returns
the resume key when `limit` stopped it. A read-write transaction's matches are recorded in its read set (the lists read,
the ranges scanned, the dictionary absences), so a conflicting commit aborts it; it reads its own writes. A read-only
transaction reads its snapshot and records nothing.

Bound terms are resolved through the dictionary first (`QuadStore::uid_of`); a bound IRI or literal the dictionary
does not hold matches nothing, recorded as an absent term. Then one access path per binding of subject (S), predicate
(P) and object (O), in each graph the graph slot names:

| Bound | Path | Rows in the order of |
|---|---|---|
| S P O | the forward list (G, P, S), tested for O | — |
| S P | the forward list (G, P, S) | its postings |
| P O | the reverse list (G, P, O) | its postings |
| P | the forward lists of the range (G, P) | subject, then postings |
| S | the subject's predicates (`0x12`, G, S), then each forward list (G, P, S) | predicate, then postings |
| O | the object's predicates (`0x13`, G, O), then each reverse list (G, P, O) | predicate, then postings |
| S O | as S, each list tested for O | predicate |
| none | the forward keyspace of G | predicate, subject, postings |

A bound graph or the default graph names one graph; an unbound graph slot skip-scans the namespace's graphs, as the
stage-4 drops do (one seek per graph). A vector is never a bound object: the reverse keyspace holds none (stage 4), so
a pattern binding one is refused with code 7.

**Resume keys.** A resume key is the key of the list being read when the limit was reached, followed by the `uid`
field (u64 big-endian) of the last posting returned; the next request starts after it. The server refuses a key outside
the pattern's access path, or one it cannot parse, with Error 1 on the request's stream (`PROTOCOL.md` §11.8, point 47).

**Result batches.** The session layer turns rows into `ResultBatch`es: a UID's text and its skolem flag from
`QuadStore::iri_of` (tag `0x02` for a skolem), a hash's text from `string_of`, a custom datatype's IRI as the
annotation, each looked up once per batch, which holds each distinct term once. A batch closes before a row would take
its frame past the smaller of the client's and the server's `max_frame`, or at 4,096 rows; it streams while the stream
holds credit (`Outbound`). The last batch of a read carries `LAST`, and the resume key when the limit was reached. A
read matching nothing answers one empty batch flagged `LAST` (point 9).

#### Threads and queues

A node runs, inside one `std::thread::scope` so every thread borrows the store, the applier and the configuration
without reference counting:

- **The commit-step thread**, a compio runtime owning the `Pipeline` (its log, its step, its applier's apply).
- **Workers**, one compio runtime per thread, one thread per core but the commit step's (n−1, at least one; on one core
  the worker and the commit step share it, `server-node`), each pinned with `sched_setaffinity` (`rustix`) on Linux and
  unpinned on macOS. A worker owns its connections, their transactions and their statement caches.
- **The acceptor**, a thread with a blocking `std::net` listener: it accepts, sets `TCP_NODELAY` and keepalive (60 s
  idle, 10 s interval, 6 probes), and hands the socket to the worker holding the fewest connections (an `AtomicU32` per
  worker; ties to the lowest index) over that worker's bounded channel.
- **The health endpoint**, a thread of its own (*Health*).

Every compio runtime is built with `single_issuer` and `defer_taskrun` on Linux (decided 2026-10-27); the node checks
at start that the kernel is 6.6 or later and refuses to start below it, naming the kernel. The node's threads are its
own: `std::thread::scope` carries the `#[expect(clippy::disallowed_methods, reason)]` the store's production
implementations carry (`clippy.toml`), and the simulator drives the step functions those threads run, not the threads.

**Worker to commit step.** One bounded channel (`futures-channel`, as decided) carries the pipeline's inputs —
commits, registrations, drops, retries — tagged with a ticket: the worker's index in its top 8 bits, a counter of the
worker's in the rest. A send to a full channel is refused at once with code 5 (Busy). The pipeline's own waiting
queue is bounded by `Pipeline::with_queue`. **Commit step to worker:** one bounded channel per worker carries replies
and the transaction buffers coming back (`Pipeline::buffers_mut`), routed by the ticket's top bits; its capacity is the
most commits a worker may have in flight, so the commit step never waits on it, and a full one is an invariant
violation. A worker maps each ticket to its connection and stream. **Read floor:** each worker publishes the read
version of its oldest open read-write transaction in an `AtomicU64`; the commit step takes their minimum, or the
watermark when none is open, as `Input::OldestRead` once per round.

**Time.** Every thread reads the time through the clock it is given (`clippy.toml`'s `disallowed-methods`). A worker
checks a transaction's limit (`TxnLimits`, the Welcome's `transaction_ms`) at each of its requests and once a second
over all its open transactions, ending an expired one with Error 4 on its stream.

#### Bounds

| Bound | Default | Past it |
|---|---|---|
| Connections per node | 1,024 | the Hello is answered with Error 5 (Busy) on stream 0, and the connection closes |
| Frame buffers per worker | 64 of `max_frame` | a connection's next frame waits for a free buffer |
| Commits in flight per worker | the commit queue divided among the workers | Error 5 |
| Commit queue (channel and pipeline) | 4,096 | Error 5 |
| Rows per result batch | 4,096, within `max_frame` | the next batch |
| Source addresses tracked by throttling | 65,536 | the address least recently seen is forgotten |
| Streams, credit, sessions, statements | `PROTOCOL.md` §11.9 | its codes |

Outgoing frames are built into the worker's pooled buffers, so a worker's steady state allocates nothing (Ultra
rule 3).

#### The error table

| Raised | Answered |
|---|---|
| `TxnError::ReadAboveWatermark` | Error 7 on the stream, `detail` the watermark, the message naming both versions; the worker serves on |
| `TxnError::PastLimit` | Error 4, the transaction ended |
| `TxnError::WriteSetTooLarge`, `ReadSetTooLarge` | Error 6, `detail` the limit |
| A catalog's or a term's refusal | Error 7, `detail` the term's index where one is at fault |
| `CommitError` | its own `code()` |
| A body breaking a rule | Error 1 on the stream; a frame breaking one, Error 1 on stream 0 and the connection closes (§2.6) |
| A store or log I/O failure, a corruption, an `InvariantError` | fail-stop: Error 17 to every request from then on, the event logged at error, the node shuts down and exits 1 |

#### Configuration

The node reads its configuration once, at start, from environment variables, as scrith's `dgraph_migration` does:
every place it runs supplies the environment, a container manifest and the test harness alike. The binary takes no
arguments but `--help` and `--version`. Every value is parsed and range-checked before the data directory is opened;
the first one broken stops the start with exit code 2, naming the variable and its rule.

| Variable | Default | Rule |
|---|---|---|
| `UGDB_DATA_DIR` | required | a directory, created when missing |
| `UGDB_LISTEN` | `127.0.0.1:7400` | an address and port; loopback when `UGDB_AUTH` is `off` |
| `UGDB_HEALTH_LISTEN` | `127.0.0.1:7401` | an address and port, apart from `UGDB_LISTEN`'s |
| `UGDB_AUTH` | `on` | `on` or `off` |
| `UGDB_BOOTSTRAP_SECRET_FILE` | none | required with `UGDB_AUTH=on` on an empty data directory |
| `UGDB_PASSWORD_DENYLIST_FILE` | none | a file of passwords, one per line |
| `UGDB_AUDIT_LOG` | `<UGDB_DATA_DIR>/audit.log` | a file path |
| `UGDB_WORKERS` | cores − 1, at least 1 | 1 to cores |
| `UGDB_CONNECTIONS_MAX` | 1,024 | 1 to 65,535 |
| `UGDB_FRAME_BUFFERS` | 64 | per worker, at least 4 |
| `UGDB_COMMIT_QUEUE` | 4,096 | 1 to 1,048,576 |
| `UGDB_TRANSACTION_MS` | 10,000 | a read-write transaction's life, 1 to 600,000 |
| `UGDB_READ_ONLY_MS` | 16,000 | 1 to 3,600,000 |
| `UGDB_MAX_WRITE_SET` | 1,040,384 | 4,096 to 1,040,384 |
| `UGDB_SESSIONS`, `UGDB_STATEMENTS`, `UGDB_MAX_STREAMS`, `UGDB_INITIAL_CREDIT`, `UGDB_MAX_FRAME` | §11.9's | §11.9's ranges |
| `UGDB_IDLE_LIMIT_S`, `UGDB_AUTH_TIMEOUT_S`, `UGDB_REFUSAL_INTERVAL_S` | 600, 60, 900 | at least 1 |
| `UGDB_POWER_LOSS_PROTECTION` | `false` | `true` or `false` |
| `UGDB_ROUND_BYTES`, `UGDB_ROUND_TRANSACTIONS`, `UGDB_CHECKPOINT_BYTES` | the flag's (`WalConfig`) | within the log's limits |
| `UGDB_SHUTDOWN_GRACE_S` | 30 | 0 to 3,600 |
| `UGDB_LOG_FILTER`, `UGDB_LOG_FORMAT`, `UGDB_OTLP_ENDPOINT` | `info`, `plain`, none | `ultragraphdb_common_telemetry`'s |

#### The data directory and the cluster

`<UGDB_DATA_DIR>/store` holds the store (fjall), `<UGDB_DATA_DIR>/wal` the log, `audit.log` the audit log by default. At
start the node reads the log's superblock (`SuperblockRead`): `Present` gives the cluster; `Empty` on an empty data
directory creates one, the cluster drawn from the operating system's randomness; `Empty` beside a store, or
`Unreadable`, stops the start naming the directory (exit 3).

**The bootstrap administrator.** A cluster created with authentication takes its first principal from the bootstrap
secret file: UTF-8, the password alone, one trailing line feed (or CR LF) dropped. The node refuses a file readable by
group or others on Unix, a password under 15 characters or on the deny list, naming the rule and never the password,
and creates nothing. It derives the SCRAM-SHA-256 verifier at `DEFAULT_ITERATIONS` and proposes the principal to the
commit step as its first input, `Input::CreatePrincipal { name, verifier }`, numbered by its commit version as a session
is: its round writes the principal's entry under `\xffultragraphdb/security/principal/` without versions, the security
catalog's first entry and its format's first version, and its write-set record names it with operation 8, create a
principal (`PROTOCOL.md` §12.2, to be added), so replay restores it. Until stage 7 the node loads every principal entry
at start into the authenticator's `Credentials`; the file is read only when the cluster is created.

#### Start and shutdown

Start: configuration; telemetry, once; the kernel check; the data directory; the log opened and recovered
(`Wal::open`); the store opened (`Durability::Process`: the log is the durable record); `Replay::run`; the pipeline;
the principal entries; the threads; the listener bound; then the health endpoint turns ready and the node logs
`ultragraphdb ready` with both addresses.

Shutdown, on SIGTERM or SIGINT: readiness turns false; the listener closes; requests on open connections are answered
Error 17; open transactions are aborted; the commits already queued are decided, synced and answered for up to
`UGDB_SHUTDOWN_GRACE_S`; the log and the store are synced; the node exits 0. A second signal exits at once, code 1;
recovery covers it. A fail-stop takes the same path with exit code 1.

**Checkpoints.** The commit-step thread checkpoints when the rounds since the last checkpoint pass
`UGDB_CHECKPOINT_BYTES`: it syncs the store (`FjallStore::sync`), then trims the log through the last applied round with
its version (`OrderedLog::trim_through`), between rounds.

| Exit code | Meaning |
|---|---|
| 0 | a clean shutdown |
| 1 | a fail-stop, a failed sync, a second signal |
| 2 | a configuration refused, or a kernel below 6.6 |
| 3 | a data directory or a recovery refused |

#### Health

A thread of its own serves HTTP/1.1 on `UGDB_HEALTH_LISTEN`, after Kubernetes' probe conventions and the paths
`kube-apiserver` serves: `GET /livez` and `GET /readyz`, 200 with `ok` or 503 with the reason as text, every other path
404, every request one response and the connection closed. It reads a request line and headers of at most 1 KiB within
one second, answers one request at a time, and holds no data: it reads the node's state from atomics.

- `/livez` answers 200 while the commit-step thread and every worker have marked their loop within the last 10 seconds,
  through recovery included, so a long recovery is not restarted; a stalled loop answers 503.
- `/readyz` answers 200 from the ready log line until shutdown begins or a fail-stop, 503 before and after: Kubernetes
  sends connections only to a node that accepts them.

Kubernetes probes `/livez` for liveness, `/readyz` for readiness and as the startup probe. The tests wait on `/readyz`.
It is written over `std::net` by hand, a few hundred lines, so the node adds no HTTP or gRPC stack (tonic and hyper
need tokio; the node runs compio).

#### Throttling and the audit

Each failed authentication delays its Error 19 by `min(100 ms × 2^(n−1), 10 s)`, n the consecutive failures of the
principal or of the source address, whichever is more; the connection holds the reply until its deadline, so no worker
waits. After 100 consecutive failures of a principal its authentications are refused from every source for
`UGDB_REFUSAL_INTERVAL_S`; a success resets both counts (`server-node`).

The audit log is JSON lines: one object per authentication with `time` (RFC 3339, UTC, microseconds), `event`,
`outcome`, `name` claimed, `principal` (or null), `mechanism`, `source` and `connection`, never a password, proof or
SCRAM message. Events go to the `ultragraphdb::audit` tracing target, which `ultragraphdb_common_telemetry` writes to
the audit file alone.

#### The vector indexer under the pipeline (group 8's open questions)

- **The pipeline drives the indexer**, in the form builds take: `prepare_index_vectors` making a pass an `Executed`,
  `Prepared::index_vectors`, and `Pipeline::poll_vectors` proposing each predicate with work under
  `Pipeline::SYSTEM_TICKET`, one pass in flight per predicate. A pass reads the predicate's catalog entry and its
  vector-index range, so a drop, a declaration or a build decided meanwhile aborts it; its queue entries are taken when
  it is applied.
- **After a restart** the indexer reconciles each vector predicate at open: its forward lists against its neighbour
  lists, re-queueing the vectors the lists do not hold. It writes nothing per vector; it scans once per restart.
- **The dimension travels on the wire**: change record 8 (declare a predicate) gains a u32 argument, the vector
  dimension, 0 for none, matching the catalog (`PROTOCOL.md` §4.6, to be added before 10.2).
- **A declaration over vectors of another dimension** is refused as `Contradicted`, as every other contradiction a
  predicate's data makes of a declaration is.

#### Tests

- **Unit and BDD**, in each crate: the session layer, the pattern engine, configuration, the start's decisions and the
  health responses as step functions, without a runtime.
- **Acceptance**, `ultragraphdb_tests`: the binary copied into the sandbox (`copy_file`), started by
  `service_utils::ServiceUtil` with its environment (a data directory under `TEST_TMPDIR`, a port per battery),
  `WaitForHttpHealthCheck` on `/readyz`, then a battery of scenarios sequenced in one `#[test]` through
  `ultragraphdb_client_tokio`, as scrith's `smdb_tests` run. Every server scenario and the conformance suite run this
  way (10.5). `service_utils` joins the workspace's dependencies for it, as in scrith.
- **Simulation**: the node simulator (11.3) drives the session layer, the workers' logic and the commit step as step
  functions, the threads replaced by its schedule.

From *Risks / Trade-offs*:

- [Elle runs on the Java virtual machine] → it runs outside Bazel, as the Lean model does, through the Rust client.

From *Open Questions*:

Every decision of the note was taken on 2026-10-27. *The server* (task 10.0, drafted 2026-10-30) proposes the rest of
the server's design; each proposal stands until the user confirms or changes it:

1. Reads: the access paths, the resume key's form, 4,096 rows per batch, and a bound vector object refused.
2. Threads: one `std::thread::scope`, a blocking acceptor thread, pinning through `rustix`, tickets carrying the
   worker's index.
3. The bounds' defaults: 1,024 connections, 64 frame buffers per worker, a commit queue of 4,096.
4. The error table, a read above the watermark answered with code 7, and fail-stop on a store or log failure.
5. Configuration from `UGDB_*` environment variables, the ports 7400 and 7401, and the exit codes.
6. The bootstrap principal through the commit step, `Input::CreatePrincipal` and the log's operation 8.
7. The health endpoint: HTTP/1.1 written over `std::net`, `/livez` and `/readyz`.
8. Throttling's delay, `min(100 ms × 2^(n−1), 10 s)`, and the audit log's JSON lines.
9. The vector indexer: the pipeline's proposals, reconciliation at open, the dimension on the wire, the refused
   contradiction.

## Source material: tasks

Group 10, *The server*:

Written to Ultra Style (`openspec/protocols/ULTRA_STYLE.md`): each crate carries the Ultra lint set in its own `[lints]`
table, and checks its invariants through `ultragraphdb_error_invariant`. The design is `design.md`, *The server*.

- [ ] 10.0 The design: *The server*'s proposals (`design.md`, *Open Questions* 1 to 9) confirmed or changed by the user;
  then `PROTOCOL.md` first, before any code: §4.6, change record 8's vector dimension, and §12.2, the write-set
  record's operation 8, create a principal
- [ ] 10.1 Scenarios: server-node, all requirements, the health endpoint, the connection bound, fail-stop and reads
  among them; wire-protocol ("The Rust client", end to end against the node), transactions ("A read above the
  watermark is refused", at the session); the two scenarios group 2 left to the server, "A refused request keeps the
  connection" and "A large result streams", with the cluster and unknown-command scenarios rebound to answering and
  closing the connection; vector-index, the dimension declared on the wire and the contradiction refused
- [ ] 10.2 Phases 1–3, the session layer (`ultragraphdb_server_session`), sans-I/O: a connection's requests turned into
  transaction operations; Write, Describe and Operation bodies and request terms converted to `Changeset`, declarations
  and oxrdf types, a triple term iteratively or within the wire's nesting bound, never recursing past it (decided
  2026-10-27); result batches from rows, texts and skolem flags looked up once per batch, 4,096 rows or `max_frame`
  per batch; the error table; the authenticator over the principals' `Credentials`, throttling and the audit events
- [ ] 10.3 Phases 1–3, the pattern engine (`ultragraphdb_transactions_txn`): `Transaction::match_into` over the access
  paths of *Reads*, an unbound graph skip-scanning the namespace's graphs, resume keys checked against the path, a
  read-write transaction's matches recorded in its read set and its own writes seen
- [ ] 10.4 Phases 1–3, the commit step's new work (`ultragraphdb_transactions_commit`, `_txn`,
  `ultragraphdb_store_apply`, `ultragraphdb_transactions_log`): `Input::CreatePrincipal` and its replay; the vector
  indexer under the pipeline (`Pipeline::poll_vectors`, its read set, its queue taken at apply, reconciliation at open);
  checkpoints driven by the rounds' bytes (store sync, then `trim_through`)
- [ ] 10.5 Phases 1–3, the node (`ultragraphdb`): configuration from `UGDB_*`, every value checked before the data
  directory opens; the data directory, the superblock's three outcomes and cluster creation with the bootstrap
  administrator (the secret file's rules); the start's order, the shutdown's and the exit codes; the health endpoint's
  responses; the bounds; the ticket routing between workers and the commit step
- [ ] 10.6 Phase 4: 10.2 to 10.5 implemented against their audited suites: the threads in one scope, workers pinned on
  Linux, the commit step on its thread, the acceptor handing sockets to the least-loaded worker, bounded channels
  answering busy, every compio runtime with `single_issuer` and `defer_taskrun` (decided 2026-10-27) and the kernel
  checked for 6.6; the health endpoint over `std::net`; telemetry through `ultragraphdb_common_telemetry`, the audit
  target written to its own file; signals and the shutdown's grace; fail-stop
- [ ] 10.7 Acceptance (`ultragraphdb_tests`): the binary copied into the sandbox and started by `service_utils`, its
  environment set per battery, `WaitForHttpHealthCheck` on `/readyz`, batteries sequenced in one `#[test]` each through
  `ultragraphdb_client_tokio`, as scrith's `smdb_tests` run; every server scenario and the conformance suite;
  `service_utils` pinned in the workspace and listed in `AGENTS.md`
- [ ] 10.8 Phase 5: fuzz targets for the decoders the group adds — resume keys, the configuration's values, the health
  endpoint's request, the bootstrap secret file, the principal entry — built and not run (11.7); mutation testing is
  11.8's, at the stage's close

From groups 11 and 12 (11.9 and 12.5 moved in part: stage 5 keeps the log's compio suite on Linux and the log's write
volume beside fjall's journal):

- [ ] 11.3 The node simulator from a seed: workers, commit step, log over simulated storage, simulated store, network and clock faults; acknowledged commits present and the store equal to the log's replay after every step and recovery; the model's theorems restated as invariants
- [ ] 11.4 The white-box checker over the log and the simulator's trace
- [ ] 11.5 Elle's list-append and read-write register workloads through the client at 8 clients, with real-time edges, outside Bazel; the runs and their results recorded
- [ ] 11.6 Kill tests on the binary during rounds, syncs and checkpoints
- [ ] 11.9 The write-ahead log's compio suite and kill tests on Linux, in an OrbStack container, so the `O_DIRECT` path, which only type-checks on macOS, runs against a real Linux file system
- [ ] 12.2 Throughput at 1, 2, 4, 8 and n−1 workers with abort rates; the commit step's cost per transaction against the execution cost divided by n−1
- [ ] 12.3 Match-then-insert and insert-if-absent over a Zipf-distributed set of entities at 8 writers; D46's batch of 1,000 member moves
- [ ] 12.5 The write volume of the log beside fjall's journal; the vector indexer's lag on a vector-heavy workload
- [ ] 12.7 Commit latency (median and p99) and throughput at n−1 workers with the runtimes' `single_issuer` and `defer_taskrun` on and off, on Linux
- [ ] 12.4 The sync cost and the round sizes on a drive with power-loss protection and on one without; the flag's defaults set from them; the latency of the round that moves into a new segment (a spare's rename and directory sync) against a round within one, and of a fresh segment's first sync after `fallocate`
- [ ] 12.6 Read summaries' false-abort rates replayed offline against the log's write sets, per budget and bits per key
