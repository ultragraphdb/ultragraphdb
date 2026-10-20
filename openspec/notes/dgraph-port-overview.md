# Porting Dgraph from Go to Rust: overview

* Date: 2026-10-20
* Source: Dgraph `v25.5.0` (commit `93ff232`, 2026-10-14), Apache-2.0, `github.com/dgraph-io/dgraph`
* Prior work: the investigation of Dgraph v25.5.0 in the scrith repository,
  `scrith/openspec/notes/schema/bench/dgraph-investigation.md` and `dgraph-bench.md` §5–§6 there.

## The plan

ultragraphdb ports Dgraph to Rust in five steps:

1. **Port.** Translate Dgraph's Go code to Rust, keeping its architecture and its DQL query language.
2. **Consolidate and clean up** the ported code.
3. **Fix the known bugs** and add the missing tests.
4. **Run it in production.**
5. **Re-architect** the handful of decisions that limit Dgraph, on the tested Rust codebase.

Four things change during the port itself: Badger becomes fjall, Ristretto becomes moka, the GraphQL
layer is cut, and a transaction is one request (§Transactions). Everything else keeps Dgraph's design
until step 5.

**Why this order.** Dgraph's mechanisms are sound; the codebase is under-tested and was left unmaintained
for years. Re-architecting it in Go means changing code that cannot be verified. A consolidated, tested
Rust port is a base on which each re-architecture step can be checked.

## Scope

Dgraph v25.5.0 holds about 254k lines of code; about 104k of them are hand-written production Go,
excluding test harnesses (`systest`, `dgraphtest`, `testutil` and others, about 20.6k). Line counts
below are `tokei` code lines in production files.

**Ported:**

| Area | Packages | Lines |
|---|---|---|
| Query language and execution | `query`, `dql`, `lex`, `algo` | ~12.9k |
| Index tokenizers, types, schema | `tok`, `types`, `schema` | ~6.7k |
| Posting lists and their encoding | `posting`, `codec`, `chunker` | ~7.1k |
| Server API and transaction entry | `edgraph` | ~4.2k |
| Group work: proposals, apply, snapshots, predicate moves | `worker` | ~13.5k |
| Coordination: timestamps, commits, membership, tablets | `dgraph/cmd/zero` | ~3.0k |
| Raft wiring and transport | `conn`, `raftwal` | ~2.1k |
| Shared utilities | `x` | ~5.5k |
| Server binaries and tools | `dgraph/cmd/alpha`, `bulk`, `live`, `debug` and others | to be scoped |

**Cut:** the GraphQL layer (`graphql`, 34.3k lines) and everything that exists only for it.

**Replaced:** Badger (15.1k lines), Ristretto (4.4k lines), and the Go libraries in §Dependencies.

What remains to port is about 70k lines of production Go, plus Dgraph's Go tests where they test
ported code (about 90.6k test lines across the repository).

## Dependencies

| Go dependency | Rust | Status |
|---|---|---|
| `dgraph-io/badger/v4` (storage) | `fjall` | Decided |
| `dgraph-io/ristretto/v2` (cache) | `moka` | Decided |
| `go.etcd.io/etcd/raft/v3` (consensus) | `raft-rs` (TiKV's port of etcd raft) | Decided |
| `google.golang.org/grpc`, `protobuf` | `tonic`, `prost` | Decided |
| `blevesearch/bleve` (tokenizers, stemmers) | tantivy's tokenizers or `rust-stemmers` | Proposed |
| `golang/geo`, `twpayne/go-geom`, `paulmach/go.geojson` | `geo` and an S2 implementation | Proposed |
| `dgryski/go-farm`, `go-groupvarint` | `farmhash`, an in-house group-varint | Proposed |
| `golang/snappy`, `klauspost/compress` | `snap`, `zstd` | Proposed |
| `dgraph-io/simdjson-go` | `simd-json` or `serde_json` | Proposed |
| `minio/minio-go` (backups) | `object_store` | Proposed |
| OpenTelemetry, OpenCensus, Prometheus | `tracing`, `opentelemetry`, a Prometheus exporter | Proposed |
| `gqlgen`, `gqlparser`, `graphql-go`, `graphql-transport-ws` | — | Cut with GraphQL |
| `IBM/sarama` (Kafka change data capture), `go-sql-driver/mysql`, `mark3labs/mcp-go` | — | Cut |

**Consensus: raft-rs.** raft-rs is TiKV's port of etcd raft, the library Dgraph uses. Its `RawNode`
and `Ready` loop are etcd raft's, so `worker/draft.go` and `conn/node.go` port loop for loop.

## API and client

ultragraphdb serves Dgraph's gRPC API, `api.proto`, unchanged on the wire, and accepts the subset of it
that §Transactions describes. A request in that subset means the same to Go Dgraph, so the two can be
compared on the same traffic. `ugdb_proto/proto_dgraph` generates the API. The client is
`ultragraph_client`, with `UltraGraphClient` and `UltraGraphError`, copied from `dgraph-rs` and
renamed; it speaks to both servers. Its `test-utils` fixture starts a Go Dgraph container, which serves
the differential runs as well as the client's acceptance test.

## Transactions

**The port holds the transaction; the store applies what the port decided.** Two properties of a
replicated, sharded database place it there:

* **A transaction spans nodes.** Predicates are sharded across groups, so one mutation writes keys on
  several machines, and its conflict check is global. Zero's oracle hands out the start timestamp, checks
  the transaction against the commits since then, and assigns the commit timestamp.
* **Replicas apply deterministically.** Each group is a Raft state machine, and every replica applies the
  same entries to the same state. A store able to abort a write could abort on one replica what another
  committed. A write reaches the store with its outcome decided, and the store applies it.

So the store runs without transactions of its own, as Badger runs in managed mode in Dgraph.

**A transaction is one request.** It carries a read timestamp, the preconditions the client relied on,
and the mutations; the server keeps no state between requests. In `api.proto` terms it is a `Request`
with `commit_now = true`. A read-only query is a request too, at its own timestamp. Mutations without
`commit_now` and `CommitOrAbort` are refused, because they leave a transaction open across requests.

That removes Dgraph's uncommitted layer. Dgraph proposes a mutation through Raft when the client sends
it, holds it in the Alpha's memory under its start timestamp, and decides it in a second round at commit:
two Raft rounds per transaction, state for clients that walk away, and the gap between Zero's decision
and the group's apply where the stale reads arise. A one-request transaction within one group is proposed once
and decided and applied in the same step. A transaction spanning groups needs an order across them; how it gets
one is open in stage 5.

The same reasoning stands behind H-Store, VoltDB, Calvin and TigerBeetle (Stonebraker et al., "The End
of an Architectural Era", VLDB 2007), and FoundationDB's servers hold no transaction state either: its
client buffers writes and sends its read ranges and writes in one commit.

**Two forms of precondition, both supported.** Which one an application prefers is not decidable at this
stage, and one application may need both, so the port carries both:

| Form | The request carries | Outcome | Serializable by |
|---|---|---|---|
| **Read-set check** | The read timestamp, and the keys and index ranges the client's queries read at it | Aborts when any of them was committed after the read timestamp; the client retries | Validation at commit, FoundationDB's model |
| **Server-side conditions** | A DQL upsert block: the query and its `@if` mutations | Runs inside the group's apply thread, in Raft order and in batches; the conditions decide, and it never aborts | Serial execution, TigerBeetle's model |

The read-set check serves arbitrary client logic: read, compute anything, submit. Server-side conditions
serve guards and invariants on hot keys, where retries would pile up. Read-only queries run in parallel
on store snapshots and take part in neither.

Both forms give serializability, and strict serializability, because Zero is one timestamp oracle and
commit order follows real time. Dgraph today checks only the keys a transaction writes
(`zero/oracle.go`, `hasConflict`), and an index key conflicts across different uids only under `@upsert`
(`posting/list.go`, `GetConflictKey`): snapshot isolation that admits write skew, and, without `@upsert`, two nodes
taking one indexed value.

**What it costs.**

* A transaction cannot read its own writes; the client computes dependent writes locally.
* Under the read-set check, retries are the client's, and on hot keys they repeat.
* A query in a read-set transaction returns what it read. `Response.txn.keys` can carry it back; large
  traversals coarsen to whole-predicate ranges.
* `ultragraph_client` replaces `Txn<ReadWrite>`'s `mutate` and `commit` with a one-request transaction.
* scrith's manager moves to this model.

**Which form performs best is measured, not assumed.** The benchmark compares the execution models on
scrith's workload of guards, upserts and hot envelope predicates, with key skew from uniform to Zipfian:
Dgraph's write-key check, the read-set check, and serial execution of upsert blocks. It records
throughput, p99, the abort rate, and the head-of-line blocking the longest upsert causes on the serial
thread.

## Threads and the async runtime

The loop model is hybrid:

* **The network edge is async.** tonic serves the gRPC API and the connections between nodes on a
  tokio runtime, where many idle connections cost little.
* **The long-running loops run on a fixed pool of dedicated threads.** Each thread drives a set of
  raft-rs groups, taking the `Ready` of every group it holds in one batch. A Dgraph Alpha belongs to
  one group and runs each loop as its own goroutine; a fixed pool keeps the thread count bounded when
  a node holds several groups.
* **One apply thread per group writes to the store.** It applies the committed entries of one `Ready`,
  upsert blocks included, and writes them as one fjall write batch.
* **Batched channels between them.** A loop takes everything waiting in one receive, so a busy
  system pays one wake-up per batch rather than one per message.
* **Each loop is a step function** over its inputs, so a simulation can drive the loops
  deterministically, with no runtime and no clock of its own.

## Storage: Badger to fjall

Dgraph runs Badger in managed mode: every key carries its commit timestamp, and a read takes the newest
version at or below its read timestamp. fjall is an LSM-tree store, as Badger is, with atomic write
batches across keyspaces, snapshots, range and prefix scans, and compaction filters. Its sequence numbers
are its own, so the port keeps Dgraph's commit timestamp in the key and uses fjall's plain database, the
one without transactions:

| Badger use in Dgraph | fjall |
|---|---|
| Versioned keys, read at a timestamp | The version in the key, `(key, ts)` with `ts` descending, so "newest at or below" is one seek |
| Rollups writing a complete posting list and discarding older versions | A write of the complete list; a compaction filter drops the versions below the discard timestamp, as Badger's compaction does |
| Writes from the apply loop and from background rollups | Atomic write batches from the group's apply thread |
| The Stream framework: snapshots to followers, backups, export, predicate moves | Streamed range scans over a snapshot |
| No sync on commit by default, `--survive filesystem` for sync | Writes reach OS buffers; the applied Raft index is written in the same batch, and the Raft log carries durability |

**Visibility.** Dgraph's stale reads come from its visibility layer above Badger: writes held in memory
until Zero decides, commits written by the apply loop afterwards, the `MaxAssigned` and `WaitForTs` gate,
and the posting-list cache updated after the write. One-request transactions remove the first of these,
and three rules replace the rest:

1. **A commit never spans write batches.** A batch holds every commit of one `Ready`, each with its data,
   its index entries and its reverse edges, and records the applied Raft index.
2. **A read at timestamp `t` waits until every commit at or below `t` is applied, then reads one store
   snapshot.** The wait makes the read complete; the snapshot keeps it whole while later batches and
   compactions run.
3. **Cache entries keyed by version**, `(key, version)`, so the cache holds only what that version's
   snapshot holds.

A reader then sees a commit whole or not at all, and an index read and a scan of the same snapshot agree.

**The commit path is not storage-bound in Dgraph.** Moving `/dgraph` onto tmpfs raised Dgraph's commit
rate by 1–3 % (`dgraph-bench.md` §6 in scrith), and the two patches in `dgraph-investigation.md` lifted it
23 % without touching storage.

**What to measure early.** Pathxnfs that run storage at full speed in Dgraph too, measured in the Linux VM
where the Dgraph benchmarks ran:

* the apply thread's batches of Dgraph-shaped commits at 14.5k and 50k commits/s;
* rollups and the compaction filter over a large predicate;
* a bulk load and a snapshot install;
* the size of the files after each.

## Correctness oracles

Dgraph has no conformance suite. The port checks itself against five sources:

1. **Go Dgraph v25.5.0 beside the port.** The same data and the same DQL, answers compared, for every
   request in the subset ultragraphdb accepts.
2. **Dgraph's Go tests,** ported with the code they test.
3. **Dgraph's issue tracker:** 2,594 closed issues and 51 open on 2026-10-20, 21 of them labelled `bug`.
   A closed bug with a reproduction is a regression test. Closed by area: query language 103, crash 35,
   indexes 16, mutations 14, data loss 8.
4. **The scrith probes.** Index-rooted reads missing commits below the read point, and reverse edges
   doing the same, measured on v25.4.0 and v25.5.0 and matching no upstream issue (`dgraph-bench.md` §3 in
   scrith).
5. **Elle,** Jepsen's isolation checker, over append and register histories. Go Dgraph admits write skew,
   so it is no oracle for isolation; Elle checks the serializability both precondition forms claim.

## Stages

Step 1 of the plan, the port, runs in five stages ordered by Dgraph's package import graph, each an OpenSpec change
under `openspec/changes/`:

| Stage | Change | Contents | Checked by |
|---|---|---|---|
| 1. Foundations | `port-stage-1-foundations` | `x` (keys), `protos/pb`, `codec`, `algo`, `types`, `lex`, `dql`, `tok`, `schema` (parsing), `chunker` (parsers); the BDD and TDD harness | Dgraph's unit tests; golden vectors from the Go source |
| 2. Storage | `port-stage-2-storage` | fjall and moka under `posting`; rollups, splits, the single-batch commit, index maintenance, rebuilds, drops; `tok/hnsw` | Dgraph's posting tests; crash and concurrency tests |
| 3. Single node | `port-stage-3-single-node` | One Alpha and one Zero process: `worker/task.go`, `query`, `edgraph`, `acl`, Zero's leases; one-request transactions in both forms | Differential DQL against Go Dgraph; the tracker's query, index and mutation reproductions; the visibility probes; Elle |
| 4. Replication | `port-stage-4-replication` | Raft groups of three for the Alpha group and Zero: `raftwal`, `conn`, `draft.go`, snapshots, membership | The crash and data-loss reproductions; fault injection; Elle under faults |
| 5. Sharding and operations | `port-stage-5-sharding-and-operations` | Tablets, fan-out, cross-group transactions, predicate moves, rebalancing; export, backup, restore, the live and bulk loaders | Differential DQL across group layouts; Elle across groups; round trips from Go Dgraph's exports |

Known bugs (step 3 of the plan) and re-architecture (step 5) follow the port.

**Scope.** Beyond GraphQL, the port cuts change data capture, MCP, audit logging and encryption at rest; it keeps
access control and namespaces, backup, restore and export, the bulk and live loaders, and the HNSW vector index.
Zero stays a separate process.

**Testing.** Every spec scenario is a Gherkin scenario run by `cucumber`, written and seen failing before its code;
every ported Go file's tests are ported first; byte-exact formats are checked against golden vectors generated
from the Go source (`AGENTS.md`, §TDD and BDD).

## Re-architecture candidates

Measured on Dgraph v25.5.0 in scrith (`dgraph-investigation.md`), single-relation commits at 256 tasks:

| Decision | What it costs today | Measured |
|---|---|---|
| Zero's leader check: two synchronous `Status()` round trips through etcd raft's node goroutine per `AmLeader` call, about ten per commit | Commits queue on the raft goroutine | Caching leadership raised commits 12 % |
| One UID request to Zero per mutation, unbatched | About 9 ms of a 19 ms median transaction | Block-leased UIDs raised commits 14 %; both changes together 23 %, 11.8k to 14.5k commits/s |
| The Alpha's serial path: one apply goroutine per group, a stop-and-wait commit-delta proposer, `WaitForTs` | Sets the ceiling once Zero is relieved | Not yet isolated |
| Every group carries every commit; Zero decides every commit | Groups add data volume, not commits | Three groups added 8–10 % |
| Predicate sharding: the hottest predicate bounds a namespace's write rate | One group's rate for an envelope predicate | Documented in Gen4 (scrith) |

## Licensing and attribution

Dgraph is licensed under Apache-2.0 and carries no NOTICE file. A ported file keeps the copyright line of
its Dgraph source (`SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.`), names its Dgraph source
path, and states that it was changed (Apache-2.0 §4(b), §4(c)). `AGENTS.md` holds the rule.

## Open questions

Each stage's `design.md` holds the questions it must settle. The ones that shape the whole port:

1. How does a read-set transaction carry its read set to the server? `Request` has a `start_ts` for the
   read timestamp and no field for keys or ranges. (Stage 3.)
2. How is a read set bounded: past what size does a query's read set coarsen to whole-predicate ranges?
   (Stage 3.)
3. Which mechanism commits a transaction spanning groups: Zero ordering the whole request, or Dgraph's prepare
   and decide? (Stage 5.)
4. How is an upsert block spanning several groups ordered: by a Calvin-style sequence that Zero assigns
   across groups, or is a serially executed upsert confined to one group? (Stage 5.)
5. Is the admin interface for export, backup, restore and moves a set of HTTP endpoints, or a gRPC admin
   service? (Stage 5.)
