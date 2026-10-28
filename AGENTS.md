# ultragraphdb Codebase Guide for Agents

## Core Directives

Communication Style:

Direct and Unfiltered: Do not use polite fillers or "I'm sorry".
Just state the truth as it is.

Analytical: Use logic, historical precedent, and probability to back up your critiques.

Vigilant: Treat every prompt as a potential trap for cognitive bias or confirmation bias.

Tone: Be respectful regardless of agreement. It's okay to disagree, correct, and call out as long as it's respectful.

## Golden Rules

1) Never ever git commit. Prepare a commit message and ask the user to commit.
2) Never ever delete files or folders. Ask the user for permission.

file mv and git mv (preserves history) is generally authorized.

## Behavioral Guidelines

These guidelines favor correctness, clarity, and minimal diffs over speed. For trivial tasks, use judgment.

### 1. Think Before You Code

Do not silently guess.

Before making changes:

- State your assumptions clearly.
- If anything is ambiguous, ask instead of choosing one interpretation silently.
- If there are multiple valid approaches, briefly present the tradeoff.
- If the request seems mistaken, inefficient, or overcomplicated, say so.
- If a simpler solution exists, recommend it before implementing.
- If you are confused, stop and explain what is unclear.

- Do not act certain when you are uncertain.

### 2. Keep the Solution Simple

Solve the requested problem with the minimum necessary code.

- Do not add features that were not asked for.
- Do not introduce abstractions for one-time use.
- Do not add configurability, extensibility, or generalization unless requested.
- Do not add defensive error handling for unrealistic cases.
- Prefer simple, readable code over clever code.
- If the solution feels too large, step back and simplify it.

### 3. Stay Strictly Within Scope

Only change what the task requires.

- Do not refactor unrelated code.
- Do not rewrite comments, formatting, or naming unless necessary for the task.
- Match the existing style and conventions of the codebase.
- Do not fix neighboring issues unless the user asked.
- If you notice unrelated problems, mention them separately instead of changing them.

### 4. Make Surgical Diffs

- Touch as few files as possible.
- Change as little code as necessary.
- Avoid broad rewrites when a targeted fix is enough.
- Preserve existing structure unless changing it is required.
- Remove only the dead code, imports, or variables created by your own changes.
- Do not delete pre-existing unused code unless asked.

### 5. Work Toward Verifiable Outcomes

Turn requests into clear success criteria whenever possible.

- "Fix the bug" - reproduce it, fix it, then verify the fix
- "Add validation" -> add checks for invalid input and verify behavior
- "Refactor this" - preserve behavior and confirm tests still pass
- "Optimize this" -> improve performance without changing correctness

Prefer tests, existing checks, or concrete validation over verbal confidence.

### 6. Read Before You Write

- Read enough nearby code to understand how the target piece fits in.
- Identify the local conventions before introducing new patterns.
- Identify the levels of abstraction in the affected code, and land new code at the right one.
- If you find inconsistencies of abstraction, or cannot place new code, stop and say so.

### 7. Preserve Intent

- Preserve comments unless they are clearly outdated and directly affected by the task.
- Preserve behavior unless the requested change is meant to alter it.
- Preserve public interfaces unless changing them is necessary.
- Call out any intentional behavior change explicitly.

Do not make hidden product or design decisions on the user's behalf. Instead, ask.

### 8. Ask for Help at the Right Time

Pause and ask if the request is ambiguous in a way that affects implementation, the codebase contains
conflicting patterns, the correct behavior is unclear, or the task requires a product or architectural
decision.

### 9. Final Check Before You Finish

Confirm that the request was addressed, the change is no larger than necessary, unrelated code was not
modified, assumptions were surfaced, affected tests or checks were run, and the result matches the
requested scope. If something could not be verified, say that clearly.

## Project

ultragraphdb is a distributed graph database in Rust, ported from Dgraph (Apache-2.0,
`github.com/dgraph-io/dgraph`). SPARQL and GQL replace Dgraph's DQL, and its GraphQL layer is left out.
Storage is fjall, the cache is moka, and replication and partitioning are part of the database. A
transaction is one request, `commit_now`, checked by its read set or by an upsert block's conditions; see
`openspec/notes/dgraph-port-overview.md` §Transactions.

### Ported code and attribution

Dgraph is licensed under Apache-2.0 and carries no NOTICE file. A file ported from Dgraph:

- keeps the copyright line of the Dgraph file it comes from, for example
  `SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.`, beside this repository's own;
- states that it was changed and names its Dgraph source path, in its header.

Code written here from scratch carries this repository's header alone:

```rust
/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */
```

### Crates

Every crate sits in a group folder named for its layer, in a folder named as the crate: `ultragraphdb_common/`,
`ultragraphdb_error/`, `ultragraphdb_proto/`, `ultragraphdb_traits/`, `ultragraphdb_core/`, `ultragraphdb_store/`,
`ultragraphdb_tool/` and `ultragraphdb_test/`. The root `Cargo.toml` takes them as members through one glob per group folder, and a
crate's Bazel package is its path, e.g. `//ultragraphdb_core/ultragraphdb_core_types`. An error raised by more than one
crate takes a crate of its own under `ultragraphdb_error/`, and a trait implemented outside its crate one under
`ultragraphdb_traits/`, so a crate naming either depends on that alone. Tests find their crate's `features/` and
`testdata/` with `ultragraphdb_tool_bdd::features_dir()` and `testdata_dir()`, from Bazel's `TEST_TARGET` or Cargo's
`CARGO_MANIFEST_DIR`, so moving a crate needs no change in its tests. A binary keeps its own name: the `ultragraph`
binary is built by `ultragraphdb_bin`.

| Crate | Path | Depends on | Provides |
|---|---|---|---|
| `ultragraphdb_common_version` | `ultragraphdb_common/ultragraphdb_common_version` | — | `PRODUCT_NAME`; the workspace's first crate |
| `ultragraphdb_error_num` | `ultragraphdb_error/ultragraphdb_error_num` | — | `NumError(pub NumErrorEnum)`: Go's `strconv.NumError`, `Syntax` and `Range`, with Go's texts; depends on nothing |
| `ultragraphdb_error_json` | `ultragraphdb_error/ultragraphdb_error_json` | — | `JsonError(pub JsonErrorEnum)`: Go's `encoding/json` errors, `io.EOF`, `io.ErrUnexpectedEOF`, `SyntaxError` with its offset, `UnmarshalTypeError` and a reader's error, with Go 1.27's texts; raised by `ultragraphdb_common_utils`' JSON decoder, carried by the chunker and types |
| `ultragraphdb_error_options` | `ultragraphdb_error/ultragraphdb_error_options` | — | `OptionsError(pub OptionsErrorEnum)`: why an index option was refused, raised by `ultragraphdb_traits_index` and tok's hnsw factory |
| `ultragraphdb_common_utils` | `ultragraphdb_common/ultragraphdb_common_utils` | `ultragraphdb_error_json`, `ultragraphdb_error_num`, `dashu-int` | The Go standard-library behaviour the port reproduces, once: modules `go_strconv` (parse, format, quote, unquote, `isprint`), `go_fmt`, `go_strings`, `go_unicode` (Unicode 17 tables generated from Go), `go_utf8`, `go_json` (`valid`, `unmarshal`, `encode_string`) and `go_farm` (go-farm's `Fingerprint64`, under its MIT notice in `LICENSE-GO-FARM`), and the types `JsonDecoder` and `JsonValue`: Go 1.27's `encoding/json`, which runs on `jsontext`, decoding into `interface{}` with `UseNumber`, nesting 10000 deep without recursion |
| `ultragraphdb_proto_dgraph` | `ultragraphdb_proto/ultragraphdb_proto_dgraph` | — | Dgraph's gRPC API, `api.proto`, generated unchanged, for the messages `pb.proto` imports from it. Built twice, see *Structure exemptions* |
| `ultragraphdb_proto_pb` | `ultragraphdb_proto/ultragraphdb_proto_pb` | `ultragraphdb_proto_dgraph` | Dgraph's internal messages and services, `pb.proto`, with the Badger messages it imports (`badgerpb4.proto`), as modules `pb` and `badgerpb4`. Generated without tonic's transport, whose `connect` clashes with Zero's `Connect` RPC. Built twice, see *Structure exemptions* |
| `ultragraphdb_traits_collator` | `ultragraphdb_traits/ultragraphdb_traits_collator` | — | `Collator`: the language collation Dgraph's sort uses, implemented by `ultragraphdb_core_types::TextCollator` |
| `ultragraphdb_traits_index` | `ultragraphdb_traits/ultragraphdb_traits_index` | `ultragraphdb_common_utils`, `ultragraphdb_error_options` | Dgraph's `tok/index` and `tok/options`: the traits `IndexFactory`, `VectorIndex`, `OptionalIndexSupport`, `OptionalSearchOptions`, `Float`, `Txn`, `LocalCache`, `CacheType`, `OptionType`, and the types their signatures name (`Options`, `AllowedOptions`, `OptionValue`, `OptionParser`, `KeyValue`, `SearchPathResult`, `VectorIndexOptions`, `SearchFilter`, `Metric`), so a vector index is implemented without depending on tok. A vector index's cache-generic methods are bounded `C: CacheType<Error = Self::Error>`, and `insert` writes through a `Txn` and takes its level draws from the caller |
| `ultragraphdb_tool_bdd` | `ultragraphdb_tool/ultragraphdb_tool_bdd` | — | The test harness every crate's suites use: `features_dir` and `testdata_dir`, a crate's `features/` and `testdata/` under both build systems; `load_golden`, `Golden<T>` and `compare_bytes` for golden vectors |
| `ultragraphdb_core_terms` | `ultragraphdb_core/ultragraphdb_core_terms` | `ultragraphdb_common_utils`, `blake2`, `oxrdf`, `oxsdatatypes` | RDF terms as the store keeps them: `EncodedTerm` and `EncodedLiteral`, the encoding by content with its canonical-form test, `KnownDatatype` and `Datatype`, `Hash128` naming a dictionary entry, the posting fingerprint, vectors (`VectorView`, `FLOAT32_VECTOR`), `TermError` |
| `ultragraphdb_core_mapping` | `ultragraphdb_core/ultragraphdb_core_mapping` | `ultragraphdb_core_terms`, `oxrdf` | The mapping contract: the canonical statement model (`Statement`, `StatementId`, `CanonicalDataset`), its RDF and property-graph views, the declarations (`PredicateDeclaration`, `TypeDeclaration`, `RelationDeclaration`, `VectorIndexSpec`), and `Changeset` with its `Change`s, the write format the engines produce and storage applies; `MappingError` |
| `ultragraphdb_core_keys` | `ultragraphdb_core/ultragraphdb_core_keys` | `ultragraphdb_core_terms` | The quad layout's keys: `QuadKey`, one variant per `Keyspace`, built and parsed back into its fields, refusing a truncated key or a zero UID with `QuadKeyError`; split keys under `SPLIT_FLAG`; `key_prefix`, `Region`, `CountDirection`, `ROOT_NAMESPACE`, `DEFAULT_GRAPH` |
| `ultragraphdb_core_codec` | `ultragraphdb_core/ultragraphdb_core_codec` | `ultragraphdb_proto_pb` | Dgraph's `codec`: `Encoder`, `Decoder`, `SeekPos` over `pb::UidPack`, and the go-groupvarint port beneath them |
| `ultragraphdb_core_algo` | `ultragraphdb_core/ultragraphdb_core_algo` | `ultragraphdb_core_codec`, `ultragraphdb_proto_pb` | Dgraph's `algo`: sorted UID-list intersection, merge and difference, packed and unpacked, `Uint64Heap` |
| `ultragraphdb_core_text` | `ultragraphdb_core/ultragraphdb_core_text` | `ultragraphdb_common_utils` | Go's `x/text/collate` with its CLDR 23 tables and `x/text/language`'s base inference: `Collator`, `Tag`, `Base`, `Confidence`, `language_base`, `LanguageError` |
| `ultragraphdb_core_types` | `ultragraphdb_core/ultragraphdb_core_types` | `ultragraphdb_common_utils`, `ultragraphdb_core_text`, `ultragraphdb_error_json`, `ultragraphdb_error_num`, `ultragraphdb_proto_dgraph`, `ultragraphdb_proto_pb`, `ultragraphdb_traits_collator` | Dgraph's `types`: `TypeId`, `Val`, conversion and marshalling, sorting and comparison, `BigFloat`, `DateTime`, passwords, geometries with their WKB and GeoJSON encodings, geo tokens and filters over a port of golang/geo's S2; `TextCollator`, `TypesError` |
| `ultragraphdb_core_tok` | `ultragraphdb_core/ultragraphdb_core_tok` | `ultragraphdb_common_utils`, `ultragraphdb_core_text`, `ultragraphdb_core_types`, `ultragraphdb_error_options`, `ultragraphdb_proto_pb`, `ultragraphdb_traits_index`, `tracing` | Dgraph's `tok`: `Tokenizer` and the registry, the bleve analysis it runs (segmentation, stop words, stemmers), the hnsw vector-index factory, `TokError` |
| `ultragraphdb_core_catalog` | `ultragraphdb_core/ultragraphdb_core_catalog` | `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_tok`, `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `oxrdf` | The catalog: `Catalog`, per namespace the declarations of its predicates, entity types and relations, the checks of a write's object against them, a declaration's store entry and the load of a namespace's catalog, refused whole as the store's `Corrupted` when an entry does not read back; `datatype_of`, `tokenizer_takes`, `CatalogError` |
| `ultragraphdb_error_store` | `ultragraphdb_error/ultragraphdb_error_store` | — | `StoreError(pub StoreErrorEnum)`: why a store operation failed (`TsTooOld`, `KeyTooLarge`, `EmptyKey`, `ReadFailed`, `Corrupted`, `WriteFailed`, `Poisoned`, `OpenFailed`, `Unsupported`, `UnknownTemporary`), with Dgraph's or Badger's text where they have one, and `go_hex_dump` for the keys they name; raised by every store and carried by every crate above |
| `ultragraphdb_traits_store` | `ultragraphdb_traits/ultragraphdb_traits_store` | `ultragraphdb_error_store` | The synchronous, batch-shaped `Store` trait and `StoreSnapshot`, with a keyspace without versions a batch writes atomically with its entries, the types their signatures name (`WriteBatch`, `PersistMode`, `Scan`, `Entry`, `Item`, `Direction`, `Temporary`, `RebuildRecord`), a rebuild's temporary keyspaces and its synced rebuild records, the `Executor` background work runs on and the `ListCache` trait, so the fjall store and the simulator are interchangeable |
| `ultragraphdb_common_telemetry` | `ultragraphdb_common/ultragraphdb_common_telemetry` | `ultragraphdb_common_version`, `tracing`, `tracing-subscriber`, `tracing-opentelemetry`, `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `tokio` | What a binary calls once at start: `init(TelemetryConfig)` installs the level filter, plain or JSON output to stderr and an optional OTLP span exporter, and returns the `TelemetryGuard` that flushes on drop; `subscriber`, `LogFormat`, `TelemetryError`. No library crate depends on it |
| `ultragraphdb_store_sim` | `ultragraphdb_store/ultragraphdb_store_sim` | `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `tracing`, `tracing-subscriber` | The deterministic storage simulator: `SimStore` with seeded fault injection (`Fault`, `FaultConfig`, `Corruption`, `DropStage`, `RebuildStage`), the shadow model `StoreModel` and its `Invariant`s, `Simulation`, `Seed` and its `Stream`s, `SimRng`, `SimClock`, `SteppedExecutor`, `SimListCache`, `LogCapture`; the `store_sim_fuzz` binary. Crates above test their own workloads on it; its README states what it injects and detects |
| `ultragraphdb_store_kv` | `ultragraphdb_store/ultragraphdb_store_kv` | `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `fjall`, `moka`, `tracing` | The production `Store` over fjall: `FjallStore` (versioned keys and a keyspace without versions, physical drops through synced drop records, journaled removals, the fenced flag, temporary keyspaces dropped at open, rebuild records), `FjallSnapshot`, `FjallScan`, `Durability`, the `GcFilter` compaction filter, `ThreadExecutor` and `MokaListCache`; the `kv_crash_writer` binary the kill tests drive |
| `ultragraphdb_store_posting` | `ultragraphdb_store/ultragraphdb_store_posting` | `ultragraphdb_common_utils`, `ultragraphdb_core_algo`, `ultragraphdb_core_codec`, `ultragraphdb_core_keys`, `ultragraphdb_error_store`, `ultragraphdb_proto_pb`, `ultragraphdb_traits_store`, `prost`, `tracing` | Dgraph's `posting` over the quad layout's list keys: `List` with its mutable layer, postings identified by uid or, where the keyspace compares values, by uid and value, rollups and splits; `Lists` (the commit with its hook under the writer lock, timestamps, drops), `Request` (a transaction's `LocalCache`), `Batch`, `CachedList`, the posting codec, `PostingError` |
| `ultragraphdb_store_hnsw` | `ultragraphdb_store/ultragraphdb_store_hnsw` | `ultragraphdb_common_utils`, `ultragraphdb_core_keys`, `ultragraphdb_core_terms`, `ultragraphdb_core_tok`, `ultragraphdb_error_options`, `ultragraphdb_proto_pb`, `ultragraphdb_store_posting`, `ultragraphdb_traits_index`, `ultragraphdb_traits_store`, `tracing` | Dgraph's `tok/hnsw` over the vector-index keyspace: `PersistentHnsw` implementing `VectorIndex`, `HnswFactory`, `VectorKeys` with the entry, neighbour and dead lists, the transaction adapter `ViTxn` reading vectors from forward lists, with `TxnCache` and `QueryCache`, the distance kernels, the neighbour-matrix codec, `HnswError` |
| `ultragraphdb_store_quads` | `ultragraphdb_store/ultragraphdb_store_quads` | `ultragraphdb_common_utils`, `ultragraphdb_core_catalog`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_error_store`, `ultragraphdb_proto_pb`, `ultragraphdb_store_posting`, `ultragraphdb_traits_store`, `oxrdf` | Quads on posting lists: `QuadStore` with the term dictionary over the keyspace without versions, `Leases` for UIDs and statement IDs, `QuadWrite` and `WriteTerms`, `StatementRecord`, `Statistics` kept by deltas, and the `DataDictionary` read API with its `Shapes`; `QuadError` |
| `ultragraphdb_store_apply` | `ultragraphdb_store/ultragraphdb_store_apply` | `ultragraphdb_core_catalog`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_core_tok`, `ultragraphdb_core_types`, `ultragraphdb_error_store`, `ultragraphdb_proto_pb`, `ultragraphdb_store_hnsw`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_traits_index`, `ultragraphdb_traits_store`, `oxrdf`, `oxsdatatypes`, `tracing` | Changesets applied to the quads: `ChangesetApplier` (writes checked against the catalog with their value, count and vector indexes, statement records and statistics; declarations checked against the data; index builds in the background with a crash-safe rebuild record; drops of a predicate, graph, namespace or everything), `Applied`, `LevelDraw`, `token_value`, `ChangesetError` |
| `ultragraphdb_tool_bench` | `ultragraphdb_tool/ultragraphdb_tool_bench` | `ultragraphdb_common_telemetry`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_error_store`, `ultragraphdb_store_apply`, `ultragraphdb_store_kv`, `ultragraphdb_store_posting`, `ultragraphdb_traits_store`, `oxrdf` | The `store_bench` binary: the apply thread at a fixed offered rate with the Dgraph runs' commits as changesets of quads, reporting lists read per relation, cache hits and store reads that found nothing; rollups and the compaction filter over one large predicate; the store's file sizes; each run one JSON line, the clock behind a `Clock` trait so the pacer and summaries are tested without it. Results in the overview's "Measured" sections |
| `ultragraphdb_tool_test` | `ultragraphdb_tool/ultragraphdb_tool_test` | `ultragraphdb_core_catalog`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_core_tok`, `ultragraphdb_error_store`, `ultragraphdb_store_apply`, `ultragraphdb_store_kv`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_store_sim`, `ultragraphdb_traits_store`, `fjall`, `oxrdf`, `tracing` | The helpers the store crates' test targets share, a dev-dependency of `ultragraphdb_store_kv` and `ultragraphdb_store_apply`: `TestDir` and the store's short write and read helpers, the crash writer's harness, the store workload run on fjall beside the simulated store (`run_both`, `Run`), and the changeset applier's workload on the simulator (`ChangesetWorkload`). A slow suite splits into test targets of their own, which Bazel runs in parallel, and each reaches the shared helpers here |

### Structure exemptions

`ultragraphdb_proto_dgraph` keeps a public `api` module at its root. Cargo builds the crate from
`src/lib.rs` via `tonic-prost-build`, while Bazel generates it from `proto/api.proto` with
`rust_prost_library` and never reads `src/lib.rs`, so the `api` module name is the contract both
toolchains agree on. Consumers import `ultragraphdb_proto_dgraph::api`, and its generated stub keeps Dgraph's name,
`ultragraphdb_proto_dgraph::api::dgraph_client::DgraphClient`.
`ultragraphdb_proto_pb` keeps its public `pb` and `badgerpb4` modules at its root for the same reason, named after
the proto packages: `ultragraphdb_proto_pb::pb::Posting`.

### External dependencies

Every external crate is pinned once in `[workspace.dependencies]` and referenced by member crates as
`{ workspace = true }`. Add a new one there or nowhere. Choose a pure-Rust crate wherever one exists (`lz4_flex` over
`zstd`, `miniz_oxide` over zlib): it keeps cross-compilation to static musl free of a C toolchain per target.

| Crate                                 | Version               | Used by                             | Why                                                                                                     |
|---------------------------------------|-----------------------|-------------------------------------|---------------------------------------------------------------------------------------------------------|
| `prost`, `tonic`, `tonic-prost`       | 0.14                  | `ultragraphdb_proto_dgraph`, `ultragraphdb_proto_pb`; `prost` also `ultragraphdb_store_posting` | Generated protobuf types and the gRPC surface; the posting codec                                        |
| `tonic-prost-build`                   | 0.14                  | `ultragraphdb_proto_dgraph`, `ultragraphdb_proto_pb` (build) | Cargo-side codegen. Bazel generates the same crate with `rust_prost_library` and the toolchains under `bazel/toolchains/` |
| `tokio`                               | =1.54.1; features per crate: `rt` (telemetry), `net`, `io-util`, `time` (client), `rt` in the client's tests | `ultragraphdb_common_telemetry`, `ultragraphdb_client_tokio` | The runtime the OTLP exporter sends on, and the primary public Rust client's: most Rust applications run tokio, so the client's first shell is a tokio one with `Send` futures (decided 2026-10-28). Pure Rust: `mio`, `socket2`, `bytes`, `pin-project-lite`, and `libc` as bindings only, no C toolchain |
| `bcrypt` | 0.19 | `ultragraphdb_core_types` | Hashes and verifies passwords as Go's bcrypt does, `$2a$` hashes included |
| `blake2` | 0.11 | `ultragraphdb_core_tok`, `ultragraphdb_core_terms` | BLAKE2b-256 of the hash tokenizer; BLAKE2b-128 of a dictionary entry's hash |
| `oxrdf`, `oxsdatatypes` | 0.3.5, `rdf-12` / 0.2.4 | `ultragraphdb_core_terms`, `ultragraphdb_core_mapping`, `ultragraphdb_core_catalog`, `ultragraphdb_store_quads`, `ultragraphdb_store_apply`, `ultragraphdb_tool_bench` | The RDF term model and the XSD datatypes' parsing and canonical forms, from Oxigraph |
| `dashu-base`, `dashu-int` | 0.6 | `ultragraphdb_core_types`; `dashu-int` also `ultragraphdb_common_utils` | Exact integer arithmetic beneath the port of Go's `math/big` for 200-bit big floats |
| `rust-stemmers` | 1.2 | `ultragraphdb_core_tok` | The Snowball stemmers of nine languages, checked against Go on golden vectors |
| `unicode-normalization` | 0.1 | `ultragraphdb_core_tok` | NFKC at Unicode 17.0.0, as Go's `x/text` v0.41 |
| `serde`, `serde_json` | 1.0 | `ultragraphdb_tool_bdd`; every core crate's tests | Golden files |
| `cucumber`, `futures` | 0.23 / 0.3 | every crate's `bdd` test | The BDD scenarios, run without the libtest harness; `futures::executor::block_on` drives them |
| `criterion` | 0.7.0, no default features | `ultragraphdb_store_kv` (benches) | The kv store's benchmarks, run by `cargo bench` and `bazel run`, never by `make test`. 0.7.0 is the newest release without `alloca`, whose build script compiles C |
| `fjall` | `=3.1.12` | `ultragraphdb_store_kv` | The LSM-tree storage engine, pure Rust. Pinned exactly: `Store::compact` calls fjall's `#[doc(hidden)]` `rotate_memtable_and_wait` and `major_compact`, which carry no stability promise |
| `moka` | 0.12.16, `sync` | `ultragraphdb_store_kv` | The posting-list cache, keyed with a per-process `RandomState` |
| `tracing` | 0.1 | every library crate that logs | Structured log events where the ported Go code logs; its macros are used in `src/` |
| `tracing-subscriber` | 0.3.23, `fmt` + `json` + `env-filter` | `ultragraphdb_common_telemetry`, `ultragraphdb_store_sim` | The level filter and plain or JSON output; the simulator's capturing subscriber |
| `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` | 0.33.1, `trace`; `grpc-tonic` | `ultragraphdb_common_telemetry` | Span export over OTLP, on the workspace's tonic 0.14 |
| `tracing-opentelemetry` | 0.34.0 | `ultragraphdb_common_telemetry` | Bridges `tracing` spans to OpenTelemetry |
| `compio` | 0.19.2, no default features; `runtime`, `io-uring`, `fs`, `net`, `time` | `ultragraphdb_protocol_transport_compio`, `ultragraphdb_client_compio`, `ultragraphdb_transactions_log` | The completion-based runtime of the workers, the write-ahead log, the transport's compio shell and the compio client, io_uring on Linux; the stable release Apache Iggy builds on. Pure Rust on every shipped target: `cc` enters only through `loom` under `cfg(loom)` |
| `xxhash-rust` | 0.8.19, `xxh3` | `ultragraphdb_protocol_wire` and the formats checksummed with it | XXH3-64, as xxHash's `doc/xxhash_spec.md` specifies it, for frame, chunk, WAL and posting-list checksums; pure Rust |
| `rustix` | `=1.1.6`, no default features; `fs` | `ultragraphdb_transactions_log`, on Linux only | `fallocate(2)` with `FALLOC_FL_KEEP_SIZE`, which reserves a new WAL segment's blocks without writing zeros; compio has no such call and the workspace forbids `unsafe`. Pinned exactly, at the release compio already resolves. Pure Rust: its Linux backend makes raw system calls with `linux-raw-sys`'s generated constants, which builds nothing, and its build script only probes `rustc`; macOS does not depend on it |
| `arbitrary` | 1.5.0, `derive` | the fuzz workspace in `fuzz/` only | Structure-aware inputs for the `cargo-fuzz` targets run by `scripts/fuzz.sh`; no shipped crate depends on it |

## Building and Running

The project uses `make` for common development tasks:

* `make build`: Builds the entire workspace with Bazel
* `make test`: Builds, then runs the doc tests and the unit tests with Bazel
* `make fix`: Fixes linting issues as reported by `clippy`
* `make format`: Formats Bazel files with `buildifier` and Rust code with `cargo fmt`
* `make check`: Checks for outdated and unused dependencies and security vulnerabilities
* `make doc`: Builds the documentation and runs the doc tests
* `make count`: Counts lines of code
* `make update`: Updates the Rust toolchain and Cargo dependencies

Bazel is the build of record; Cargo serves the IDE. Bazel resolves external crates through
`all_crate_deps()` reading the Cargo manifests, so a dependency added to a member `Cargo.toml` needs no
`BUILD.bazel` edit.

## Code structure

Each crate adheres to the following base structure

src/errors/mod.rs  # contains each error type in a separate file
src/traits/mod.rs # contains each trait in a separate file
src/types/mod.rs # contains each type in a separate file

## One type, one Rust module.

For very small types (total implementation in less than 25 lines), the type is stored in a file named as
snake_case of the type name. For example:

src/types/small_type.rs

For more complex types, the type is stored in a folder module, for example the type Uncertain is stored in:

src/types/uncertain/mod.rs

The mod.rs contains the type definition and constructors.

When the type implements multiple traits, each trait is stored within a file named after the implementing
trait or trait group. For example, when implementing PartialEq and Debug for type Uncertain, these would be
in files:

src/types/uncertain/uncertain_debug.rs
src/types/uncertain/uncertain_part_eq.rs

## Optional src folders

src/extensions/mod.rs # contains type extensions i.e. a default impl for a trait
src/utils/mod.rs # contains utils

## Test structure

Every test file must be registered to the corresponding mod file and that module must be registered with
its higher up module. Ensure the correct #[cfg(test)] annotation is set for each registered test file.

Test targets live in the crate's own `BUILD.bazel`, beside the library. A crate whose tests mirror `src/`
through `tests/mod.rs` takes one `rust_test` with `crate_root = "tests/mod.rs"`, tagged `unit-test`.
A `rust_test_suite` over `*_tests.rs` fits only when every test file stands alone.

The Rust client is three crates in `ultragraphdb_client/`, each with a test root `tests/mod.rs`,
declared as `[[test]] unit` under `autotests = false` and run by one `rust_test` tagged `unit-test`;
`ultragraphdb_client_core` adds `tests/bdd.rs`, the wire protocol's client-side scenarios, its own
`bdd` target with the harness included by path.
`ultragraphdb_client_core` is tested synchronously, with no runtime: `tests/utils/harness.rs` drives
the core against the scripted server over the simulated network, under drop, delay, duplicate and
split, moving time itself. The scripted server is `src/utils_test/`, behind the `test-utils`
feature, so the shells' tests serve the same script; the core's `BUILD.bazel` sets
`crate_features = ["test-utils"]`, and its own dev-dependency on itself turns it on under Cargo.
`ultragraphdb_client_tokio` and `ultragraphdb_client_compio` run the same conversations through
their shells, over the simulated network and a TCP loopback, with the server served from
`tests/utils/serve.rs`.

### TDD and BDD

The port is written test first, at two levels:

* **BDD, for behaviour.** Every `#### Scenario:` in a spec under `openspec/` is a Gherkin scenario of the
  same name in the owning crate's `features/` folder, run by `cucumber` from `tests/bdd.rs` with the
  libtest harness off (`harness = false` in `Cargo.toml`, `use_libtest_harness = False` in
  `BUILD.bazel`), tagged `bdd-test`. A scenario is written, and seen to fail, before the code that makes
  it pass.
* **TDD, for units.** A ported Go file's tests are ported first, under `tests/` as below, and fail
  before the code is ported. Where Dgraph's output has to be byte-identical (key encodings, tokens,
  packed UIDs, value encodings), golden vectors generated from the Go source sit under
  `testdata/golden/` and the Rust test reads them.

The port's reference is Go 1.27, the toolchain Dgraph's `go.mod` names, including its default `encoding/json`, which
runs on json v2.

The order inside every task group is the same: scenarios and tests, then code, then refactoring with
both green.

The tests folder replicates the exact src folder structure, for example:

tests/errors/mod.rs # contains tests for each error type in a separate file
tests/traits/mod.rs # Optional contains tests for each trait in a separate file
tests/types/mod.rs # contains tests for each type in a separate file

Test files replicate the source file name with an appended _tests. For example, a source file

`src/errors/normal_error/normal_error.rs`

is matched with the test file under the tests folder:

`tests/errors/normal_error/normal_error_tests.rs`

Shared utils used for testing are stored in the src tree under:

`src/utils_test/mod.rs # contains utils`

Bazel cannot access util files from within the test folder, but it can access the full src folder during
testing. As a result, test utils have to be fully tested to count towards the code coverage score.

A slow suite is split into several test targets, which Bazel runs in parallel. Helpers that more than one of those
targets uses live in `ultragraphdb_tool/ultragraphdb_tool_test`, a crate the test targets depend on.

The usage of a prelude file is prohibited.

## Code export

* All public types, traits, and errors are exported from src/lib.rs
* internal modules remain private at the root level.

## Code import

When importing from a crate, always import directly from the root level, for example:

`use ultragraphdb_common_version::PRODUCT_NAME;`

## Code conventions

Field visibility:
* Public types: All fields will be private, and access will be provided through constructors, getters,
  and setters as appropriate.
* Private types: Public fields may be used, provided they do not leak outside their defined scope.

Errors:
* Every error is `pub struct FooError(pub FooErrorEnum)` over an exhaustive enum, with one constructor per variant
  (`FooError::Variant(..)`), `Display` and `std::error::Error`, as `deep_causality_physics`' `PhysicsError` has it. The
  inner field is the one public field a public type carries, so callers match on `e.0`.
* `Display` gives Dgraph's exact text wherever Dgraph has one: clients and tests match on it.

Panics:
* Code under `src/` does not panic on any input. Where Dgraph panics, the port returns its crate's error with a clear
  message; the golden file keeps Go's panic as evidence, and the test names the case as a deviation. An `expect`
  remains only where a local invariant proves it cannot fire, and its message states the invariant.
* `ultragraphdb_tool_bdd`, the test harness, panics with a message naming the file or variable at fault and the fault,
  because its callers are tests and a panic is how a test fails. Each panicking function states when under `# Panics`.

Static Dispatch:
* Use static dispatch
* Avoid usage of dyn, trait objects, and dynamic dispatch

Coding style:
* Prefer idiomatic zero cost abstractions
* Prefer functional style i.e. map, flatmap, filter when dealing with collections

Bazel targets:
* Every Bazel target is hermetic and sandboxed, because the cache is only correct for a target that reads nothing but
  its declared inputs. No target carries `local`, `external`, `no-sandbox` or a similar tag, and no test reaches files
  outside its declared `data`.

Safety and security style:
* No `unsafe`. Every crate opts into the workspace lints with `[lints]` and `workspace = true` in its own
  `Cargo.toml`; new crates MUST include this.
* Define no macros in lib code, i.e. everything under /src; macros defined in tests are permissible when used
  sparingly, e.g. for bulk testing many types implementing the same trait. Macros from external crates, such as
  `tracing`'s, are used freely.

Logging and telemetry:
* Library crates log through `tracing`: a structured event (fields, not a formatted string) wherever the ported Go code
  logs (`glog`, `x.Printf`, Badger's logger), at Dgraph's level, and wherever the port survives a failure an operator
  needs to see. A library crate never installs a subscriber.
* A binary installs the subscriber once, through `ultragraphdb_common_telemetry`: the level filter, plain or JSON
  lines, and spans exported over OTLP when an endpoint is configured.
* Simulator tests capture events with the simulator's subscriber, which records no wall-clock time, so a seed still
  replays.
* Avoid the introduction of external crates unless it is necessary.
