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
transaction is read-write or read-only and may stay open across requests: it reads a snapshot at the visibility
watermark, records its read set, and commits through the commit step, which validates it exactly against the conflict
history, runs its commit-time operations, appends its round to the write-ahead log and acknowledges it once the
watermark passes it; a session makes each commit exactly-once. The `transactions` and `commit-rounds` specs state the
rules.

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
`ultragraphdb_error/`, `ultragraphdb_traits/`, `ultragraphdb_core/`, `ultragraphdb_store/`, `ultragraphdb_protocol/`,
`ultragraphdb_transactions/`, `ultragraphdb_client/` and `ultragraphdb_tool/`. The root `Cargo.toml` takes them as
members through one glob per group folder, and a crate's Bazel package is its path, e.g.
`//ultragraphdb_core/ultragraphdb_core_types`. An error raised by more than one crate takes a crate of its own under
`ultragraphdb_error/`, and a trait implemented outside its crate one under `ultragraphdb_traits/`, so a crate naming
either depends on that alone. Tests find their crate's `features/` and `testdata/` with
`ultragraphdb_tool_bdd::features_dir()` and `testdata_dir()`, from Bazel's `TEST_TARGET` or Cargo's
`CARGO_MANIFEST_DIR`, so moving a crate needs no change in its tests. A binary keeps its own name: `store_bench` and
`wal_bench` are built by `ultragraphdb_tool_bench`.

| Crate | Path | Depends on | Provides |
|---|---|---|---|
| `ultragraphdb_common_version` | `ultragraphdb_common/ultragraphdb_common_version` | — | `PRODUCT_NAME`; the workspace's first crate |
| `ultragraphdb_error_invariant` | `ultragraphdb_error/ultragraphdb_error_invariant` | — | `InvariantError(pub InvariantErrorEnum)` and `invariant(holds, name)`, the check of Ultra Style rule 5 (`openspec/protocols/ULTRA_STYLE.md`): a violation panics at the caller in a debug build and is returned in a release build, which the component it was found in fails-stop on; depends on nothing. Its `BUILD.bazel` also builds and tests the crate with debug assertions off, so both halves run |
| `ultragraphdb_error_num` | `ultragraphdb_error/ultragraphdb_error_num` | — | `NumError(pub NumErrorEnum)`: Go's `strconv.NumError`, `Syntax` and `Range`, with Go's texts; depends on nothing |
| `ultragraphdb_error_json` | `ultragraphdb_error/ultragraphdb_error_json` | — | `JsonError(pub JsonErrorEnum)`: Go's `encoding/json` errors, `io.EOF`, `io.ErrUnexpectedEOF`, `SyntaxError` with its offset, `UnmarshalTypeError` and a reader's error, with Go 1.27's texts; raised by `ultragraphdb_common_utils`' JSON decoder, carried by the chunker and types |
| `ultragraphdb_error_options` | `ultragraphdb_error/ultragraphdb_error_options` | — | `OptionsError(pub OptionsErrorEnum)`: why an index option was refused, raised by `ultragraphdb_traits_index` and tok's hnsw factory |
| `ultragraphdb_common_utils` | `ultragraphdb_common/ultragraphdb_common_utils` | `ultragraphdb_error_json`, `ultragraphdb_error_num`, `dashu-int` | The Go standard-library behaviour the port reproduces, once: modules `go_strconv` (parse, format, quote, unquote, `isprint`), `go_fmt`, `go_strings`, `go_unicode` (Unicode 17 tables generated from Go), `go_utf8`, `go_json` (`valid`, `unmarshal`, `encode_string`) and `go_farm` (go-farm's `Fingerprint64`, under its MIT notice in `LICENSE-GO-FARM`), and the types `JsonDecoder` and `JsonValue`: Go 1.27's `encoding/json`, which runs on `jsontext`, decoding into `interface{}` with `UseNumber`, nesting 10000 deep without recursion |
| `ultragraphdb_traits_collator` | `ultragraphdb_traits/ultragraphdb_traits_collator` | — | `Collator`: the language collation Dgraph's sort uses, implemented by `ultragraphdb_core_types::TextCollator` |
| `ultragraphdb_traits_index` | `ultragraphdb_traits/ultragraphdb_traits_index` | `ultragraphdb_common_utils`, `ultragraphdb_error_options` | Dgraph's `tok/index` and `tok/options`: the traits `IndexFactory`, `VectorIndex`, `OptionalIndexSupport`, `OptionalSearchOptions`, `Float`, `Txn`, `LocalCache`, `CacheType`, `OptionType`, and the types their signatures name (`Options`, `AllowedOptions`, `OptionValue`, `OptionParser`, `KeyValue`, `SearchPathResult`, `VectorIndexOptions`, `SearchFilter`, `Metric`), so a vector index is implemented without depending on tok. A vector index's cache-generic methods are bounded `C: CacheType<Error = Self::Error>`, and `insert` writes through a `Txn` and takes its level draws from the caller |
| `ultragraphdb_tool_bdd` | `ultragraphdb_tool/ultragraphdb_tool_bdd` | `serde`, `serde_json` | The test harness every crate's suites use: `features_dir` and `testdata_dir`, a crate's `features/` and `testdata/` under both build systems; `load_golden`, `Golden<T>` and `compare_bytes` for golden vectors; `fuzz_regressions`, the minimized inputs fuzzing found, committed under `testdata/fuzz_regressions/<target>/`, for a unit test to replay |
| `ultragraphdb_core_terms` | `ultragraphdb_core/ultragraphdb_core_terms` | `oxrdf`, `oxsdatatypes`, `sha2`, `xxhash-rust` | RDF terms as the store keeps them: `EncodedTerm` and `EncodedLiteral`, the encoding by content with its canonical-form test, `KnownDatatype` and `Datatype`, `Hash128` naming a dictionary entry, the posting fingerprint, vectors (`VectorView`, `FLOAT32_VECTOR`), `TermError` |
| `ultragraphdb_core_mapping` | `ultragraphdb_core/ultragraphdb_core_mapping` | `ultragraphdb_core_terms`, `oxrdf` | The mapping contract: the canonical statement model (`Statement`, `StatementId`, `CanonicalDataset`), its RDF and property-graph views, the declarations (`PredicateDeclaration`, `TypeDeclaration`, `RelationDeclaration`, `VectorIndexSpec`), and `Changeset` with its `Change`s, the write format the engines produce and storage applies; `MappingError` |
| `ultragraphdb_core_keys` | `ultragraphdb_core/ultragraphdb_core_keys` | `ultragraphdb_core_terms` | The quad layout's keys: `QuadKey`, one variant per `Keyspace`, built and parsed back into its fields, refusing a truncated key or a zero UID with `QuadKeyError`; split keys under `SPLIT_FLAG`; `key_prefix`, `Region`, `CountDirection`, `ROOT_NAMESPACE`, `DEFAULT_GRAPH` |
| `ultragraphdb_core_codec` | `ultragraphdb_core/ultragraphdb_core_codec` | `ultragraphdb_error_store`, `xxhash-rust` | Dgraph's `codec`: `Encoder`, `Decoder`, `SeekPos` over `UidPack`, and the go-groupvarint port beneath them, byte-identical to Dgraph's; the posting lists' types of the repository's own in place of Dgraph's protobuf messages (`PostingList`, `Posting`, `PostingType`, `UidPack`, `UidBlock`, `List`, `Facet`, `BackupPostingList`), carrying the fields the code reads, and their stored format: a version byte, flags, the commit timestamp, pack blocks, postings and splits under an XXH3-64 trailer (`PostingList::encode` and `decode`), a truncation or a flipped bit refused as `CodecError` naming the key |
| `ultragraphdb_core_algo` | `ultragraphdb_core/ultragraphdb_core_algo` | `ultragraphdb_core_codec` | Dgraph's `algo`: sorted UID-list intersection, merge and difference, packed and unpacked, `Uint64Heap` |
| `ultragraphdb_core_text` | `ultragraphdb_core/ultragraphdb_core_text` | `ultragraphdb_common_utils` | Go's `x/text/collate` with its CLDR 23 tables and `x/text/language`'s base inference: `Collator`, `Tag`, `Base`, `Confidence`, `language_base`, `LanguageError` |
| `ultragraphdb_core_types` | `ultragraphdb_core/ultragraphdb_core_types` | `ultragraphdb_common_utils`, `ultragraphdb_core_text`, `ultragraphdb_error_json`, `ultragraphdb_error_num`, `ultragraphdb_traits_collator` | Dgraph's `types`: `TypeId`, `Val`, conversion and marshalling, sorting and comparison, `BigFloat`, `DateTime`, passwords, geometries with their WKB and GeoJSON encodings, geo tokens and filters over a port of golang/geo's S2; `TextCollator`, `TypesError` |
| `ultragraphdb_core_tok` | `ultragraphdb_core/ultragraphdb_core_tok` | `ultragraphdb_common_utils`, `ultragraphdb_core_text`, `ultragraphdb_core_types`, `ultragraphdb_error_options`, `ultragraphdb_traits_index`, `tracing` | Dgraph's `tok`: `Tokenizer` and the registry, the bleve analysis it runs (segmentation, stop words, stemmers), the hnsw vector-index factory, `TokError` |
| `ultragraphdb_core_catalog` | `ultragraphdb_core/ultragraphdb_core_catalog` | `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_tok`, `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `oxrdf` | The catalog: `Catalog`, per namespace the declarations of its predicates, entity types and relations, the checks of a write's object against them, a declaration's store entry and the load of a namespace's catalog, refused whole as the store's `Corrupted` when an entry does not read back; `datatype_of`, `tokenizer_takes`, `CatalogError` |
| `ultragraphdb_error_store` | `ultragraphdb_error/ultragraphdb_error_store` | — | `StoreError(pub StoreErrorEnum)`: why a store operation failed (`TsTooOld`, `KeyTooLarge`, `EmptyKey`, `ReadFailed`, `Corrupted`, `WriteFailed`, `Poisoned`, `OpenFailed`, `Unsupported`, `UnknownTemporary`), with Dgraph's or Badger's text where they have one, and `go_hex_dump` for the keys they name; raised by every store and carried by every crate above |
| `ultragraphdb_error_protocol` | `ultragraphdb_error/ultragraphdb_error_protocol` | — | `ProtocolError(pub ProtocolErrorEnum)`: why a frame of the binary protocol was refused, one variant per validation rule; depends on nothing |
| `ultragraphdb_error_log` | `ultragraphdb_error/ultragraphdb_error_log` | — | `LogError(pub LogErrorEnum)`: why an operation of the ordered log failed, split by what the caller does about it; depends on nothing |
| `ultragraphdb_traits_store` | `ultragraphdb_traits/ultragraphdb_traits_store` | `ultragraphdb_error_store` | The synchronous, batch-shaped `Store` trait and `StoreSnapshot`, with a keyspace without versions a batch writes atomically with its entries, the types their signatures name (`WriteBatch`, `PersistMode`, `Scan`, `Entry`, `Item`, `Direction`, `Temporary`, `RebuildRecord`), a rebuild's temporary keyspaces and its synced rebuild records, the `Executor` background work runs on and the `ListCache` trait, so the fjall store and the simulator are interchangeable |
| `ultragraphdb_traits_log` | `ultragraphdb_traits/ultragraphdb_traits_log` | `ultragraphdb_error_log` | `OrderedLog`, the ordered log the commit step writes its rounds to: `append` stages a round at the position after the tail, naming the tail's checksum as its parent, `sync` makes the staged rounds durable, then `committed`, `tail`, `read`, `trim_through`, `checkpoint_version`, `seal` and `next_event`; with `Position`, `Round`, `RoundRef` and `LogEvent`. The deciding methods are synchronous and do no I/O; those waiting on storage return futures. Implemented by `ultragraphdb_transactions_log`'s `Wal` |
| `ultragraphdb_common_telemetry` | `ultragraphdb_common/ultragraphdb_common_telemetry` | `ultragraphdb_common_version`, `tracing`, `tracing-subscriber`, `tracing-opentelemetry`, `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-http`, `async-trait` | What a binary calls once at start: `init(TelemetryConfig)` installs the level filter, plain or JSON output to stderr and an optional OTLP/HTTP span exporter, and returns the `TelemetryGuard` that flushes on drop; `PlainHttpClient`, the exporter's `std::net` HTTP/1.1 client, `http://` only and bounded by a timeout, which blocks on the batch processor's own thread, so no async runtime is needed; `subscriber`, `LogFormat`, `TelemetryError`. No library crate depends on it |
| `ultragraphdb_store_sim` | `ultragraphdb_store/ultragraphdb_store_sim` | `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `tracing`, `tracing-subscriber` | The deterministic storage simulator: `SimStore` with seeded fault injection (`Fault`, `FaultConfig`, `Corruption`, `DropStage`, `RebuildStage`), the shadow model `StoreModel` and its `Invariant`s, `Simulation`, `Seed` and its `Stream`s, `SimRng`, `SimClock`, `SteppedExecutor`, `SimListCache`, `LogCapture`; the `store_sim_fuzz` binary. Crates above test their own workloads on it; its README states what it injects and detects |
| `ultragraphdb_store_kv` | `ultragraphdb_store/ultragraphdb_store_kv` | `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `fjall`, `moka`, `tracing` | The production `Store` over fjall: `FjallStore` (versioned keys and a keyspace without versions, physical drops through synced drop records, journaled removals, the fenced flag, temporary keyspaces dropped at open, rebuild records), `FjallSnapshot`, `FjallScan`, `Durability`, the `GcFilter` compaction filter, `ThreadExecutor` and `MokaListCache`; the `kv_crash_writer` binary the kill tests drive |
| `ultragraphdb_store_posting` | `ultragraphdb_store/ultragraphdb_store_posting` | `ultragraphdb_common_utils`, `ultragraphdb_core_algo`, `ultragraphdb_core_codec`, `ultragraphdb_core_keys`, `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `tracing`, `xxhash-rust` | Dgraph's `posting` over the quad layout's list keys: `List` with its mutable layer, postings identified by uid or, where the keyspace compares values, by uid and value, rollups and splits; `Lists` (the commit with its hook under the writer lock, timestamps, drops), `Request` (a transaction's `LocalCache`), `Batch`, `CachedList`, a list's store entries marshalled in `ultragraphdb_core_codec`'s format (`marshal`, `unmarshal_complete`, `unmarshal_delta`), `PostingError` |
| `ultragraphdb_store_hnsw` | `ultragraphdb_store/ultragraphdb_store_hnsw` | `ultragraphdb_common_utils`, `ultragraphdb_core_codec`, `ultragraphdb_core_keys`, `ultragraphdb_core_terms`, `ultragraphdb_core_tok`, `ultragraphdb_error_options`, `ultragraphdb_store_posting`, `ultragraphdb_traits_index`, `ultragraphdb_traits_store`, `tracing` | Dgraph's `tok/hnsw` over the vector-index keyspace: `PersistentHnsw` implementing `VectorIndex`, `HnswFactory`, `VectorKeys` with the entry, neighbour and dead lists, the transaction adapter `ViTxn` reading vectors from forward lists, with `TxnCache` and `QueryCache`, the distance kernels, the neighbour-matrix codec, `HnswError` |
| `ultragraphdb_store_quads` | `ultragraphdb_store/ultragraphdb_store_quads` | `ultragraphdb_core_catalog`, `ultragraphdb_core_codec`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_error_store`, `ultragraphdb_store_posting`, `ultragraphdb_traits_store`, `oxrdf`, `xxhash-rust` | Quads on posting lists: `QuadStore` with the term dictionary over the keyspace without versions, `Leases` for UIDs and statement IDs, `QuadWrite` and `WriteTerms`, `StatementRecord`, `Statistics` kept by deltas, and the `DataDictionary` read API with its `Shapes`; `QuadError` |
| `ultragraphdb_store_apply` | `ultragraphdb_store/ultragraphdb_store_apply` | `ultragraphdb_core_catalog`, `ultragraphdb_core_codec`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_core_tok`, `ultragraphdb_core_types`, `ultragraphdb_error_store`, `ultragraphdb_store_hnsw`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_traits_index`, `ultragraphdb_traits_store`, `oxrdf`, `oxsdatatypes`, `tracing` | Changesets applied to the quads: `ChangesetApplier` (writes checked against the catalog with their value, count and vector indexes, statement records and statistics; declarations checked against the data; index builds in the background with a crash-safe rebuild record; drops of a predicate, graph, namespace or everything; a transaction's changesets executed against its snapshot as an `Execution`, sealed as `Executed`, and a round's `Executed`s applied at their commit versions in one store batch, `apply_round_with`), `Applied`, `LevelDraw`, `token_value`, `ChangesetError` |
| `ultragraphdb_protocol_wire` | `ultragraphdb_protocol/ultragraphdb_protocol_wire` | `ultragraphdb_core_terms`, `ultragraphdb_error_protocol`, `xxhash-rust` | The binary protocol's bytes as `ultragraphdb_protocol/PROTOCOL.md` specifies them, with no I/O: the 256-byte `Header` and `HeaderView`, XXH3-64 checksums and chunk tables, `FrameView`'s validation in the protocol's order, the `Command`s, and every body's view and builder (`BodyView`, `BodyBuilder`): the handshake and authentication, transactions, reads and writes, prepared statements and describe, columnar `ResultBatch`es with their term tables, errors, the `Round` frame and the `WriteSetRecord` the write-ahead log holds; every value read in place with `from_le_bytes`; `StatementError` |
| `ultragraphdb_protocol_transport` | `ultragraphdb_protocol/ultragraphdb_protocol_transport` | `ultragraphdb_protocol_wire`, `ultragraphdb_error_protocol` | The transport that carries the wire protocol, sans-I/O: synchronous state machines with no runtime, which a runtime's shell drives. `Framer` assembles frames pull-style into a `BufferPool`'s `PooledBuffer`s, verifying the header before its size is trusted; `ClientHandshake` and `ServerHandshake`; `Outbound`, a server's frames scheduled across streams under credit and cancellation; `ServerConnection`, a server's whole connection with authentication through an `Authenticator`; `ClientAuthentication` through a `Login`'s `ClientMechanism`; `SimNetwork` and `SimStream`, a seeded network that drops, delays, duplicates and splits; `TransportError` |
| `ultragraphdb_protocol_scram` | `ultragraphdb_protocol/ultragraphdb_protocol_scram` | `ultragraphdb_protocol_transport`, `sha2`, `hmac`, `subtle`, `base64`, `stringprep`, `getrandom` | SCRAM-SHA-256 (RFC 5802, RFC 7677) for the Authenticate exchange, sans-I/O, passwords prepared with SASLprep (RFC 4013), without channel binding: `ScramClient` and `ScramLogin`, `ScramAuthenticator` and `ScramServer`, `ScramVerifier` (a password's salt, iteration count and two derived keys, never the password), `system_nonce`, `ScramError` |
| `ultragraphdb_protocol_transport_compio` | `ultragraphdb_protocol/ultragraphdb_protocol_transport_compio` | `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire`, `compio` | The transport's compio shell, which the compio client uses: `Connection` does a connection's I/O over a compio stream as the `Framer` and the handshakes direct; `CompioSimStream`, a simulated stream as a compio stream |
| `ultragraphdb_transactions_txn` | `ultragraphdb_transactions/ultragraphdb_transactions_txn` | `ultragraphdb_core_codec`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_apply`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_traits_store`, `oxrdf` | A transaction on a worker: `Transaction` begins at the visibility watermark or at an earlier version it names, a version above the watermark refused at once; its changesets execute against its snapshot through one held execution of the changeset applier, so later requests read them, and its reads are recorded in a `ReadSet`; its commit is a `Prepared`, carrying the write-set record `WriteSetBuilder` writes in the wire's layout. `TxnLimits` (10 s read-write, 16 s read-only, the largest write-set record and read set), `TxnBuffers` lent by the worker so the steady state allocates no read set or record buffer, `Mode`, `Operated`, `TxnError` |
| `ultragraphdb_transactions_log` | `ultragraphdb_transactions/ultragraphdb_transactions_log` | `ultragraphdb_error_log`, `ultragraphdb_error_protocol`, `ultragraphdb_protocol_wire`, `ultragraphdb_traits_log`, `compio`, `tracing`; `rustix` on Linux | The write-ahead log: `Wal`, implementing `OrderedLog`, with rounds as checksummed Round frames chained by their parents' checksums, written at 4,096-byte boundaries into segments of a fixed size; the two-copy `Superblock` and the three outcomes of reading it (`SuperblockRead`); recovery's scan (`scan_segment`, `find_complete_round`), which truncates a torn tail and reports it (`Truncation`) and refuses interior damage (`Damage`); trimmed segments zeroed and recycled; `WalConfig` with the power-loss-protection flag, which sets round bounds and the checkpoint interval and never a sync. The storage seam `DurableStorage` and `DurableFile`, implemented by `CompioStorage` (`O_DIRECT`, `fdatasync` and `fallocate` on Linux; on macOS a buffered file whose sync is `fsync`, which leaves the drive's cache unflushed) and by the deterministic `SimStorage` with its `FaultMode`s; `Recovery` |
| `ultragraphdb_transactions_commit` | `ultragraphdb_transactions/ultragraphdb_transactions_commit` | `ultragraphdb_core_keys`, `ultragraphdb_error_log`, `ultragraphdb_error_protocol`, `ultragraphdb_error_store`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_apply`, `ultragraphdb_store_posting`, `ultragraphdb_traits_log`, `ultragraphdb_traits_store`, `ultragraphdb_transactions_txn` | The commit step: `CommitStep`, a step function over `Input`s (commit requests, sync completions, the oldest open read) filling a `StepOutput` (the round it closed, the synced round to apply, the replies), so the simulator drives it without a runtime; a `Sequencer` giving even commit versions, `max(previous + 2, 2 × clock µs)`; a `Resolver` deciding each transaction in round order against the `ConflictHistory`, exactly; commit-time operations through an `Operator` (`ApplierOperator`, `NoOperations`); `Sessions` for exactly-once commits; `Pipeline`, the step on one node, each round appended to an `OrderedLog` and synced, then applied to the changeset applier, which moves the visibility watermark; `Replay`, recovery from the log; `RoundBounds`, `RoundRecord`, `Reply`, `CommitError`, `PipelineError`, `RecoveryError`; the criterion bench `history_bench` |
| `ultragraphdb_client_core` | `ultragraphdb_client/ultragraphdb_client_core` | `ultragraphdb_core_terms`, `ultragraphdb_error_protocol`, `ultragraphdb_protocol_scram`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire` | The Rust client as a sans-I/O core: `ClientCore` holds every rule of the protocol a client follows (the handshake, authentication, `Session`s, `Transaction`s, prepared `Statement`s, describe, `ResultStream`s with credit and cancel, `Reply`s read in place through the wire's views); it takes the frames its shell read and the time with every call that may start a wait, and queues the frames to write. A commit is retried only through its session, with the same request number, so the server applies it once. `ClientConfig`, `ClientError`; behind the `test-utils` feature, `TestServer`, the scripted server the client's tests run against |
| `ultragraphdb_client_tokio` | `ultragraphdb_client/ultragraphdb_client_tokio` | `ultragraphdb_client_core`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire`, `tokio` | The primary public Rust client: `Client`, the core's tokio shell, its futures `Send` over `TcpConnector`, and `TokioSimStream` for the simulated network; the core's types re-exported |
| `ultragraphdb_client_compio` | `ultragraphdb_client/ultragraphdb_client_compio` | `ultragraphdb_client_core`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_protocol_wire`, `compio` | The public compio client: `Client`, the core's compio shell over the transport's compio `Connection`, thread per core, io_uring on Linux; the core's types re-exported |
| `ultragraphdb_tool_bench` | `ultragraphdb_tool/ultragraphdb_tool_bench` | `ultragraphdb_common_telemetry`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_error_store`, `ultragraphdb_store_apply`, `ultragraphdb_store_kv`, `ultragraphdb_store_posting`, `ultragraphdb_traits_log`, `ultragraphdb_traits_store`, `ultragraphdb_transactions_commit`, `ultragraphdb_transactions_log`, `ultragraphdb_transactions_txn`, `compio`, `oxrdf` | The `store_bench` binary: the apply thread at a fixed offered rate with the Dgraph runs' commits as changesets of quads, reporting lists read per relation, cache hits and store reads that found nothing; rollups and the compaction filter over one large predicate; the store's file sizes. The `wal_bench` binary: the same commits as transactions through the commit pipeline, each round synced in the write-ahead log on compio and applied to fjall, reporting the rounds and the bytes the log's segments and fjall's journal took. Each run one JSON line, the clock behind a `Clock` trait so the pacer and summaries are tested without it. Results in the overview's "Measured" sections |
| `ultragraphdb_tool_test` | `ultragraphdb_tool/ultragraphdb_tool_test` | `ultragraphdb_core_catalog`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_core_tok`, `ultragraphdb_error_store`, `ultragraphdb_store_apply`, `ultragraphdb_store_kv`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_store_sim`, `ultragraphdb_traits_store`, `fjall`, `oxrdf`, `tracing` | The helpers the store crates' test targets share, a dev-dependency of `ultragraphdb_store_kv` and `ultragraphdb_store_apply`: `TestDir` and the store's short write and read helpers, the crash writer's harness, the store workload run on fjall beside the simulated store (`run_both`, `Run`), and the changeset applier's workload on the simulator (`ChangesetWorkload`). A slow suite splits into test targets of their own, which Bazel runs in parallel, and each reaches the shared helpers here |

### External dependencies

Every external crate is pinned once in `[workspace.dependencies]` and referenced by member crates as
`{ workspace = true }`. Add a new one there or nowhere. Choose a pure-Rust crate wherever one exists (`lz4_flex` over
`zstd`, `miniz_oxide` over zlib): it keeps cross-compilation to static musl free of a C toolchain per target.

| Crate                                 | Version               | Used by                             | Why                                                                                                     |
|---------------------------------------|-----------------------|-------------------------------------|---------------------------------------------------------------------------------------------------------|
| `tokio`                               | 1.54; features: `net`, `io-util`, `time`, and `rt` in the client's tests | `ultragraphdb_client_tokio` | The primary public Rust client's runtime: most Rust applications run tokio, so the client's first shell is a tokio one with `Send` futures (decided 2026-10-28). Pure Rust: `mio`, `socket2`, `bytes`, `pin-project-lite`, and `libc` as bindings only, no C toolchain |
| `bcrypt` | 0.19 | `ultragraphdb_core_types` | Hashes and verifies passwords as Go's bcrypt does, `$2a$` hashes included |
| `oxrdf`, `oxsdatatypes` | 0.3, `rdf-12` / 0.2 | `ultragraphdb_core_terms`, `ultragraphdb_core_mapping`, `ultragraphdb_core_catalog`, `ultragraphdb_store_quads`, `ultragraphdb_store_apply`, `ultragraphdb_tool_bench` | The RDF term model and the XSD datatypes' parsing and canonical forms, from Oxigraph |
| `dashu-int` | 0.6 | `ultragraphdb_core_types`, `ultragraphdb_common_utils` | Exact integer arithmetic beneath the port of Go's `math/big` for 200-bit big floats; `dashu_int::ops` re-exports the traits of `dashu-base` it builds on |
| `rust-stemmers` | 1.2 | `ultragraphdb_core_tok` | The Snowball stemmers of nine languages, checked against Go on golden vectors |
| `unicode-normalization` | 0.1 | `ultragraphdb_core_tok` | NFKC at Unicode 17.0.0, as Go's `x/text` v0.41 |
| `serde`, `serde_json` | 1.0 | `ultragraphdb_tool_bdd`; every core crate's tests | Golden files |
| `cucumber`, `futures` | 0.23 / 0.3 | every crate's `bdd` test | The BDD scenarios, run without the libtest harness; `futures::executor::block_on` drives them |
| `criterion` | 0.8, no default features | `ultragraphdb_store_kv`, `ultragraphdb_transactions_commit`, `ultragraphdb_transactions_txn` (benches) | The kv store's benchmarks, the conflict history's and the read set's, run by `cargo bench` and `bazel run`, never by `make test`. 0.8 depends on `alloca`, whose build script compiles C through `cc`, in the bench targets alone |
| `fjall` | 3.1 | `ultragraphdb_store_kv` | The LSM-tree storage engine, pure Rust. `Store::compact` calls fjall's `#[doc(hidden)]` `rotate_memtable_and_wait` and `major_compact`, which carry no stability promise |
| `moka` | 0.12, `sync` | `ultragraphdb_store_kv` | The posting-list cache, keyed with a per-process `RandomState` |
| `tracing` | 0.1 | every library crate that logs | Structured log events where the ported Go code logs; its macros are used in `src/` |
| `tracing-subscriber` | 0.3, `fmt` + `json` + `env-filter` | `ultragraphdb_common_telemetry`, `ultragraphdb_store_sim` | The level filter and plain or JSON output; the simulator's capturing subscriber |
| `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-http`, `async-trait` | 0.33, `trace`; `http-proto` / 0.33, no default features / 0.1 | `ultragraphdb_common_telemetry` | Span export over OTLP/HTTP with protobuf, `prost` beneath `opentelemetry-otlp`, sent by the crate's own `PlainHttpClient` through `opentelemetry-http`'s `HttpClient` trait, which `async-trait` implements. A bundled client would add hyper (68 crates in all) or reqwest (100) where the own client keeps it at 49, against 79 over gRPC on tonic |
| `tracing-opentelemetry` | 0.34 | `ultragraphdb_common_telemetry` | Bridges `tracing` spans to OpenTelemetry |
| `compio` | 0.19, no default features; `runtime`, `io-uring`, `fs`, `net`, `time` | `ultragraphdb_protocol_transport_compio`, `ultragraphdb_client_compio`, `ultragraphdb_transactions_log`, `ultragraphdb_tool_bench` | The completion-based runtime of the write-ahead log, the transport's compio shell, the compio client and `wal_bench`, io_uring on Linux; the stable release Apache Iggy builds on. Pure Rust on every shipped target: `cc` enters only through `loom` under `cfg(loom)` |
| `xxhash-rust` | 0.8, `xxh3` | `ultragraphdb_protocol_wire`, `ultragraphdb_core_codec`, `ultragraphdb_core_terms`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads` | XXH3-64, as xxHash's `doc/xxhash_spec.md` specifies it: the one non-cryptographic hash, for frame, chunk, WAL and posting-list checksums, the posting fingerprint, the rollup queue's deduplication and the statistics' sketches; pure Rust |
| `sha2`, `hmac`, `subtle`, `base64`, `stringprep` | 0.11 / 0.13 / 2.6 / 0.23 / 0.1 | `ultragraphdb_protocol_scram`; `sha2` also `ultragraphdb_core_terms`, `ultragraphdb_core_tok` | SHA-256 is the one cryptographic hash: SCRAM-SHA-256 (RFC 5802, RFC 7677) of the Authenticate exchange with HMAC and the constant-time comparison of a proof, from RustCrypto, and `SaltedPassword`'s `Hi`, PBKDF2's one block, written over that HMAC in `ultragraphdb_protocol_scram`; a dictionary entry's 128-bit hash (SHA-256 truncated to 16 bytes); the hash tokenizer's digest and a long shingle's. The base64 of SCRAM's messages; SASLprep (RFC 4013) of names and passwords. Pure Rust |
| `getrandom` | 0.4 | `ultragraphdb_protocol_scram` | The operating system's random source, for SCRAM's client and server nonces (`system_nonce`); already in the lockfile beneath other crates. Pure Rust: system calls through `libc`'s bindings |
| `rustix` | 1.1, no default features; `fs` | `ultragraphdb_transactions_log`, on Linux only | `fallocate(2)` with `FALLOC_FL_KEEP_SIZE`, which reserves a new WAL segment's blocks without writing zeros; compio has no such call and the workspace forbids `unsafe`. Pure Rust: its Linux backend makes raw system calls with `linux-raw-sys`'s generated constants, which builds nothing, and its build script only probes `rustc`; macOS does not depend on it |
| `arbitrary` | 1.5, `derive` | the fuzz workspace in `fuzz/` only | Structure-aware inputs for the `cargo-fuzz` targets run by `scripts/fuzz.sh`; no shipped crate depends on it |

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

Two tools run through Cargo, as approved exceptions to building and testing with Bazel alone, because neither is
hermetic nor has a fixed end:

* `scripts/mutants.sh [crate [file]]`: mutation testing with `cargo-mutants`, which changes one expression at a time
  in a copy of the tree and runs the crate's suite. A survivor is a gap in the suite until a test kills it or an entry
  in `.cargo/mutants.toml` records why the mutant cannot change behaviour.
* `scripts/fuzz.sh [crate [target]]`: fuzzing with `cargo-fuzz` over libFuzzer, every target time-boxed
  (`FUZZ_SECONDS`, 60 by default), reporting executions, coverage and failing inputs. The targets are packages at
  `fuzz/<crate>/`, one workspace of their own in `fuzz/` built with a nightly toolchain, which the root workspace
  excludes and Bazel never builds, as `libfuzzer-sys` compiles C++. A target's seeds are its crate's
  `testdata/fuzz_seeds/<target>/`.

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
the core against `TestServer` over the simulated network, under drop, delay, duplicate and split,
moving time itself. `TestServer` is the minimal server covering the core: the transport's
`ServerConnection` for the connection's rules, one fixed answer per request, the session table, and
faults a test names; it grows with the node in task group 10. It is `src/utils_test/`, behind the
`test-utils` feature, so the shells' tests serve the same server; the core's `BUILD.bazel` sets
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
* **Mutation testing and fuzzing, for hardening** (phase 5 of `openspec/protocols/tdd-protocol.md`).
  `scripts/mutants.sh` runs over the code a stage added or edited. Every decoder of bytes from a client,
  a peer or the disk has a fuzz target, run by `scripts/fuzz.sh`; each failing input is minimized with
  `cargo fuzz tmin` and committed under the crate's `testdata/fuzz_regressions/<target>/`, where a unit
  test replays it through `ultragraphdb_tool_bdd::fuzz_regressions`, so Bazel's suite carries every
  finding.

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
