# Porting Dgraph from Go to Rust: overview

* Date: 2026-10-20
* Source: the Dgraph fork at `9fbf35614`: v25.5.0 (commit `93ff232`, 2026-10-14) plus six commits, Apache-2.0,
  `github.com/dgraph-io/dgraph`
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

A dependency is pure Rust wherever one exists, so cross-compiling to static musl stays a Cargo and Bazel matter with no
C toolchain per target. The documented exception is `ring`, rustls' crypto provider, which bundles C and assembly; its
pure-Rust alternative, `rustls-rustcrypto`, is younger and less audited.

| Go dependency | Rust | Status |
|---|---|---|
| `dgraph-io/badger/v4` (storage) | `fjall` | Decided |
| `dgraph-io/ristretto/v2` (cache) | `moka` | Decided |
| `go.etcd.io/etcd/raft/v3` (consensus) | `raft-rs` (TiKV's port of etcd raft) | Decided |
| `raftwal` (Dgraph's own Raft log) | An append-only log of the port's own, over `compio` (io_uring on Linux) | Decided |
| `google.golang.org/grpc`, `protobuf` | `tonic`, `prost` | Decided |
| `blevesearch/bleve` (tokenizers, stemmers) | A port of bleve's analysis in `ultragraphdb_core_tok`; `rust-stemmers` for the Snowball languages | Decided |
| `golang/geo`, `twpayne/go-geom`, `paulmach/go.geojson` | Ports of golang/geo's S2 and of go-geom in `ultragraphdb_core_types` | Decided |
| `dgryski/go-farm`, `go-groupvarint` | In-house ports: go-farm's `Fingerprint64` as `go_farm` in `ultragraphdb_common_utils` (MIT), group-varint in `ultragraphdb_core_codec` | Decided |
| `golang/snappy`, `klauspost/compress` | `lz4_flex` for backups; `flate2` over `miniz_oxide` for gzip exports | Decided |
| `dgraph-io/simdjson-go`, `encoding/json` | A port of Go's `encoding/json` in `ultragraphdb_common_utils` | Decided |
| `minio/minio-go` (backups) | `object_store` | Proposed |
| `glog`, OpenTelemetry, OpenCensus, Prometheus | `tracing` and `tracing-subscriber` for logs; `opentelemetry` with `tracing-opentelemetry` and OTLP for spans; a Prometheus exporter for metrics (stage 7) | Decided |
| `gqlgen`, `gqlparser`, `graphql-go`, `graphql-transport-ws` | — | Cut with GraphQL |
| `IBM/sarama` (Kafka change data capture), `go-sql-driver/mysql`, `mark3labs/mcp-go` | — | Cut |

**Consensus: raft-rs.** raft-rs is TiKV's port of etcd raft, the library Dgraph uses. Its `RawNode`
and `Ready` loop are etcd raft's, so `worker/draft.go` and `conn/node.go` port loop for loop.

## API and client

ultragraphdb serves Dgraph's gRPC API, `api.proto`, unchanged on the wire, and accepts the subset of it
that §Transactions describes. A request in that subset means the same to Go Dgraph, so the two can be
compared on the same traffic. `ultragraphdb_proto_dgraph` generates the API. The client is
`ultragraphdb_client`, with `UltraGraphClient` and `UltraGraphError`, copied from `dgraph-rs` and
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
and decided and applied in the same step. A transaction spanning groups needs an order across them; stage 10 orders
transactions in epochs and validates them deterministically (`UltraGraphDB Dual Query Engine Plan.md`, "The
distributed design").

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
* `ultragraphdb_client` replaces `Txn<ReadWrite>`'s `mutate` and `commit` with a one-request transaction.
* scrith's manager moves to this model.

**Which form performs best is measured, not assumed.** The benchmark compares the execution models on
scrith's workload of guards, upserts and hot envelope predicates, with key skew from uniform to Zipfian:
Dgraph's write-key check, the read-set check, and serial execution of upsert blocks. It records
throughput, p99, the abort rate, and the head-of-line blocking the longest upsert causes on the serial
thread.

## Threads and the async runtimes

The loop model is hybrid:

* **The network edge is async.** tonic serves the gRPC API and the connections between nodes on a
  tokio runtime, where many idle connections cost little.
* **The long-running loops run on a fixed pool of dedicated threads.** Each thread drives a set of
  raft-rs groups, taking the `Ready` of every group it holds in one batch. Each Raft thread runs one compio
  runtime for the Raft log's file I/O, completion-based on io_uring on Linux; the Ready loop stays a synchronous
  step function and waits on its own appends. A Dgraph Alpha belongs to
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
| Versioned keys, read at a timestamp | The version in the key: the user key with `0x00` escaped as `0x00 0xFF`, the terminator `0x00 0x01`, then the complemented big-endian version, so fjall's byte order is Badger's key-then-newest order and "newest at or below" is one seek |
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
3. **Cache entries keyed by key, each holding the list, the commit timestamp it reflects and the cache's epoch,**
   replaced in the commit's critical section. A reader at `t` uses an entry only when its timestamp is at or below
   `t` and no commit to that key is applied in between; a fill is refused after a newer commit or an epoch change,
   which drops, schema changes and rebuilds make (stage 2's design).

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

**Measured, stage 2 (2026-10-25).** `store_bench` (`ultragraphdb_tool/ultragraphdb_tool_bench`) drives the apply thread of
`ultragraphdb_store_apply`, then an `Applier` over `FjallStore` with `MokaListCache`, at a fixed offered rate, with commits
shaped as the Dgraph runs wrote them; it measures the store alone, with no client, network, Zero or Raft. Environment as in
`dgraph-bench.md`: MacBook Pro (Mac15,8), Apple M3 Max; OrbStack 2.2.5, Linux VM kernel 7.2.12-orbstack, arm64, 12 CPUs,
11.73 GiB; a `debian:bookworm-slim` container limited to 10 GB, the store on a Docker volume; a release build for
`aarch64-unknown-linux-gnu`; durability `process`, rollups on, a 128 MB cache, batches of at most 256 commits. Each row
is the range over three 30 s runs (`store_bench apply --rate <r> --relations <n> --seconds 30`).

| Offered | Relations per commit | Commits/s | Relations/s | Batch apply p50 / p99 | fjall write share | Write stalls | Peak RSS | On disk | Bytes per relation |
|---|---|---|---|---|---|---|---|---|---|
| 14,500 | 1 | 14,498–14,499 | 14,498–14,499 | 0.2–1.0 ms / 7.0–7.3 ms | 13 % | 0 | 321–339 MiB | 362–365 MB | 832–838 |
| 50,000 | 1 | 20,850–22,990 | 20,850–22,990 | 7.2–8.2 ms / 21.8–24.5 ms | 12 % | 0 | 868–880 MiB | 389–417 MB | 604–622 |
| 14,500 | 50 | 446–464 | 22,284–23,220 | 492–507 ms / 833–889 ms | 7 % | 0 | 932–936 MiB | 321–326 MB | 465–470 |
| 50,000 | 50 | 438–469 | 21,882–23,474 | 490–556 ms / 809–857 ms | 7 % | 0 | 935–949 MiB | 315–328 MB | 465–473 |

- The apply thread's CPU is the limit: 82–90 % busy at saturation, about 40 µs of CPU per relation, while fjall's write
  call takes 7–13 % of a batch and never stalls (at most 4 level-0 tables and one sealed memtable). The port sustains
  14.5k commits/s and saturates near 21–23k commits/s or 22–23k relations/s, whatever the relations per commit. One
  Dgraph group reached about 79k relations/s at 50 per commit (`dgraph-bench.md`), and Dgraph applies a group's
  proposals one at a time on one goroutine (`worker/draft.go:863`, `processApplyCh`), splitting a mutation's edges only
  from 512 edges (`draft.go:561`): its apply loop takes at most about 12.7 µs per relation, the port's 40–45 µs, about
  three times as much. The port's cost per relation is its first bottleneck; `perf-optimization.md` holds the options.
- A stack sample of the apply thread at saturation (macOS, 50 relations per commit): allocation 28 % of its time,
  `memcmp` and `memmove` 19 %, lsm-tree's block decoding and seeks 18.5 % and the memtable skiplist 3 %, the port's own
  posting, apply and kv code about 8 %. Every list read is a range seek through fjall's merging iterator
  (`fjall_snapshot_store_snapshot.rs`), which bloom filters do not serve, so a read of a key not yet written seeks every
  table.
- A musl build at 14,500 offered applies 13,096–13,495 commits/s and does not sustain the rate; the glibc build does.
  The store ships against glibc.
- The journal is 60–70 % of the bytes on disk and is not reclaimed by compaction within a run.
- Batch size, 50,000 offered, one relation, 10 s, three runs per cap: before the posting fix, a request copied the
  batch's stacked layers whole, so a batch cost O(n²); after it, requests share one stack.

  | Batch cap | Commits/s before | Commits/s after | Peak RSS after | Batch apply p99 after |
  |---|---|---|---|---|
  | 16 | 22,608–26,045 | 25,988–27,836 | 589–610 MiB | 1.5–1.6 ms |
  | 64 | 13,695–14,845 | 28,831–29,348 | 610–618 MiB | 4.2–5.0 ms |
  | 256 | 5,575–5,746 | 31,513–32,324 | 902–909 MiB | 16.8–18.1 ms |
  | 1,024 | did not finish in 90 s | 27,356–28,307 | 907–927 MiB | 72.8–82.8 ms |
  | 4,096 | did not finish in 90 s | 24,887–26,102 | 912–953 MiB | 255.8–315.7 ms |

- Rollups and the compaction filter, one run (`store_bench rollup --hubs 10000 --versions 100 --relations 50`): 1,000,000
  relations on 10,000 reverse lists, loaded at 585 commits/s; the rollup of all 10,000 lists took 0.52 s (19,096 lists/s,
  1.9M deltas/s); a major compaction took 1.40 s and collected 997,609 of the 1,007,609 reverse versions at or below the
  discard timestamp. Data files went from 140 MB to 48 MB, the journal from 318 MB to 399 MB; peak RSS 1.0 GB.
- A bulk load and a snapshot install are stages 7 and 10; they are measured there.

**Measured, stage 4 (2026-10-26).** The same runs over the quad layout: `store_bench` drives `ChangesetApplier` with the
Dgraph runs' commits as changesets of quads in the default graph, each relation an edge to a hub and a string, the hubs
named by an `exact`-indexed string; every IRI goes through the term dictionary, every quad writes its forward, reverse
and predicate lists, a statement record and statistics. Environment, build and settings as for stage 2; one 30 s run
per row.

| Offered | Relations per commit | Commits/s | Relations/s | Batch apply p50 / p99 | fjall write share | Write stalls | Peak RSS | On disk | Bytes per relation |
|---|---|---|---|---|---|---|---|---|---|
| 14,500 | 1 | 12,215 | 12,215 | 1.7 ms / 28.2 ms | 24 % | 0 | 466 MiB | 829 MB | 2,263 |
| 50,000 | 1 | 12,925 | 12,925 | 15.1 ms / 33.7 ms | 23 % | 0 | 470 MiB | 885 MB | 2,267 |
| 14,500 | 50 | 635 | 31,728 | 382 ms / 521 ms | 19 % | 0 | 587 MiB | 423 MB | 441 |
| 50,000 | 50 | 620 | 30,978 | 391 ms / 559 ms | 19 % | 0 | 601 MiB | 400 MB | 427 |

- The commits read no list: each relation's subject is a new IRI, whose lists are known empty, and the reverse list, the
  value index and the predicate lists are written blind, so the lists read per relation are 0 and no read fills the
  cache. The cache's lookups, 2.6–4.8 million per run, are the commits' updates of the entries they wrote, and none
  found one; the store's reads of a list, 41,000–529,000, are the rollups', and none found nothing.
- At 50 relations per commit the apply thread saturates at 31–32k relations/s, 97 % busy, about 31 µs per relation
  against stage 2's 40–45 µs. At one relation per commit it no longer sustains 14,500 commits/s and saturates near
  12–13k: a commit's fixed cost, its dictionary lookups, statement records and statistics, falls on one relation.
- The journal is 66 % of the bytes on disk at one relation per commit, 545–570 MB of 829–885 MB, and is not reclaimed
  within a run; data files are 257–287 MB.
- Rollups and the compaction filter, one run (`store_bench rollup --hubs 10000 --versions 100 --relations 50`): 1,000,000
  relations loaded at 479 commits/s; the rollup of the 10,000 hubs' reverse lists took 0.68 s (14,729 lists/s, 1.5M
  deltas/s); a major compaction took 1.88 s and collected 997,609 of the 1,007,609 reverse versions at or below the
  discard timestamp. Data files went from 236 MB to 85 MB, the journal from 176 MB to 273 MB; peak RSS 468 MiB.

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

Stages 1 and 2 ported Dgraph's foundations and storage. From stage 4, SPARQL and GQL replace DQL, and the stages follow
`UltraGraphDB Dual Query Engine Plan.md` ("Stages") rather than Dgraph's package graph. The roadmap has ten stages
(`ROADMAP.md`, with 1.5 and 5.5 between them; stage 3 is the website and public repository), each engineering stage an
OpenSpec change under `openspec/changes/`:

| Stage | Change | Contents | Checked by |
|---|---|---|---|
| 1. Foundations | `port-stage-1-foundations`, archived 2026-10-23; its specs are in `openspec/specs/` | `x` (keys), `protos/pb`, `codec`, `algo`, `types`, `lex`, `dql`, `tok`, `schema` (parsing), `chunker` (parsers); the BDD and TDD harness | Dgraph's unit tests; golden vectors from the Go source |
| 2. Storage | `port-stage-2-storage`, archived 2026-10-25; its specs are in `openspec/specs/` | fjall and moka under `posting`; rollups, splits, the single-batch commit, index maintenance, rebuilds, drops; `tok/hnsw` | Dgraph's posting tests; crash and concurrency tests |
| 4. Data model and quad storage | `stage-4-data-model-and-quad-storage`, archived 2026-10-26; its specs are in `openspec/specs/` | Mapping contract and catalog; the term dictionary; the quad keyspaces; statement IDs; type bits in UIDs; changesets; DQL and Dgraph's gRPC API removed | Quads round-tripped under the simulator's faults; reads per relation in `store_bench` |
| 5. Transactions and the binary protocol | Not yet written | The commit step on n−1 cores in its distributed shape (order, deterministic validation, apply; read summaries; epochs); the binary protocol crate, its conformance suite and client; the single-node server | Elle (strict serializability); scaling at 1 to n−1 workers; read summaries' false-abort rates; the protocol conformance suite |
| 5.5. Hardening transactions with chaos testing | Not yet written | Stage 5's commit step and server under the simulator's faults and process kills; scrith's Dgraph probes Q1 and A8 as tests | Elle under faults; no acknowledged commit lost across a kill |
| 6. Query core | Not yet written | What SPARQL and GQL share: the kernel, both parsers, CRUD, pattern matching, filters, ordering, aggregates, bounded walks, vector and text search; spareval as the SPARQL oracle | Selected W3C tests and MGQL's golden results inside the core; SPARQL against GQL on the same questions; the DeepBrain MVP's queries |
| 7. Single-node product | Not yet written | Access control and namespaces; backup, restore, export; the loaders; administration; client libraries | The DeepBrain MVP against a single-node server; backup and restore under faults |
| 8. Full SPARQL and GQL conformance | Not yet written | Everything outside the core; the Flagger and feature register; the full MGQL use; worst-case-optimal joins and morsels; SHACL and GQL graph types | The roadmap's phase-1 exit gate: the W3C suites, the openCypher TCK and GQL scenarios, published conformance numbers and baselines |
| 9. Bolt | Not yet written | The Bolt adapter and Neo4j's schema procedures | Neo4j drivers through Bolt; LangChain and LlamaIndex initialise |
| 10. Distributed tiles | Not yet written | Tiles, first on one node; the ordering service over a VSR-replicated log; compute and storage nodes; placement and rebalancing | Storage-differential testing; Elle and Jepsen under faults; a graph larger than one node's memory |

The single-node, replication and sharding port changes were closed unapplied on 2026-10-25 and are archived as
`2026-10-25-port-stage-5-single-node`, `2026-10-25-port-stage-6-replication` and
`2026-10-25-port-stage-7-sharding-and-operations`.

Known bugs (step 3 of the plan) and re-architecture (step 5) follow the port.

**Scope.** Beyond GraphQL, the port cuts Dgraph's change data capture and its Kafka sink, MCP, audit logging and
encryption at rest; the single node offers a commit log of records that transactions attach instead (stage 7); it keeps
access control and namespaces, backup, restore and export, the bulk and live loaders, and the HNSW vector index.
DQL is replaced by SPARQL and GQL from stage 4. Dgraph's Zero and Alpha are not built as separate processes;
storage and compute separate instead, into compute nodes, an ordering service and storage nodes (stage 10).

**Data from Go Dgraph and backups.** Data enters from Go Dgraph through its RDF or JSON export and the live or bulk
loader, which build every index from values with the port's tokens. Backups are the port's own, in its own format (stage 7):
per-group parts in key order with checksummed frames, one immutable manifest per backup written last, every string value
checked for valid UTF-8 before writing; restore loads only such backups, into staged stores it switches to on success. Go Dgraph's backups, stores and WALs are not read.

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

1. Is the admin interface for export, backup and restore a set of HTTP endpoints, or requests on the binary
   protocol? (Stage 7.)

How a read-write transaction's reads are carried and bounded, and how transactions spanning groups and upsert blocks
are ordered, are decided in `UltraGraphDB Dual Query Engine Plan.md` ("Decisions", "The distributed design").
