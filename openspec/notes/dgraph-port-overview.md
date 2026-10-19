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

Three components change during the port itself: Badger becomes redb, Ristretto becomes moka, and the
GraphQL layer is cut. Everything else keeps Dgraph's design until step 5.

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
| `dgraph-io/badger/v4` (storage) | `redb` | Decided |
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

ultragraphdb serves Dgraph's gRPC API, `api.proto`, unchanged, so a request Go Dgraph accepts reaches
ultragraphdb in the same bytes and the two can be compared on the same traffic. `ugdb_proto/proto_dgraph`
generates it. The client is `ultragraph_client`, with `UltraGraphClient` and `UltraGraphError`, copied
from `dgraph-rs` and renamed; it speaks to both servers. Its `test-utils` fixture starts a Go Dgraph
container, which serves the differential runs as well as the client's acceptance test.

## Threads and the async runtime

The loop model is hybrid:

* **The network edge is async.** tonic serves the gRPC API and the connections between nodes on a
  tokio runtime, where many idle connections cost little.
* **The long-running loops run on a fixed pool of dedicated threads.** Each thread drives a set of
  raft-rs groups, taking the `Ready` of every group it holds in one batch. A Dgraph Alpha belongs to
  one group and runs each loop as its own goroutine; a fixed pool keeps the thread count bounded when
  a node holds several groups.
* **One writer thread per redb file.** redb admits one write transaction at a time, so commits and
  rollups queue to that thread and it writes them in batches.
* **Batched channels between them.** A loop takes everything waiting in one receive, so a busy
  system pays one wake-up per batch rather than one per message.
* **Each loop is a step function** over its inputs, so a simulation can drive the loops
  deterministically, with no runtime and no clock of its own.

## Storage: Badger to redb

Dgraph runs Badger in managed mode: every key carries its commit timestamp, and a read takes the newest
version at or below its read timestamp. redb is a single-writer store of copy-on-write B+trees with
multi-version reads and no versions of its own, so the port maps each Badger use explicitly:

| Badger use in Dgraph | redb |
|---|---|
| Versioned keys, read at a timestamp | The version in the key, `(key, ts)` with `ts` descending, so "newest at or below" is one seek |
| Rollups writing a complete posting list and discarding older versions | A write of the complete list and range deletes of the versions it replaces |
| Concurrent writers: the apply loop's commits, background rollups | One writer per file: commits and rollups go through a single writer thread in batches |
| The Stream framework: snapshots to followers, backups, export, predicate moves | Streamed range scans |
| No sync on commit by default, `--survive filesystem` for sync | `Durability::None` with a periodic `Durability::Immediate` commit recording the applied Raft index; the Raft log carries durability |

**Visibility.** Dgraph's stale reads come from its visibility layer above Badger, which is itself
multi-version: writes held in memory until Zero decides, commits written by the apply loop afterwards,
the `MaxAssigned` and `WaitForTs` gate, and the posting-list cache updated after the write. The port
replaces that layer with three rules:

1. **One commit, one redb write transaction**, holding the data, its index entries and its reverse edges.
2. **One read, one redb read transaction**, opened at an applied Raft index, with no second visibility
   mechanism beside it.
3. **Cache entries keyed by version**, `(key, version)`, so the cache holds only what that version's
   snapshot holds.

A reader then sees a commit whole or not at all, and an index read and a scan of the same snapshot agree.

**What to measure early.** redb's own benchmark (Ryzen 9950X3D, NVMe) places it 3.5–4.5× behind RocksDB
and fjall on batch writes and 2–4× behind on removals, with files 2–4.5× larger than RocksDB's. Rollups
and version garbage collection are removals, so the port measures them on Dgraph's own workloads.

## Correctness oracles

Dgraph has no conformance suite. The port checks itself against four sources:

1. **Go Dgraph v25.5.0 beside the port.** The same data and the same DQL, answers compared.
2. **Dgraph's Go tests,** ported with the code they test.
3. **Dgraph's issue tracker:** 2,594 closed issues and 51 open on 2026-10-20, 21 of them labelled `bug`.
   A closed bug with a reproduction is a regression test. Closed by area: query language 103, crash 35,
   indexes 16, mutations 14, data loss 8.
4. **The scrith probes.** Index-rooted reads missing commits below the read point, and reverse edges
   doing the same, measured on v25.4.0 and v25.5.0 and matching no upstream issue (`dgraph-bench.md` §3 in
   scrith).

## Stages

| Stage | Contents | Checked by |
|---|---|---|
| 1. Single node | One Alpha and one Zero on redb and moka: `posting`, `query`, `dql`, `tok`, `types`, `schema`, `edgraph`, and `worker` for one group | Differential DQL; the tracker's query, index and mutation reproductions; the visibility probes |
| 2. Replication | Raft groups of three replicas: `worker`'s proposal and apply loop, `conn`, `raftwal`, Zero | The crash and data-loss reproductions; fault injection |
| 3. Several groups | Tablets across groups, predicate moves | Differential DQL across group layouts |
| 4. Known bugs | The open issues, triaged | Their reproductions |
| 5. Re-architecture | The decisions below | The benchmarks that located them |

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

1. Which Dgraph features beyond GraphQL are in scope for stage 1: ACL, multi-tenancy namespaces, backups,
   the bulk and live loaders, vector indexes, change data capture?
2. Does Zero stay a separate process, as in Dgraph?
