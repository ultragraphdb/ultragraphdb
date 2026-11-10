# UltraGraphDB Codebase Guide for Agents

UltraGraphDB is a next generation distributed graph database written in Rust.
UltraGraphDB aims for

- Ultra Safe
- Ultra Speed
- Ultra Scale

Ultra Safe
To be safe, a program must not only run correctly. It must apply defense-in-depth and verify itself while running, to run correctly or else shut down if it detects that it has violated expectations.

Ultra Speed
Think about performance from the outset. The time to solve performance, and get the 1000x wins, is in the design phase, when you can't profile. It's hard to fix a system after implementation, and the gains are less. Have mechanical sympathy.

Ultra Scale
A day of design is worth weeks or months in production. Therefore, go slow to go fast. Optimize the total cost of software ownership, not for those who write it once, but for those who read and run it many times. Trade linear deadlines for exponential quality.

Correctness, speed, scale, developer experience, in that order. All three matter, and the order decides the rare case where they conflict.

The key properties of the UltraGraphDB design:
- Zero copy binary protocol for network and WAL to eliminate serialization overhead; Reuse the protocol's body layout to re-encoding whenever possible 
- Thread per core architecture on compio to minimize CPU cache misses and thread contention; pinning where the platform allows it
- Coordination minimization through classification of typed queries
- Allocation minimization through consistent reference usage
- Storage Compute seperation 
- Persistent-free VSR replication, built up over the stages from a single replica behind the log's interface


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

### Attribution

Code written here from scratch carries this repository's header alone:

```rust
/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */
```

### Docker / OCI images

Base images and the image macros are Bazel targets under `bazel/`, described in `bazel/README_BAZEL.md`:
`bazel/oci_images/glibc_base` (Wolfi, built by rules_apko) for the roles and the development image,
`bazel/oci_images/scrach_base` for the config service's static musl image, and `bazel/container/img.bzl`.
`bazel/oci_images/seccomp/ultragraphdb.json` is the seccomp profile the roles run under, Docker's default with io_uring
allowed. Each image's `push` publishes it beside the image as an OCI artifact whose subject is the image's digest
(`seccomp_referrer`, artifact type `application/vnd.ultragraphdb.seccomp-profile.v1+json`), in the same `bazel run`;
`oras discover --artifact-type <that type> <repo>:<tag>` lists it and `oras pull` fetches it.

### Crates

Every crate sits in a group folder named for its layer, in a folder named as the crate: `ultragraphdb_common/`,
`ultragraphdb_error/`, `ultragraphdb_traits/`, `ultragraphdb_core/`, `ultragraphdb_store/`, `ultragraphdb_protocol/`,
`ultragraphdb_transactions/`, `ultragraphdb_transport/`, `ultragraphdb_client/`, `ultragraphdb_cluster/`,
`ultragraphdb_tool/`, and `ultragraphdb/` for the service binaries and their test crates. The root `Cargo.toml` takes them as
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
| `ultragraphdb_common_utils` | `ultragraphdb_common/ultragraphdb_common_utils` | `ultragraphdb_error_json`, `ultragraphdb_error_num`, `dashu-int` | The Go standard-library behaviour the port reproduces, once: modules `go_strconv` (parse, format, quote, unquote, `isprint`), `go_fmt`, `go_strings`, `go_unicode` (Unicode 17 tables generated from Go), `go_utf8`, `go_json` (`valid`, `unmarshal`, `encode_string`) and the types `JsonDecoder` and `JsonValue`: Go 1.27's `encoding/json`, which runs on `jsontext`, decoding into `interface{}` with `UseNumber`, nesting 10000 deep without recursion |
| `ultragraphdb_traits_collator` | `ultragraphdb_traits/ultragraphdb_traits_collator` | — | `Collator`: the language collation Dgraph's sort uses, implemented by `ultragraphdb_core_types::TextCollator` |
| `ultragraphdb_traits_index` | `ultragraphdb_traits/ultragraphdb_traits_index` | `ultragraphdb_common_utils`, `ultragraphdb_core_ids`, `ultragraphdb_error_options` | Dgraph's `tok/index` and `tok/options`: the traits `IndexFactory`, `VectorIndex`, `OptionalIndexSupport`, `OptionalSearchOptions`, `Float`, `Txn`, `LocalCache`, `CacheType`, `OptionType`, and the types their signatures name (`Options`, `AllowedOptions`, `OptionValue`, `OptionParser`, `KeyValue`, `SearchPathResult`, `VectorIndexOptions`, `SearchFilter`, `Metric`), so a vector index is implemented without depending on tok. A vector index's cache-generic methods are bounded `C: CacheType<Error = Self::Error>`, and `insert` writes through a `Txn` and takes its level draws from the caller |
| `ultragraphdb_tool_bdd` | `ultragraphdb_tool/ultragraphdb_tool_bdd` | `serde`, `serde_json` | The test harness every crate's suites use: `TestError`, `TestResult` and `Context` (`.context("what")?`, naming the caller's line), which every test and scenario step returns and fails through instead of `unwrap`, `expect` or `panic!`; `features_dir` and `testdata_dir`, a crate's `features/` and `testdata/` under both build systems; `load_golden`, `Golden<T>` and `compare_bytes` for golden vectors; `fuzz_regressions`, the minimized inputs fuzzing found, committed under `testdata/fuzz_regressions/<target>/`, for a unit test to replay; each returns a `TestResult` |
| `ultragraphdb_core_terms` | `ultragraphdb_core/ultragraphdb_core_terms` | `ultragraphdb_core_ids`, `oxrdf`, `oxsdatatypes`, `sha2`, `xxhash-rust` | RDF terms as the store keeps them: `EncodedTerm` and `EncodedLiteral`, an IRI or blank node by its system ID (tag `0x01` and 16 bytes, `EncodedTerm::id`, `as_id`), a literal of another datatype by its datatype's ID, the encoding by content with its canonical-form test, `KnownDatatype` and `Datatype` (`Known`, `Id`, and `Hashed`, a datatype by its IRI's hash as a Result names it), `EncodedTerm::view` reading an encoding as the store keeps it and `EncodedTerm::result_view` as a Result holds it, `Hash128` naming a dictionary entry, the posting fingerprint, vectors (`VectorView`, `FLOAT32_VECTOR`), `TermError` |
| `ultragraphdb_core_mapping` | `ultragraphdb_core/ultragraphdb_core_mapping` | `ultragraphdb_core_ids`, `ultragraphdb_core_terms`, `oxrdf` | The mapping contract: the canonical statement model (`Statement` and `PgEdge` named by their reifier IRI, `StatementRef::Reifier`, `StatementId` over a `Statement`-type system ID, `CanonicalDataset`), `Names` with skolem IRIs `…/.well-known/genid/{bucket}-{random}` (`skolem_iri`, `mint_skolem`, `skolem_bucket`) and `RandomSource` (SplitMix64, the injected random source), its RDF and property-graph views, the declarations (`PredicateDeclaration`, `TypeDeclaration`, `RelationDeclaration`, `VectorIndexSpec`), and `Changeset` with its `Change`s, the write format the engines produce and storage applies; `MappingError` |
| `ultragraphdb_core_keys` | `ultragraphdb_core/ultragraphdb_core_keys` | `ultragraphdb_core_ids`, `ultragraphdb_core_terms` | The quad layout's keys over 128-bit system IDs: `QuadKey`, one variant per `Keyspace`, every ID a `SystemId` as its 16 big-endian bytes and the default graph (`Graph::Default`, `DEFAULT_GRAPH`) as 16 zero bytes, a reverse key ending with its object, the marks keyspace keyed by an entity and its 18-bit bucket group as 3 bytes, the referrers keyspace by an entity, the dictionary keyspace (0x1B) by a `DictionaryEntry` (kind 1 a term's hash, 2 an ID, 3 a literal's hash); an ID object validated as a system ID; built and parsed back into its fields, refusing a truncated key, an invalid system ID or a group past 18 bits with `QuadKeyError`; split keys under `SPLIT_FLAG` with a `SplitStart` after its kind byte, kind 1 and a fingerprint's 8 bytes or kind 2 and an ID's 16 (an ID alone for a value-index key), a kind byte the layout lacks refused with `UnknownKind`; `key_prefix`, `Region`, `CountDirection`, `ROOT_TENANT`. Fuzz target `quad_key_parse` |
| `ultragraphdb_core_codec` | `ultragraphdb_core/ultragraphdb_core_codec` | `ultragraphdb_core_ids`, `ultragraphdb_error_store`, `xxhash-rust` | Packed lists of 128-bit system IDs after Dgraph's `codec`, in the repository's own format: `Encoder` (a list not strictly increasing refused naming its position), `Decoder` and `SeekPos` over `UidPack` and `UidBlock` (blocks of at most the block size, each its first ID whole and LEB128 deltas of the IDs' first 104 bits; the bytes begin with version 1, `UidPack::encode` and `decode`); the posting lists' types of the repository's own in place of Dgraph's protobuf messages (`PostingList`, `Posting` keyed by 128 bits, a reference's ID or a value's fingerprint, `PostingType`, `List`, `Facet`, `BackupPostingList`) and their stored format, version 2: flags, the commit timestamp, the pack, postings and splits under an XXH3-64 trailer (`PostingList::encode` and `decode`), a truncation or a flipped bit refused as the store's `Corrupted` naming the key; `CodecError`. Golden values from `testdata/golden/formats.py`, written from the formats' specifications. Fuzz targets `posting_list_decode` and `uid_pack_decode` |
| `ultragraphdb_core_algo` | `ultragraphdb_core/ultragraphdb_core_algo` | `ultragraphdb_core_codec`, `ultragraphdb_core_ids` | Dgraph's `algo` over system IDs: sorted-list intersection, merge and difference, packed and unpacked, `IdHeap` and its `Elem` |
| `ultragraphdb_core_text` | `ultragraphdb_core/ultragraphdb_core_text` | `ultragraphdb_common_utils` | Go's `x/text/collate` with its CLDR 23 tables and `x/text/language`'s base inference: `Collator`, `Tag`, `Base`, `Confidence`, `language_base`, `LanguageError` |
| `ultragraphdb_core_types` | `ultragraphdb_core/ultragraphdb_core_types` | `ultragraphdb_common_utils`, `ultragraphdb_core_text`, `ultragraphdb_error_json`, `ultragraphdb_error_num`, `ultragraphdb_traits_collator`, `bcrypt`, `dashu-int` | Dgraph's `types`: `TypeId`, `Val`, conversion and marshalling, sorting and comparison, `BigFloat`, `DateTime`, passwords, geometries with their WKB and GeoJSON encodings, geo tokens and filters over a port of golang/geo's S2; `TextCollator`, `TypesError` |
| `ultragraphdb_core_tok` | `ultragraphdb_core/ultragraphdb_core_tok` | `ultragraphdb_common_utils`, `ultragraphdb_core_text`, `ultragraphdb_core_types`, `ultragraphdb_error_options`, `ultragraphdb_traits_index`, `rust-stemmers`, `sha2`, `tracing`, `unicode-normalization` | Dgraph's `tok`: `Tokenizer` and the registry, the bleve analysis it runs (segmentation, stop words, stemmers), the hnsw vector-index factory, `TokError` |
| `ultragraphdb_core_catalog` | `ultragraphdb_core/ultragraphdb_core_catalog` | `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_tok`, `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `oxrdf` | The catalog: `Catalog`, per tenant the declarations of its predicates, entity types and relations by their system IDs, the version it holds, and the checks of a write's object against them; `CatalogContents`, a catalog version's declarations by IRI and its encoding, the opaque bytes the metadata group stores, refused whole as `CorruptedVersion` naming the version when it does not decode; `datatype_of`, `tokenizer_takes`, `CatalogError`. A partition holds no catalog key: `Catalog::entry` keys the catalog changes a write tracks, written to no store, and `Catalog::load` reads a store's catalog keys, which no partition writes |
| `ultragraphdb_error_store` | `ultragraphdb_error/ultragraphdb_error_store` | — | `StoreError(pub StoreErrorEnum)`: why a store operation failed (`TsTooOld`, `KeyTooLarge`, `EmptyKey`, `ReadFailed`, `Corrupted`, `WriteFailed`, `Poisoned`, `OpenFailed`, `Unsupported`, `UnknownTemporary`), with Dgraph's or Badger's text where they have one, and `go_hex_dump` for the keys they name; raised by every store and carried by every crate above |
| `ultragraphdb_error_protocol` | `ultragraphdb_error/ultragraphdb_error_protocol` | — | `ProtocolError(pub ProtocolErrorEnum)`: why a frame of the binary protocol was refused, one variant per validation rule; depends on nothing |
| `ultragraphdb_core_body` | `ultragraphdb_core/ultragraphdb_core_body` | — | The body layout and the string list of `specification/binary_protocol/PROTOCOL.md` §4.1 and §4.3, read in place and written, with no I/O: `Sections` (a fixed part, a section table of offsets and lengths, each section at the next multiple of 8, the padding zero, nothing after the last), `write_sections` and `write_sections_into`, `StringList` and `StringWriter`, a section's records, u32s and text, and `BodyError`, one variant per rule, naming no command or entry. The wire reads every body through it and the catalog its versions (`specification/schema/CATALOG.md`), so one reading discipline serves both and neither depends on the other. Fuzz targets `sections` and `string_list` |
| `ultragraphdb_error_log` | `ultragraphdb_error/ultragraphdb_error_log` | — | `LogError(pub LogErrorEnum)`: why an operation of the ordered log failed, split by what the caller does about it; depends on nothing |
| `ultragraphdb_core_ids` | `ultragraphdb_core/ultragraphdb_core_ids` | `xxhash-rust` | The 128-bit system ID with no I/O: `SystemId` (a 16-bit `TypePrefix`, a 64-bit `Sequence`, a 24-bit `Bucket`, 24 reserved bits that are zero; stored and ordered as its 16 big-endian bytes, so by type, sequence and bucket, decoded with every invalid field refused), `TypePrefix::RESOURCE` and `STATEMENT`, `Tenant`, `CanonicalKey` (an IRI's 4-byte big-endian length and UTF-8 bytes), `Bucket::derive` (8 bits of XXH3-64 of the tenant, 16 of the declaring type's prefix and the canonical key, pinned by golden buckets from the xxHash reference), `Tenant::bucket_bits` (the top byte of XXH3-64 of its 8 big-endian bytes, every bucket's top 8 bits), `Version` (a 42-bit `Epoch`, `Epoch::ZERO` before the first close, and a 22-bit sequence, even for a commit) and `SessionToken` (partition, log position, epoch); `BucketGroup` (64 buckets, a bucket's top 18 bits, one type for the core and placement); `IdError`, one variant per refusal (`Length`, `ReservedBits`, `ZeroTypePrefix`, `ZeroSequence`, `BucketRange`, `BucketGroupRange`, `IriTooLong`, `EpochRange`, `CommitSequenceRange`, `OddCommitSequence`). Fuzz targets `system_id_decode` and `version_decode` |
| `ultragraphdb_error_cluster` | `ultragraphdb_error/ultragraphdb_error_cluster` | `ultragraphdb_error_invariant`, `ultragraphdb_error_log` | `ClusterError(pub ClusterErrorEnum)`: why an operation of the cluster's state machines was refused, raised by the placement map, the sequencer and the metadata group and carried by the frontend's owner: the placement map's refusals (`ZeroPartition`, `PartitionCount`, `BucketRange`, `Unassigned` and `AssignedTwice` naming the first bucket, `ChangeEpoch`, `UnknownPartition`), the settings' (`SettingBelow`, `SettingAbove`, naming the setting and the bound), the sequencer's (`EpochLimit`, `EpochAhead`, `BeforeFirstClose`, `NoPartitions`, `DuplicatePartition`, `NotSubscribed`, `CatchUpTrimmed`), the metadata group's (`StaleCatalog` naming the predecessor and the latest version, `NotPending`, `CatalogReleased`, `BlockSize` naming the size and `block_size_max`, `SequencesSpent`, `NoPlacement`, `Contradicts`, `PrincipalName`, `PrincipalExists`, `UnknownPrincipal`, `LastAdministrator`, `Bootstrap`, `SessionExists`, `SessionEvicted`), the bounds' (`TooMany`, `Full`), damage (`Malformed`, `ReplayDiverged`, `SnapshotBehind`) and the carried `Log` and `Invariant`, into which `?` converts `LogError` and `InvariantError` |
| `ultragraphdb_cluster_placement` | `ultragraphdb_cluster/ultragraphdb_cluster_placement` | `ultragraphdb_core_ids`, `ultragraphdb_error_cluster` | The placement map with no I/O: `PartitionId` (from 1), `NodeId`, `BucketRange` (half-open, at most 2^24), `TenantRun` (a tenant's 65,536 buckets, `Tenant::bucket_bits`; `of_group`), `BucketRange::of_group` over `ultragraphdb_core_ids::BucketGroup`, and `PlacementMap`: every bucket assigned to one partition as normalised ranges, each layout with the epoch it applies from (the first from epoch 0), `even` splitting every run over 1 to 1,024 partitions, `change` refusing a layout that leaves a bucket unassigned or assigns it twice, naming the first, `partition_of` and `group_partitions` at an epoch, `holders_of_groups` and `holders_of_run`, the partitions a reverse read and a fan-out ask, the rule a partition's own path and compute both read, `ranges_at`, preferred leaders, and its encoding (`encode`, `decode`, refusing any non-canonical byte), checked against a plain bucket-by-bucket reference model. Fuzz target `placement_decode` |
| `ultragraphdb_cluster_metadata` | `ultragraphdb_cluster/ultragraphdb_cluster_metadata` | `ultragraphdb_cluster_placement`, `ultragraphdb_core_ids`, `ultragraphdb_error_cluster`, `ultragraphdb_error_log`, `ultragraphdb_protocol_scram`, `ultragraphdb_traits_blocks`, `ultragraphdb_traits_catalog`, `ultragraphdb_traits_log`, `xxhash-rust` | The metadata group's state machine over an `OrderedLog`, with no I/O: `MetadataGroup` appends each command as a `MetadataCommand` (one round, recording its decision: a lease its block, a principal its identity, a registration its evictions, a removal its principal's sessions) decided against every command appended, applies it to the `MetadataState` reads answer from only once synced, and reads name the position (`applied`); `recover` from a `MetadataSnapshot` and the rounds after it, `snapshot`, `snapshot_due` every `snapshot_rounds`, `trim`. The state: the tenants, created by a command, tenant 0 at the first start, and the secret an unknown name's verifier derives from, drawn once and held durably; per tenant numbered catalog versions (`CatalogRecord`, opaque bytes in `core_catalog`'s encoding) committed as pending, a predecessor other than the latest refused, given the epoch after the learned close (`learn_close`: the close of `c + 1` assigns `c + 2`), answered as `catalog_at` up to one epoch past the last close, released once every partition's untrimmed log begins after them (`report_untrimmed`); a version tightening a predicate held as being checked (`CatalogCheck`) until every partition holding its tenant reports it passed (`report_check`) or a restoring version withdraws it (`offer_restoring`), `settled_at` what a client reads meanwhile; default-entry records by (tenant, predicate), found when held, no version; per-type high-water marks leasing blocks of 1 to `block_size_max` (`MetadataConfig`); the `PlacementMap`, initialised evenly as version 0, each change committed as a pending placement version (at most `PLACEMENT_PENDING_MAX`, 16, `pending_placement` the oldest) and taken into the map from the epoch after the close announcing it, one version per close, the oldest pending; principals with SCRAM verifiers, the first from the bootstrap secret file, the last administrator kept, a removed one's sessions evicted with it; sessions bounded by `sessions_max`, the oldest evicted and carried by the next close (`pending_evictions`). `MetadataState` implements `ultragraphdb_traits_catalog::CatalogSource` and `ultragraphdb_protocol_scram::Credentials`; `LeasedBlocks`, the blocks a partition's host leased durably, implements `ultragraphdb_traits_blocks::BlockSource`. Recovery is tested in the simulator with a kill after every append and a fault at every storage operation. Fuzz targets `command_decode`, `snapshot_decode` |
| `ultragraphdb_cluster_partition` | `ultragraphdb_cluster/ultragraphdb_cluster_partition` | `ultragraphdb_cluster_placement`, `ultragraphdb_cluster_sequencer`, `ultragraphdb_core_catalog`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_apply`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_traits_blocks`, `ultragraphdb_traits_log`, `ultragraphdb_traits_store`, `ultragraphdb_transactions_commit`, `ultragraphdb_transactions_txn`, `oxrdf` | A partition over its bucket ranges: routing naming the owner (`Partition`), registration as a referrer (`Reference`, `Answer`) and as an entity's home (`Home`), reverse reads and summed reverse counts through referrer sets at a closed epoch (`Fanout`, `ReverseQuery`), one partition's loop on whichever runtime hosts it, a turn taking at most `MAILBOX_PER_TURN_MAX` (2,048) of its mailbox's messages (`PartitionHost`, `PartitionMailbox`, `PartitionMessage`, `PartitionOutcome`), what it sends beyond its process as role commands (`RemoteMessage`), a transaction a Bind opened held between its requests (`BoundKey`, `BoundStep`), with what its read kernel answers free of frames (`BoundRead`), `PartitionError` |
| `ultragraphdb_cluster_sequencer` | `ultragraphdb_cluster/ultragraphdb_cluster_sequencer` | `ultragraphdb_cluster_placement`, `ultragraphdb_core_ids`, `ultragraphdb_error_cluster`, `ultragraphdb_error_invariant`, `ultragraphdb_error_log`, `ultragraphdb_traits_log` | The sequencer's state machine over an `OrderedLog`, its time a `ClockReading` (monotonic and wall-clock microseconds) the caller passes: `Sequencer` closes epochs from 1, each `epoch_ms_min` to `epoch_ms_max` after the last (`SequencerConfig`, the bounds 1–1,000 and `epoch_ms_min`–10,000 refused naming the setting), early on a close request from a partition or a waiting read, past 2^42 − 1 refused; each `Close` records its wall-clock time (never falling), the `CatalogVersion`s it announces and the placement version (0 for none), effective the next epoch, and the session evictions it carries; multi-partition transactions are placed in the open epoch as `SequencedTransaction`s; every close and transaction is a `SequencerRecord`, one round each, delivered to subscribed partitions (with catch-up), mapped by `as_of` and read back only once durable; `recover` replays the log; `snapshot` (`SequencerSnapshot`: the durable closes retained and the transactions held, its encoding refusing any non-canonical byte), `trim` through it and `recover_from` a snapshot and the rounds after it (task 6.4). Recovery is tested in the simulator with a kill after every append and a fault at every storage operation. Fuzz targets `record_decode`, `snapshot_decode` |
| `ultragraphdb_traits_store` | `ultragraphdb_traits/ultragraphdb_traits_store` | `ultragraphdb_error_store` | The synchronous, batch-shaped `Store` trait and `StoreSnapshot`, with a keyspace without versions a batch writes atomically with its entries, the types their signatures name (`WriteBatch`, `PersistMode`, `Scan`, `Entry`, `Item`, `Direction`, `Temporary`, `RebuildRecord`), a rebuild's temporary keyspaces and its synced rebuild records, the `Executor` background work runs on and the `ListCache` trait, so the fjall store and the simulator are interchangeable |
| `ultragraphdb_traits_blocks` | `ultragraphdb_traits/ultragraphdb_traits_blocks` | `ultragraphdb_core_ids` | `BlockSource`, where a partition leases the sequences its system IDs take: `lease` hands out the next `SequenceBlock` of a type (a half-open range of at least one sequence, never leased twice to any partition) or `None` when none can be leased now, at once and without I/O; implemented by the metadata group (task group 6). Behind `test-utils`, on by default and in its Bazel target, `TestBlockSource`: counted blocks of a fixed length from sequence 1 per type, or scripted blocks, unreachable on demand, its clones sharing one state as partitions share one metadata group |
| `ultragraphdb_traits_catalog` | `ultragraphdb_traits/ultragraphdb_traits_catalog` | — | `CatalogSource`, where a partition reads its tenants' catalog versions: `version_at` the `HeldVersion` in effect at an epoch (`None` for the initial catalog) and `version` one by number, for a replayed build's start record; a version's contents are opaque bytes (`ultragraphdb_core_catalog::CatalogContents`); `DefaultEntry`, the default-entry record a first write of an undeclared predicate asks the metadata group for, sent by the partition's host; implemented by the metadata group (task group 6). Behind `test-utils`, on by default and in its Bazel target, `TestCatalogSource`: committed versions and default-entry records in memory, unreachable on demand, its clones sharing one state |
| `ultragraphdb_traits_log` | `ultragraphdb_traits/ultragraphdb_traits_log` | `ultragraphdb_error_log` | `OrderedLog`, the ordered log the commit step, the sequencer and the metadata group write their rounds to: `append` stages a round at the position after the tail, naming the tail's checksum as its parent, and returns its `DurabilityTicket`, which `poll_ticket` answers complete once the round is durable; `sync` makes the staged rounds durable, then `committed`, `tail`, `read`, `trim_through` (the store synced first), `checkpoint` (its position), `seal` and `next_event`; with `Position`, `Round`, `RoundRef` (each carrying its derivation version) and `LogEvent`. The deciding methods are synchronous and do no I/O; those waiting on storage return futures. Implemented by `ultragraphdb_transactions_log`'s `Wal` |
| `ultragraphdb_common_telemetry` | `ultragraphdb_common/ultragraphdb_common_telemetry` | `ultragraphdb_common_version`, `ultragraphdb_transport_tls`, `tracing`, `tracing-subscriber`, `tracing-opentelemetry`, `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-http`, `async-trait` | What a binary calls once at start: `init(TelemetryConfig)` installs the level filter, plain or JSON output to stderr and an optional OTLP/HTTP span exporter, an `https://` endpoint verified against the image's CA bundle or the file `SSL_CERT_FILE` names, and returns the `TelemetryGuard` that flushes on drop; `OtlpHttpClient`, the exporter's HTTP/1.1 client over the connections a `Dial` opens (`TcpDial` in production, an in-memory collector in tests), plain or verified TLS through a `TlsConnector`, dialing and every read and write each bounded by its timeout, with no clock; it blocks on the batch processor's own thread, so no async runtime is needed; `decode_response`, its decoder of a collector's answer, public for the fuzz target `http_response_decode`; `subscriber`, `LogFormat`, `TelemetryError`. No library crate depends on it |
| `ultragraphdb_transport_tls` | `ultragraphdb_transport/ultragraphdb_transport_tls` | `rustls`, `rustls-pki-types`; `rcgen` behind `test-utils` | Client-side TLS over a blocking stream, a socket or any other, with ring as rustls's one crypto provider, named rather than taken from the process default: `TlsTrust` (the image's bundle `/etc/ssl/certs/ca-certificates.crt` or the file `SSL_CERT_FILE` names, `system_from` taking that value passed in, any PEM file or bytes; each anchor once, unusable certificates skipped, an empty source refused), `TlsConnector` (TLS 1.3 and 1.2, the handshake completed before anything is sent), `TlsStream`, `TlsError`; behind `test-utils`, on in its Bazel target, `TestPki`, a CA and a server certificate rcgen generates per call, served by `TestPeer`, an in-memory rustls server (TLS 1.3 and 1.2, or 1.2 alone) driven by the client's own reads and writes in front of an application such as `TestEcho`, so a handshake runs on the test's thread with no socket. Built through the five phases of `tdd-protocol.md` (`openspec/changes/transport-tls/notes/phases.md`); its unit tests run with `SSL_CERT_FILE` set to a path no file is at, as `scripts/mutants.sh` must be run |
| `ultragraphdb_store_sim` | `ultragraphdb_store/ultragraphdb_store_sim` | `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `tracing`, `tracing-subscriber` | The deterministic storage simulator: `SimStore` with seeded fault injection (`Fault`, `FaultConfig`, `Corruption`, `DropStage`, `RebuildStage`), the shadow model `StoreModel` and its `Invariant`s, `Simulation`, `Seed` and its `Stream`s, `SimRng`, `SimClock`, `SteppedExecutor`, `SimListCache`, `LogCapture`; the `store_sim_fuzz` binary. Crates above test their own workloads on it; its README states what it injects and detects |
| `ultragraphdb_store_kv` | `ultragraphdb_store/ultragraphdb_store_kv` | `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `fjall`, `moka`, `tracing` | The production `Store` over fjall, its journal persisted manually: `FjallStore` (versioned keys and a keyspace without versions, physical drops through synced drop records, journaled removals, the fenced flag, temporary keyspaces dropped at open, rebuild records), `FjallSnapshot`, `FjallScan`, `Durability` (`Log`: the write-ahead log the only stream synced per round, the store synced at checkpoints; Dgraph's `Process` and `Filesystem`), the `GcFilter` compaction filter, `ThreadExecutor` and `MokaListCache`; the `kv_crash_writer` binary the kill tests drive |
| `ultragraphdb_store_posting` | `ultragraphdb_store/ultragraphdb_store_posting` | `ultragraphdb_common_utils`, `ultragraphdb_core_algo`, `ultragraphdb_core_codec`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `tracing`, `xxhash-rust` | Dgraph's `posting` over the quad layout's list keys: `List` with its mutable layer, postings identified by uid or, where the keyspace compares values, by uid and value, rollups and splits; `Lists` (the commit with its hook under the writer lock, timestamps, drops), `Request` (a transaction's `LocalCache`), `Batch`, `CachedList`, a list's store entries marshalled in `ultragraphdb_core_codec`'s format (`marshal`, `unmarshal_complete`, `unmarshal_delta`), `PostingError` |
| `ultragraphdb_store_hnsw` | `ultragraphdb_store/ultragraphdb_store_hnsw` | `ultragraphdb_core_codec`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_terms`, `ultragraphdb_core_tok`, `ultragraphdb_error_options`, `ultragraphdb_store_posting`, `ultragraphdb_traits_index`, `ultragraphdb_traits_store`, `tracing` | Dgraph's `tok/hnsw` over the vector-index keyspace: `PersistentHnsw` implementing `VectorIndex`, `HnswFactory`, `VectorKeys` with the entry, neighbour and dead lists, the transaction adapter `ViTxn` reading vectors from forward lists, with `TxnCache` and `QueryCache`, the distance kernels, the neighbour-matrix codec, `HnswError` |
| `ultragraphdb_store_quads` | `ultragraphdb_store/ultragraphdb_store_quads` | `ultragraphdb_core_catalog`, `ultragraphdb_core_codec`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_error_store`, `ultragraphdb_store_posting`, `ultragraphdb_traits_blocks`, `ultragraphdb_traits_store`, `oxrdf`, `xxhash-rust` | Quads on posting lists over 128-bit system IDs: `QuadStore` with the term dictionary in the dictionary keyspace (0x1B: a term's ID by its hash, an ID's term, a literal's string by its hash) and the IDs created during executions and not yet committed (a bounded pending map, `PENDING_MAX`), settled once a commit writes them or a refused write is released; `WriteTerms`, an IRI's ID got or created while the write runs and shared by concurrent writers (bucket from `Bucket::derive`, sequence from `Sequences` over a `BlockSource`), blank nodes and statement reifiers named by skolem IRIs drawn from the injected `RandomSource` among the `OwnedBuckets`, redrawn on a collision (`SKOLEM_DRAWS_MAX`), absent terms recorded by ID; `QuadWrite` with `STATEMENTS_PER_OBJECT_MAX`, `StatementRecord` (format 2), `Statistics` kept by deltas, the `DataDictionary` read API with its `Shapes`; `QuadError` |
| `ultragraphdb_store_apply` | `ultragraphdb_store/ultragraphdb_store_apply` | `ultragraphdb_core_catalog`, `ultragraphdb_core_codec`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_core_tok`, `ultragraphdb_core_types`, `ultragraphdb_error_invariant`, `ultragraphdb_error_store`, `ultragraphdb_store_hnsw`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_traits_blocks`, `ultragraphdb_traits_catalog`, `ultragraphdb_traits_index`, `ultragraphdb_traits_store`, `oxrdf`, `oxsdatatypes`, `tracing` | Changesets applied to the quads: `ChangesetApplier` (writes checked against the catalog with their value, count and vector indexes, statement records and statistics; declarations checked against the data; index builds in the background with a crash-safe rebuild record; drops of a predicate, a graph, a tenant's data or a tenant; a transaction's changesets executed against its snapshot as an `Execution`, sealed as `Executed`, and a round's `Executed`s applied at their commit versions in one store batch, `apply_round_with`, their new terms' pending IDs settled, and an execution the commit step refused released with `release`); statements named by their reifiers; `OperationOutcome`, `Applied`, `LevelDraw`, `token_value`, `ChangesetError` |
| `ultragraphdb_protocol_wire` | `ultragraphdb_protocol/ultragraphdb_protocol_wire` | `ultragraphdb_core_body`, `ultragraphdb_core_ids`, `ultragraphdb_core_terms`, `ultragraphdb_error_protocol`, `xxhash-rust` | The binary protocol's bytes as `specification/binary_protocol/PROTOCOL.md` specifies them, with no I/O: the 256-byte `Header` and `HeaderView`, XXH3-64 checksums and chunk tables, `FrameView`'s validation in the protocol's order, the `Command`s, and every body's view and builder (`BodyView`, `BodyBuilder`): the handshake and authentication, transactions, reads and writes, queries and installs, prepared statements and describe, the role commands with `Held` and `EntriesSequence`, the rules comparing a Scan's Entries frames with its ranges, columnar `ResultBatch`es with their term tables, `ResumeKey` and `ResumeKeyView`, a limited read's token, with a fuzz target `resume_key`, IRIs and blank nodes by the hash of their text, errors, the Commit result (`OperationResult`), the `Round` frame and the logical `WriteSetRecord` the write-ahead log holds (`PROTOCOL.md` §12: resolved quads with their system IDs, drops and builds as `RecordOperation`s, `RecordRegistration`s, the session-table entry, the round's dictionary entries, one epoch, catalog and derivation per round); every value read in place with `from_le_bytes`, system IDs big-endian; `StatementError` |
| `ultragraphdb_protocol_transport` | `ultragraphdb_protocol/ultragraphdb_protocol_transport` | `ultragraphdb_protocol_wire`, `ultragraphdb_error_protocol` | The transport that carries the wire protocol, sans-I/O: synchronous state machines with no runtime, which a runtime's shell drives. `Framer` assembles frames pull-style into a `BufferPool`'s `PooledBuffer`s, verifying the header before its size is trusted, a reader waiting for a buffer while the pool is full (`acquire_waiting`); `ClientHandshake` and `ServerHandshake`; `Outbound`, a server's frames scheduled across streams under credit and cancellation; `ServerConnection`, a server's whole connection with authentication through an `Authenticator`; `ClientAuthentication` through a `Login`'s `ClientMechanism`; `SimNetwork` and `SimStream`, a seeded network that drops, delays, duplicates and splits; `TransportError` |
| `ultragraphdb_protocol_scram` | `ultragraphdb_protocol/ultragraphdb_protocol_scram` | `ultragraphdb_protocol_transport`, `sha2`, `hmac`, `subtle`, `base64`, `stringprep`, `getrandom` | SCRAM-SHA-256 (RFC 5802, RFC 7677) for the Authenticate exchange, sans-I/O, passwords prepared with SASLprep (RFC 4013), without channel binding: `ScramClient` and `ScramLogin`, `ScramAuthenticator` and `ScramServer`, `ScramVerifier` (a password's salt, iteration count and two derived keys, never the password), `system_nonce`, `ScramError` |
| `ultragraphdb_protocol_transport_compio` | `ultragraphdb_protocol/ultragraphdb_protocol_transport_compio` | `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire`, `ultragraphdb_transport_tls`, `compio`, `socket2`, `tracing` | The transport's compio shell, which the compio client uses: `Connection` does a connection's I/O over a compio stream as the `Framer` and the handshakes direct, a read waiting for a free buffer; `Carrier` and `Listener` set TCP keepalive on every connection; `AcceptPace`, by which every accept loop, a role's, `/health`'s and `serve_listener`'s, pauses after a failed accept, from 5 ms doubling to 1 s, warning once a run of failures; `CompioTlsStream` splits into `TlsReadHalf` and `TlsWriteHalf` for a concurrent reader and writer; `CompioSimStream`, a simulated stream as a compio stream, split as a socket is |
| `ultragraphdb_transactions_txn` | `ultragraphdb_transactions/ultragraphdb_transactions_txn` | `ultragraphdb_cluster_placement`, `ultragraphdb_core_catalog`, `ultragraphdb_core_codec`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_error_invariant`, `ultragraphdb_error_protocol`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_apply`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_traits_blocks`, `ultragraphdb_traits_store`, `oxilangtag`, `oxrdf` | A transaction on a worker: `Transaction` begins at the visibility watermark or at an earlier version it names, a version above the watermark refused at once; a read-write one is bound to one partition (`Transaction::bind`), a write of a subject whose bucket another partition owns aborting it as cross-partition; its changesets execute against its snapshot through one held execution of the changeset applier, so later requests read them, and its reads are recorded in a `ReadSet`; a Write frame's changes are read in place as `FrameChanges`, each term borrowed from the frame until the write resolves it to its ID (`FrameTerms` holding the triple terms and language tags the frame's text cannot lend); its `Commit` ends a read-only transaction on the worker and hands a read-write one to the commit step as a `Prepared`: its execution, its sealed read set, the session token and epoch floor that hold it until its epoch may be stamped, and the logical write-set record `WriteSetBuilder` writes in the wire's layout (resolved quads with their system IDs, statement records, the operation, the registrations it relied on as a referrer, the client's commit record, conflict keys; never a key the store derives). `TxnLimits` (10 s read-write, 16 s read-only, the largest write-set record `MAX_WRITE_SET`, the read set's bound `MAX_FRAME`, objects named one by one per list), `TxnBuffers` lent by the worker so the steady state allocates no read set or record buffer, `Mode`, `Operated`, `TxnError`; the criterion bench `read_set_bench` |
| `ultragraphdb_transactions_log` | `ultragraphdb_transactions/ultragraphdb_transactions_log` | `ultragraphdb_error_log`, `ultragraphdb_error_protocol`, `ultragraphdb_protocol_wire`, `ultragraphdb_traits_log`, `compio`, `tracing`; `rustix` on Linux | The write-ahead log: `Wal`, implementing `OrderedLog`, with rounds as checksummed Round frames chained by their parents' checksums, written at 4,096-byte boundaries into segments of a fixed size; the two-copy `Superblock` recording the checkpoint's position, and the three outcomes of reading it (`SuperblockRead`); a `DurabilityTicket` per append; recovery's scan (`scan_segment`, `find_complete_round`), which truncates a torn tail and reports it (`Truncation`) and refuses interior damage (`Damage`); trimmed segments zeroed and recycled; `WalConfig` with the power-loss-protection flag, which sets round bounds and the checkpoint interval and never a sync. The storage seam `DurableStorage` and `DurableFile`, implemented by `CompioStorage` (`O_DIRECT`, `fdatasync` and `fallocate` on Linux; on macOS a buffered file whose sync is `fsync`, which leaves the drive's cache unflushed) and by the deterministic `SimStorage` with its `FaultMode`s; `Recovery` |
| `ultragraphdb_transactions_commit` | `ultragraphdb_transactions/ultragraphdb_transactions_commit` | `ultragraphdb_cluster_placement`, `ultragraphdb_cluster_sequencer`, `ultragraphdb_core_catalog`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_terms`, `ultragraphdb_error_log`, `ultragraphdb_error_protocol`, `ultragraphdb_error_store`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_apply`, `ultragraphdb_store_posting`, `ultragraphdb_traits_blocks`, `ultragraphdb_traits_catalog`, `ultragraphdb_traits_log`, `ultragraphdb_traits_store`, `ultragraphdb_transactions_txn`, `oxrdf` | The commit step of a partition: `CommitStep`, a step function over `Input`s (commit requests, registrations a home accepted, the sequencer's closes, sync completions, the clock, the oldest open read) filling a `StepOutput` (the round it closed, the synced round to apply, a close to ask of the sequencer when the open epoch is full, the replies with their session tokens), so the simulator drives it without a runtime; an `EpochStamp` giving commit versions, the open epoch exactly one above the latest close delivered and the next even sequence within it, never advancing its epoch on its own, and `CloseRequest`, `close_request_ms`, how long a full epoch waits for the close it asked for before the partition refuses its commits as busy; a `Resolver` deciding each `Proposal` in round order against the `ConflictHistory`, exactly; commit-time operations through an `Operator` (`ApplierOperator`, `NoOperations`); `Sessions`, the partition's session table of `SessionEntry`s, each session's last commit there with its `StoredReply`, for exactly-once commits; `Pipeline`, the step on one node, each round appended to an `OrderedLog` and synced, then applied to the changeset applier, which moves the visibility watermark; `Replay`, recovery from the log, deriving each round's writes from its logical record; `RoundBounds`, `RoundRecord`, `Reply`, `CommitError`, `PipelineError`, `RecoveryError`, `SettingError`; the criterion bench `history_bench` |
| `ultragraphdb_client_core` | `ultragraphdb_client/ultragraphdb_client_core` | `ultragraphdb_core_ids`, `ultragraphdb_error_protocol`, `ultragraphdb_protocol_scram`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire` | The Rust client as a sans-I/O core: `ClientCore` holds every rule of the protocol a client follows (the handshake, authentication, `Session`s, `Transaction`s at their read levels, one-frame writes, prepared `Statement`s, describe, `ResultStream`s with credit and cancel, `Reply`s read in place through the wire's views); it checks each frame in `PROTOCOL.md` §2.5's order, takes the frames its shell read and the time with every call that may start a wait, and queues the frames to write. A session has one request counter and keeps the token of its highest epoch; a commit, a drop or a one-frame write whose frame left and whose answer did not arrive is in doubt until a resend through its session, with the same number, says what became of it. `ClientConfig`, `ClientError`, whose `runs_again` says what a refusal runs again; behind the `test-utils` feature, `TestServer`, the scripted server the client's tests run against |
| `ultragraphdb_client_tokio` | `ultragraphdb_client/ultragraphdb_client_tokio` | `ultragraphdb_client_core`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire`, `ultragraphdb_transport_tls`, `tokio` | The primary public Rust client: `Client`, the core's tokio shell, its credit automatic, its futures `Send` over `TcpConnector`, `TlsDial` and `TokioTlsStream` for TLS through the transport's session, and `TokioSimStream` for the simulated network; the core's types re-exported |
| `ultragraphdb_client_compio` | `ultragraphdb_client/ultragraphdb_client_compio` | `ultragraphdb_client_core`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_protocol_wire`, `ultragraphdb_transport_tls`, `compio` | The public compio client: `Client`, the core's compio shell over the transport's compio `Connection`, split into its halves by `SplitStream` and each frame read on a task of its own, so no deadline cuts a read, its credit automatic, thread per core, io_uring on Linux, `TlsDial` for TLS through `CompioTlsStream`; the core's types re-exported |
| `ultragraphdb_tool_bench` | `ultragraphdb_tool/ultragraphdb_tool_bench` | `ultragraphdb_client_compio`, `ultragraphdb_cluster_partition`, `ultragraphdb_cluster_placement`, `ultragraphdb_cluster_sequencer`, `ultragraphdb_common_telemetry`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_error_store`, `ultragraphdb_protocol_scram`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_apply`, `ultragraphdb_store_kv`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_traits_blocks`, `ultragraphdb_traits_log`, `ultragraphdb_traits_store`, `ultragraphdb_transactions_commit`, `ultragraphdb_transactions_log`, `ultragraphdb_transactions_txn`, `compio`, `futures`, `mimalloc`, `oxrdf`, `stats_alloc` | The `store_bench` binary: the apply thread at a fixed offered rate with the Dgraph runs' commits as changesets of quads, reporting lists read per relation, cache hits and store reads that found nothing; rollups and the compaction filter over one large predicate; the store's file sizes. The `wal_bench` binary: the same commits as transactions through the commit pipeline, each round synced in the write-ahead log on compio and applied to fjall, reporting the rounds and the bytes the log's segments and fjall's journal took; given `--partitions`, N partitions in one process, each on a pinned compio runtime with its own store, log and generator seeded from the run's seed, a share of its edges pointing at other partitions' entities so registration runs, reporting each partition's and the total relations and syncs a second, the disk bytes, the registrations a second and the wait a first reference costs, `--verify on` comparing each reverse read at the last closed epoch with the same commits on one partition; given `--inbound`, the probe of an inbound point read, objects of chosen in-degrees read over the partitions their referrer sets name and on one partition, with the posting lists, cache hits, store reads and point reads per read and its time. The `client_bench` binary: requests through the three binaries with the real client, the compio shell's `Client` over TCP logged in as the bootstrap administrator, each connection sending DirectWrites, read-write transactions or read-only transactions one after another, its commits through a session or none, each commit's nodes and hubs in one partition of the even placement, reporting the requests and relations a second and each request's latency, the Begin's apart, and the refusals by code and detail; its reads at level snapshot or monotonic-fast, of one subject, of every node's string (`scan`) or of one row in each partition (`span`); `stall`, one transaction per connection pipelining its Writes and holding them; `list-read` and `hot`, read-write readers of a hot subject's every statement and writers of its values. `wal_bench --bound-walks`: a partition's walks of its bound transactions, `sweep_bound` and the one an answer to a registration makes, timed with many open. The `alloc_bench` binary: the heap allocations between a DirectWrite's frame and its record under a counting global allocator, split into the frame's read, the transaction's write and its commit. Each run one JSON line, the clock behind a `Clock` trait so the pacer and summaries are tested without it. Results in the overview's "Measured" sections |
| `ultragraphdb_tool_test` | `ultragraphdb_tool/ultragraphdb_tool_test` | `ultragraphdb_cluster_partition`, `ultragraphdb_cluster_placement`, `ultragraphdb_cluster_sequencer`, `ultragraphdb_core_catalog`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_core_tok`, `ultragraphdb_error_store`, `ultragraphdb_store_apply`, `ultragraphdb_store_kv`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_store_sim`, `ultragraphdb_tool_bdd`, `ultragraphdb_traits_blocks`, `ultragraphdb_traits_store`, `ultragraphdb_transactions_commit`, `ultragraphdb_transactions_log`, `ultragraphdb_transactions_txn`, `fjall`, `oxrdf`, `tracing` | The helpers the store crates' test targets share, a dev-dependency of `ultragraphdb_store_kv`, `ultragraphdb_store_apply`, `ultragraphdb_cluster_partition`, `ultragraphdb_common_storage` and `ultragraphdb_storage_tests`: `TestDir` and the store's short write and read helpers, the crash writer's harness, the store workload run on fjall beside the simulated store (`run_both`, `Run`), the changeset applier's workload on the simulator (`ChangesetWorkload`), and `PartitionWorkload`, partitions on the simulator exchanging registrations under every fault class, a crash and a power cut included, with the invariants of `lean/PlacementModel` checked after every step. A slow suite splits into test targets of their own, which Bazel runs in parallel, and each reaches the shared helpers here |
| `ultragraphdb_error_role` | `ultragraphdb_error/ultragraphdb_error_role` | — | `RoleError(pub RoleErrorEnum)`: why a role stopped, and the `sysexits` code that says so |
| `ultragraphdb_common_service` | `ultragraphdb_common/ultragraphdb_common_service` | `ultragraphdb_common_service_config`, `ultragraphdb_common_telemetry`, `ultragraphdb_error_role`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_transport_http_client`, `ultragraphdb_transport_http_server`, `ultragraphdb_transport_tls`, `compio`, `signal-hook`, `tracing` | `run_service`: what every role binary does around its role: its settings fetched from the config service through `Bootstrap`, `/health` naming its target, instance and revision, `/ready`, `/admin/load-config` restarting it at a revision by `exec`, its runtime's CPU read back and its driver logged, io_uring where allowed and polling where not, `--help` listing its variables and exit codes, stopping on SIGTERM or SIGINT, or once the process `ULTRAGRAPHDB_PARENT_PID` names is no longer its parent (`stop_with_parent` for the binaries reading no `Bootstrap`); `listen` and `bind_tcp`, every TCP bind of a role and the config service, a port in use retried every 50 ms for up to 5 s; `InboxTurn`, the bounded share of its inbox one turn of the frontend's owner or a partition takes; `LatencyHistogram`, microsecond durations in buckets a sixteenth of an octave wide, and `PhaseWindow`, the phases of a role's requests over a window due a quiet second after its last request or a minute after its first, which the frontend and storage log |
| `ultragraphdb_common_role` | `ultragraphdb_common/ultragraphdb_common_role` | `ultragraphdb_common_service`, `ultragraphdb_error_role`, `ultragraphdb_protocol_scram`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_protocol_wire`, `compio`, `getrandom`, `tracing` | What every role does on its role connections: `RoleCluster` and `RoleSecret` from its settings, `RoleCredentials` verifying a peer locally, `RoleClient` dialling and authenticating, `Server` and `serve_connection` taking what an endpoint serves, `ConnectionBound` past which a connection is refused in place of its Welcome, `ParkedBytes` at whose bound a connection stops reading and goes on sending, `RoleClient::split` into a `RoleReader` and a `RoleWriter` for a role's multiplexed connection, `OpenRequests` and `finish_open`, by which a stop waits for every request taken to be answered, and `frontend_address` and `retry_until`, by which compute and storage join the frontend |
| `ultragraphdb_common_frontend` | `ultragraphdb_common/ultragraphdb_common_frontend` | `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_placement`, `ultragraphdb_cluster_sequencer`, `ultragraphdb_common_role`, `ultragraphdb_common_service`, `ultragraphdb_common_service_config`, `ultragraphdb_core_catalog`, `ultragraphdb_core_ids`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_error_cluster`, `ultragraphdb_error_protocol`, `ultragraphdb_error_role`, `ultragraphdb_protocol_scram`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_protocol_wire`, `ultragraphdb_traits_catalog`, `ultragraphdb_traits_log`, `ultragraphdb_transactions_log`, `compio`, `futures`, `getrandom`, `oxilangtag`, `oxrdf`, `oxsdatatypes`, `tracing` | The frontend role: `Owner`, the metadata group, sequencer and membership on one core, generic over `OrderedLog`; `OwnerHandle`, how endpoint cores ask it; `start_owner` on its own thread over `Wal`, with snapshots that trim the logs; `RoleSession` serving Join, Members, Placement, Catalog, DefaultEntry, Lease, Subscribe and CloseRequest; `ClientSession`, one per client connection: a synchronous state machine over `SessionEvent`s, which asks the owner, the fail-closed `TenantGate` and its one site, and relays through the core's `RoleLink`s (`RoleLinks`, one multiplexed connection per role, compute spread in turn) in `PROTOCOL.md` §13.20's order; `EndpointLimits`, `AuthThrottle` by address and name, `EveryAuthenticated`, the `Authorizer` of this stage; `OneFramePhases`, the phases of the one-frame writes an endpoint core relays (its checks, its partition's answer, the whole), logged as `one-frame write phases` once due; behind `test-utils`, on by default, `RoleDouble`, the partition and compute a test plays |
| `ultragraphdb_common_compute` | `ultragraphdb_common/ultragraphdb_common_compute` | `ultragraphdb_cluster_placement`, `ultragraphdb_common_role`, `ultragraphdb_common_service`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_terms`, `ultragraphdb_error_protocol`, `ultragraphdb_error_role`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_quads`, `compio`, `futures`, `oxrdf`, `tracing`, `xxhash-rust` | The compute role's reads, holding no storage engine: `ComputeCore`, one core's connections to the frontend and the partitions and what it read of them; `ComputeSession`, each ComputeRead a task on the core it arrived on, every Result frame within its `max_frame` under its stream's credit; a read routed by its bound positions, each partition past a dictionary Scan at the read's epoch before any list Scan, one version of a partition across its Scans, kept open past `read_retain_ms` by a Scan of empty ranges, every Scan held by an `EntriesSequence` and granted credit as its frames are consumed; the rows merged in key order over a `Frontier`, named by the dictionary of the partition answering; resume keys, resumed inside a list by a range's `after`; `ComputeCore::value_index` and `ComputeCore::reverse_counted`, the query core's own lookups; `CoreListener`, the socket every core accepts on |
| `ultragraphdb_common_storage` | `ultragraphdb_common/ultragraphdb_common_storage` | `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_partition`, `ultragraphdb_cluster_placement`, `ultragraphdb_common_role`, `ultragraphdb_common_service`, `ultragraphdb_core_catalog`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_error_protocol`, `ultragraphdb_error_role`, `ultragraphdb_error_store`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_apply`, `ultragraphdb_store_kv`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_traits_blocks`, `ultragraphdb_traits_catalog`, `ultragraphdb_traits_store`, `ultragraphdb_transactions_commit`, `ultragraphdb_transactions_log`, `ultragraphdb_transactions_txn`, `compio`, `futures`, `oxrdf`, `tracing` | The storage role: a thread and runtime per partition, each recovering over fjall and its write-ahead log, listening at its own address and joining for itself; one task owns each partition's host, and connections hand it whole frames by message, a turn taking at most 1,024 of its inbox's messages and of what other partitions send it (`REMOTE_PER_TURN_MAX`) and 64 epoch closes (`CLOSES_PER_TURN_MAX`); a request waiting on a home parked as a copy of its frame, queued per stream, its answer pushed, so the connection reads on until its parked bytes reach `storage.held_bytes`; Bind, Write, Operation, Commit and Abort applied from their frames; DirectWrite and ConditionalWrite run in one turn from the frame, a conflict retried `ONE_FRAME_ATTEMPTS` times; a Drop hosted through a Bind of kind 2; `HeldCatalogs`, the catalog versions fetched at a close that announces them; a Bind naming a tenant the map lists no run for refused; Scan, Closed and Recall answered by the read kernel, one frame per turn, each once the one before has left and credit allows it (`EntriesFill`, `ScanBoard`, `scan_version`, `SCAN_STALL_MS_DEFAULT`); each Write and Operation answered by Held; the discard version raised by `read_retain_ms` (`Retention`) off the core; Reference between processes; leases ahead of need behind `BlockSource`; a flush at shutdown; each commit's phases from its request taken to its reply handed out (`CommitMark`, `CommitPhase`, `CommitPhases`), its log's writes and syncs among them, timed by `TimedStorage` and `TimedFile` over the log's storage into `LogIo` (`LogIoTotals`), logged as `commit phases` once due |
| `ultragraphdb_transport_http_server` | `ultragraphdb_transport/ultragraphdb_transport_http_server` | `ultragraphdb_protocol_transport_compio`, `ultragraphdb_transport_tls`, `compio`, `sha2`, `subtle`, `tracing` | HTTP/1.1 for the control plane: one request per connection, bounded in size and time, no chunked bodies; `HttpService`, `serve_http`, `serve_listener` over TLS or clear text, serving at most `HttpLimits::CONNECTIONS_MAX` connections at once and pacing its failed accepts by `AcceptPace`; `BearerKey`, a key's digest compared in constant time |
| `ultragraphdb_transport_http_client` | `ultragraphdb_transport/ultragraphdb_transport_http_client` | `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_transport_tls`, `compio` | `HttpClient`, one request per connection over verified TLS or clear text; `HttpClientError::is_unreachable` separates what waiting may mend from what it may not; `read_key` |
| `ultragraphdb_frontend` | `ultragraphdb/ultragraphdb_frontend` | `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer`, `ultragraphdb_common_frontend`, `ultragraphdb_common_role`, `ultragraphdb_common_service`, `ultragraphdb_common_service_config`, `ultragraphdb_error_role`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_transport_tls`, `compio`, `mimalloc`, `tracing` | **Binary**, `main` and `service`, the role's work (`frontend_role`) |
| `ultragraphdb_compute` | `ultragraphdb/ultragraphdb_compute` | `ultragraphdb_common_compute`, `ultragraphdb_common_role`, `ultragraphdb_common_service`, `ultragraphdb_common_service_config`, `ultragraphdb_error_role`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_protocol_wire`, `compio`, `mimalloc`, `tracing` | **Binary**, `main` and `service`, the role's work (`compute_role`) |
| `ultragraphdb_storage` | `ultragraphdb/ultragraphdb_storage` | `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_partition`, `ultragraphdb_cluster_placement`, `ultragraphdb_cluster_sequencer`, `ultragraphdb_common_role`, `ultragraphdb_common_service`, `ultragraphdb_common_service_config`, `ultragraphdb_common_storage`, `ultragraphdb_core_catalog`, `ultragraphdb_core_ids`, `ultragraphdb_core_mapping`, `ultragraphdb_error_role`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_apply`, `ultragraphdb_store_kv`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_traits_blocks`, `ultragraphdb_traits_catalog`, `ultragraphdb_transactions_commit`, `ultragraphdb_transactions_log`, `compio`, `futures`, `getrandom`, `mimalloc`, `tracing` | **Binary**, `main` and `service`, the role's work (`storage_role`); `ultragraphdb_storage_profile`, the same keeping its symbols and frame pointers for a profiler |
| `ultragraphdb_common_config_check` | `ultragraphdb_common/ultragraphdb_common_config_check` | `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_placement`, `ultragraphdb_cluster_sequencer`, `ultragraphdb_common_service_config`, `ultragraphdb_protocol_transport`, `ultragraphdb_tool_toml` | The validation of settings files, called by the config service and its CLI alike: `read_central`, `check_central`, `check_target`, `check_config_service`, `instances`, `TargetFiles` with its bundle, `Defect`, `CheckReport`. Reads a folder, and needs no store, network or key |
| `ultragraphdb_common_config` | `ultragraphdb_common/ultragraphdb_common_config` | `ultragraphdb_common_config_check`, `ultragraphdb_error_log`, `ultragraphdb_traits_log`, `ultragraphdb_transport_http_server`, `futures` | The config service's store and API: `ConfigStore`, immutable numbered revisions as a state machine over `OrderedLog`; `ConfigRecord`, a log round's body; `store_owner`, the one task owning the store; `ConfigApi` |
| `ultragraphdb_config` | `ultragraphdb/ultragraphdb_config` | `ultragraphdb_common_config`, `ultragraphdb_common_config_check`, `ultragraphdb_common_service`, `ultragraphdb_common_service_config`, `ultragraphdb_common_telemetry`, `ultragraphdb_error_role`, `ultragraphdb_protocol_transport`, `ultragraphdb_transactions_log`, `ultragraphdb_transport_http_server`, `ultragraphdb_transport_tls`, `compio`, `mimalloc`, `tracing` | **Binary**, `main` and `service`, its start: settings and central file read and validated, the data directory locked, the log opened, the API served; the config service, static musl on the scratch image, run as its `nobody` |
| `ultragraphdb_supervisor` | `ultragraphdb/ultragraphdb_supervisor` | `ultragraphdb_common_config_check`, `ultragraphdb_common_service`, `ultragraphdb_common_service_config`, `ultragraphdb_common_telemetry`, `ultragraphdb_error_role`, `ultragraphdb_transport_http_client`, `compio`, `getrandom`, `mimalloc`, `rustix`, `signal-hook`, `tracing` | **Binary**, `main` and `service`: the development container's entrypoint, `--config` naming the config service's own settings file. It draws the two keys and the role secret where the settings name them when nothing is there; starts the config service, the frontend, storage and compute, each from the binaries beside it, waiting on each over HTTP; forwards SIGTERM and SIGINT, stops them in the shutdown order, restarts nothing, names itself in `ULTRAGRAPHDB_PARENT_PID` to everything it starts so killed outright it orphans nothing, and exits with the first failure's code, 128 plus the signal's number for a role a signal killed |
| `ultragraphdb_config_cli` | `ultragraphdb/ultragraphdb_config_cli` | `ultragraphdb_common_config_check`, `ultragraphdb_transport_http_client`, `ultragraphdb_transport_tls`, `compio`, `mimalloc` | **Binary**, `main` and `restore`: checks a central file and everything it declares, exiting 0 or 78; `restore` downloads the latest revision of every target a running service holds into a folder a service can begin from |
| `ultragraphdb_config_tests` | `ultragraphdb/ultragraphdb_config_tests` | `ultragraphdb_tool_bdd`, `ultragraphdb_transport_http_client`, `service_utils`, for tests only | The service and its CLI as processes, end to end with no container, tagged `integration_test` and `exclusive` |
| `ultragraphdb_supervisor_tests` | `ultragraphdb/ultragraphdb_supervisor_tests` | `ultragraphdb_client_compio`, `ultragraphdb_protocol_scram`, `ultragraphdb_protocol_wire`, `ultragraphdb_tests`, `ultragraphdb_tool_bdd`, `ultragraphdb_transport_http_client`, `service_utils`, for tests only | The supervisor as a process over the shipped `dev` profile with its paths and ports moved into the test's own, end to end with no container, tagged `integration_test` and `exclusive`. Its `image_bdd` target, tagged `image_test`, runs the scenarios tagged `@image` and `@docker` against the role images in Docker, under the runtime's default seccomp profile and the shipped one, among them a development container killed by `docker kill` and started again with every acknowledged commit read back, and says it skipped where no daemon answers |
| `ultragraphdb_tests` | `ultragraphdb/ultragraphdb_tests` | `ultragraphdb_common_role`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire`, `ultragraphdb_tool_bdd`, `compio`, `service_utils`; for its own tests `ultragraphdb_client_compio`, `ultragraphdb_client_tokio`, `ultragraphdb_common_config_check`, `ultragraphdb_common_service`, `ultragraphdb_core_ids`, `ultragraphdb_core_terms`, `ultragraphdb_error_protocol`, `ultragraphdb_protocol_scram`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_store_quads`, `ultragraphdb_transport_http_server`, `ultragraphdb_transport_tls`, `tokio` | The process harness every service's sister crate uses, the config service and its double, each binary's place on one host, `http_status`, `http_body`; whole clusters tested as processes, the client crates' clients through the three binaries over TCP, over Unix sockets and over TLS, through both shells, and the client's conformance conversation sent to them; `notes/group-10-scenarios.md` of the stage's change lists what binds where |
| `ultragraphdb_frontend_tests` | `ultragraphdb/ultragraphdb_frontend_tests` | `ultragraphdb_client_compio`, `ultragraphdb_common_frontend`, `ultragraphdb_common_role`, `ultragraphdb_common_service`, `ultragraphdb_protocol_scram`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_transport_compio`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_quads`, `ultragraphdb_tests`, `ultragraphdb_tool_bdd`, `ultragraphdb_transport_tls`, `service_utils`, for tests only | The frontend binary's full integration test, as processes with no container, with the dependency-graph check of a role holding no storage engine |
| `ultragraphdb_compute_tests` | `ultragraphdb/ultragraphdb_compute_tests` | `ultragraphdb_cluster_placement`, `ultragraphdb_common_compute`, `ultragraphdb_common_role`, `ultragraphdb_common_service`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_terms`, `ultragraphdb_error_protocol`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_quads`, `ultragraphdb_tests`, `ultragraphdb_tool_bdd`, `service_utils`, for tests only | The compute binary's full integration test, as processes with no container, with the dependency-graph check of a role holding no storage engine |
| `ultragraphdb_storage_tests` | `ultragraphdb/ultragraphdb_storage_tests` | `ultragraphdb_cluster_placement`, `ultragraphdb_common_role`, `ultragraphdb_common_service`, `ultragraphdb_common_storage`, `ultragraphdb_core_ids`, `ultragraphdb_core_keys`, `ultragraphdb_core_mapping`, `ultragraphdb_core_terms`, `ultragraphdb_protocol_transport`, `ultragraphdb_protocol_wire`, `ultragraphdb_store_kv`, `ultragraphdb_store_quads`, `ultragraphdb_tests`, `ultragraphdb_tool_bdd`, `ultragraphdb_tool_test`, `ultragraphdb_transport_http_client`, `ultragraphdb_transport_tls`, `service_utils`, for tests only | The storage binary's full integration test, as processes with no container |
| `ultragraphdb_common_service_config` | `ultragraphdb_common/ultragraphdb_common_service_config` | `ultragraphdb_tool_toml` | `ServiceConfig`: the settings a binary runs on, read from bytes, one type per TOML table and one field per setting, every table and key taken or refused; `EndpointSecurity`; `ServiceConfigError` |
| `ultragraphdb_tool_toml` | `ultragraphdb_tool/ultragraphdb_tool_toml` | — | The roles' settings files: an in-house parser of the subset of TOML 1.0 they use (blank lines, comments, `[table]` headers with a bare name, root keys, `key = value` with a bare key and a basic string, a decimal `i64` or a boolean), read in one pass with no recursion, bounded at 64 KiB, 4 KiB a line, 256 tables and 4,096 keys, and parsing as any TOML 1.0 parser reads it: `TomlDocument` with its `TomlTable`s, `TomlEntry`s and `TomlValue`s, the typed reads `string`, `integer` and `boolean` and `finish`, which refuses any setting no read took; `TomlError` with `TomlFeature` (every feature outside the subset, refused by name and line), `TomlInvalid` (every rule of TOML 1.0 broken), `TomlLimit` and `TomlKind`. Pinned against the toml-test suite v1.6.0, vendored under `testdata/toml-test/` |

### External dependencies

Every external crate is pinned once in `[workspace.dependencies]` and referenced by member crates as
`{ workspace = true }`. Add a new one there or nowhere. Choose a pure-Rust crate wherever one exists (`lz4_flex` over
`zstd`, `miniz_oxide` over zlib): it keeps cross-compilation to static musl free of a C toolchain per target.

| Crate                                 | Version               | Used by                             | Why                                                                                                     |
|---------------------------------------|-----------------------|-------------------------------------|---------------------------------------------------------------------------------------------------------|
| `tokio`                               | 1.54; features: `net`, `io-util`, `time`, and `rt` and `test-util` in the client's tests | `ultragraphdb_client_tokio` | The primary public Rust client's runtime: most Rust applications run tokio, so the client's first shell is a tokio one with `Send` futures (decided 2026-10-28). Pure Rust: `mio`, `socket2`, `bytes`, `pin-project-lite`, and `libc` as bindings only, no C toolchain |
| `bcrypt` | 0.19 | `ultragraphdb_core_types` | Hashes and verifies passwords as Go's bcrypt does, `$2a$` hashes included |
| `oxrdf`, `oxsdatatypes` | 0.3, `rdf-12` / 0.2 | `oxrdf`: `ultragraphdb_core_terms`, `ultragraphdb_core_mapping`, `ultragraphdb_core_catalog`, `ultragraphdb_store_quads`, `ultragraphdb_store_apply`, `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_commit`, `ultragraphdb_cluster_partition`, `ultragraphdb_common_frontend`, `ultragraphdb_common_compute`, `ultragraphdb_common_storage`, `ultragraphdb_tool_bench`, `ultragraphdb_tool_test`; `oxsdatatypes`: `ultragraphdb_core_terms`, `ultragraphdb_store_apply`, `ultragraphdb_common_frontend` | The RDF term model and the XSD datatypes' parsing and canonical forms, from Oxigraph |
| `oxilangtag` | 0.1 | `ultragraphdb_transactions_txn`, `ultragraphdb_common_frontend` | A language tag checked as BCP 47 well-formed, as RDF requires, where a Write frame's table and a client's terms carry one; Oxigraph's, already beneath `oxrdf`. Pure Rust |
| `dashu-int` | 0.6 | `ultragraphdb_core_types`, `ultragraphdb_common_utils` | Exact integer arithmetic beneath the port of Go's `math/big` for 200-bit big floats; `dashu_int::ops` re-exports the traits of `dashu-base` it builds on |
| `rust-stemmers` | 1.2 | `ultragraphdb_core_tok` | The Snowball stemmers of nine languages, checked against Go on golden vectors |
| `unicode-normalization` | 0.1 | `ultragraphdb_core_tok` | NFKC at Unicode 17.0.0, as Go's `x/text` v0.41 |
| `serde`, `serde_json` | 1.0 | `ultragraphdb_tool_bdd`; every core crate's tests | Golden files |
| `cucumber`, `futures` | 0.23 / 0.3 | every crate's `bdd` test | The BDD scenarios, run without the libtest harness; `futures::executor::block_on` drives them |
| `criterion` | 0.8, no default features | `ultragraphdb_store_kv`, `ultragraphdb_transactions_commit`, `ultragraphdb_transactions_txn` (benches) | The kv store's benchmarks, the conflict history's and the read set's, run by `cargo bench` and `bazel run`, never by `make test`. 0.8 depends on `alloca`, whose build script compiles C through `cc`, in the bench targets alone |
| `fjall` | 3.1 | `ultragraphdb_store_kv`, `ultragraphdb_tool_test` | The LSM-tree storage engine, pure Rust. `Store::compact` calls fjall's `#[doc(hidden)]` `rotate_memtable_and_wait` and `major_compact`, which carry no stability promise |
| `moka` | 0.12, `sync` | `ultragraphdb_store_kv` | The posting-list cache, keyed with a per-process `RandomState` |
| `tracing` | 0.1 | every library crate that logs | Structured log events where the ported Go code logs; its macros are used in `src/` |
| `service_utils` | 0.4 | `ultragraphdb_tests` and the `*_tests` crates, tests only | Starts a role binary as a process, waits on its health and reads its exit code, in `std` alone |
| `futures` | 0.3 | `ultragraphdb_common_frontend`, `ultragraphdb_common_storage`, `ultragraphdb_common_compute`, `ultragraphdb_common_config`, `ultragraphdb_storage`, `ultragraphdb_tool_bench` | The channels by which endpoint cores ask the frontend's owner, connections hand a partition's host their frames and the config service's API asks its store's owner, each waking a runtime on another thread or task; `client_bench` awaits its connections together with `join_all`. Pure Rust, and already in the tree for the tests |
| `signal-hook` | 0.4, no default features | `ultragraphdb_common_service`, `ultragraphdb_supervisor` | Setting a flag from SIGTERM and SIGINT with safe code, which `unsafe_code = "forbid"` otherwise rules out; compio's own signal feature needs a prerelease crate |
| `tracing-subscriber` | 0.3, `fmt` + `json` + `env-filter` | `ultragraphdb_common_telemetry`, `ultragraphdb_store_sim` | The level filter and plain or JSON output; the simulator's capturing subscriber |
| `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp`, `opentelemetry-http`, `async-trait` | 0.33, `trace`; `http-proto` / 0.33, no default features / 0.1 | `ultragraphdb_common_telemetry` | Span export over OTLP/HTTP with protobuf, `prost` beneath `opentelemetry-otlp`, sent by the crate's own `OtlpHttpClient` through `opentelemetry-http`'s `HttpClient` trait, which `async-trait` implements. A bundled client would add hyper (68 crates in all) or reqwest (100) where the own client keeps it at 49, against 79 over gRPC on tonic |
| `tracing-opentelemetry` | 0.34 | `ultragraphdb_common_telemetry` | Bridges `tracing` spans to OpenTelemetry |
| `mimalloc` | 0.1, `local_dynamic_tls` | every binary under `ultragraphdb/`, `ultragraphdb_tool_bench`'s `store_bench`, `wal_bench` and `client_bench` | The `#[global_allocator]` of every shipped binary: musl's own allocator contends under threads, measured at 31% fewer commits per second than glibc's on `wal_bench` (2026-11-01). Compiled from C by `libmimalloc-sys` |
| `stats_alloc` | 0.1.10 | `ultragraphdb_tool_bench`'s `alloc_bench` | A counting wrapper over the system allocator, installed as `alloc_bench`'s `#[global_allocator]`, so the allocations between a frame and its record are counted with no `unsafe` of this workspace's own, which `GlobalAlloc` otherwise needs. Pure Rust, no dependencies |
| `socket2` | 0.6 | `ultragraphdb_protocol_transport_compio` | TCP keepalive on a compio socket (`PROTOCOL.md` §11.4), which compio's `TcpStream` does not set. Pure Rust over `libc`, and already in the tree under compio |
| `compio` | 0.19, no default features; `runtime`, `io-uring`, `polling`, `fs`, `net`, `time` | `ultragraphdb_protocol_transport_compio`, `ultragraphdb_client_compio`, `ultragraphdb_transactions_log`, `ultragraphdb_transport_http_server`, `ultragraphdb_transport_http_client`, `ultragraphdb_common_service`, `ultragraphdb_common_role`, `ultragraphdb_common_frontend`, `ultragraphdb_common_compute`, `ultragraphdb_common_storage`, every binary under `ultragraphdb/`, `ultragraphdb_tests`, `ultragraphdb_tool_bench` | The completion-based runtime of every role and service, thread per core, the write-ahead log, the transport's compio shell, the control plane's HTTP, the compio client and `wal_bench`, io_uring on Linux, and its polling driver where io_uring's system calls are refused, as Docker's default seccomp profile refuses them; the stable release Apache Iggy builds on. Pure Rust on every shipped target: `cc` enters only through `loom` under `cfg(loom)` |
| `xxhash-rust` | 0.8, `xxh3` | `ultragraphdb_protocol_wire`, `ultragraphdb_core_codec`, `ultragraphdb_core_ids`, `ultragraphdb_core_terms`, `ultragraphdb_store_posting`, `ultragraphdb_store_quads`, `ultragraphdb_cluster_metadata`, `ultragraphdb_common_compute` | XXH3-64, as xxHash's `doc/xxhash_spec.md` specifies it: the one non-cryptographic hash, for frame, chunk, WAL and posting-list checksums, the metadata group's command checksums, the scope of compute's reads of one transaction, a system ID's bucket, the posting fingerprint, the rollup queue's deduplication and the statistics' sketches; pure Rust |
| `sha2`, `hmac`, `subtle`, `base64`, `stringprep` | 0.11 / 0.13 / 2.6 / 0.23 / 0.1 | `ultragraphdb_protocol_scram`; `sha2` also `ultragraphdb_core_terms`, `ultragraphdb_core_tok`; `sha2` and `subtle` also `ultragraphdb_transport_http_server` | SHA-256 is the one cryptographic hash: SCRAM-SHA-256 (RFC 5802, RFC 7677) of the Authenticate exchange with HMAC and the constant-time comparison of a proof, from RustCrypto, and `SaltedPassword`'s `Hi`, PBKDF2's one block, written over that HMAC in `ultragraphdb_protocol_scram`; a dictionary entry's 128-bit hash (SHA-256 truncated to 16 bytes); the hash tokenizer's digest and a long shingle's; a bearer key's digest, compared in constant time by the control plane's HTTP. The base64 of SCRAM's messages; SASLprep (RFC 4013) of names and passwords. Pure Rust |
| `getrandom` | 0.4 | `ultragraphdb_protocol_scram`, `ultragraphdb_common_role`, `ultragraphdb_common_frontend`, `ultragraphdb_storage`, `ultragraphdb_supervisor` | The operating system's random source, for SCRAM's client and server nonces (`system_nonce`), the salts of the role secret's and the bootstrap administrator's verifiers, the metadata group's secret for unknown names, session and transaction identities, a partition's seed and the secrets the supervisor draws; already in the lockfile beneath other crates. Pure Rust: system calls through `libc`'s bindings |
| `rustix` | 1.1, no default features; `fs`, `process` | `ultragraphdb_transactions_log`, on Linux only; `ultragraphdb_supervisor` | `fallocate(2)` with `FALLOC_FL_KEEP_SIZE`, which reserves a new WAL segment's blocks without writing zeros; compio has no such call and the workspace forbids `unsafe`. `kill(2)`, by which the supervisor forwards a signal to a role, in an image holding no `kill` tool. Pure Rust: its Linux backend makes raw system calls with `linux-raw-sys`'s generated constants, which builds nothing, and its build script only probes `rustc`; macOS does not depend on it |
| `rustls`, `rustls-pki-types` | 0.23, no default features; `ring`, `tls12`, `std` / 1, `std` | `ultragraphdb_transport_tls` | TLS for OTLP over HTTPS and, later, the wire protocol's TLS (`PROTOCOL.md` §12): rustls with ring as its provider. ring compiles C and assembly through `cc`, against the pure-Rust rule: the pure-Rust provider, `rustls-rustcrypto`, is an alpha its README says not to use in production, and has no audit (checked 2026-11-02) |
| `rcgen` | 0.14, no default features; `pem`, `ring` | `ultragraphdb_transport_tls`'s `test-utils` only | Generates `TestPki`'s certificate authority and server certificate in the test process, so no certificate is committed and no tool outside the build creates one |
| `arbitrary` | 1.5, `derive` | the fuzz workspace in `fuzz/` only | Structure-aware inputs for the `cargo-fuzz` targets run by `scripts/fuzz.sh`; no shipped crate depends on it |

## Building and Running

The project uses `make` for common development tasks:

* `make build`: Builds the entire workspace with Bazel
* `make test`: Builds, then runs the doc tests, the unit tests, the BDD tests and the integration tests with Bazel. Integration tests start the role binaries as processes, each copied into the sandbox, with no container, so Bazel caches them; they carry the tags `integration_test` and `exclusive`, since the binaries listen at declared ports
* `make fix`: Fixes linting issues as reported by `clippy`
* `make format`: Formats Bazel files with `buildifier` and Rust code with `cargo fmt`
* `make check`: Checks for outdated and unused dependencies and security vulnerabilities
* `make doc`: Builds the documentation and runs the doc tests
* `make count`: Counts lines of code
* `make update`: Updates the Rust toolchain and Cargo dependencies
* `make cluster-local`, `make cluster-local-bench`, `make cluster-local-status`, `make cluster-local-down`: the roles on a local
  Kubernetes cluster of Apple's `container`, from the Helm chart; see *Local Kubernetes* below. None runs in `make test`

Bazel is the build of record; Cargo serves the IDE. Bazel resolves external crates through
`all_crate_deps()` reading the Cargo manifests, so a dependency added to a member `Cargo.toml` needs no
`BUILD.bazel` edit.

### Local Kubernetes

`deploy/helm/ultragraphdb` is the Helm chart of one cluster of the roles, one release per namespace, as its objects take fixed
names: the config service, the frontend and the storage instance `storage` as StatefulSets of one replica, each with a volume for
its data directory; compute as a Deployment; a headless Service per workload, publishing not-ready addresses; the client Service
`frontend-client` on 7080; the four Secrets; the ConfigMap `ultragraphdb-profile`; and, with `bench.enabled`, one run of
`client_bench` (`//ultragraphdb_tool/ultragraphdb_tool_bench:client_bench_multiarch`) as a Job. Each role's role address is its
pod's name under its headless Service, set in the chart's `kube` profile, `profile/`, which the config battery checks beside the
shipped profiles and whose namespace and cluster domain the chart replaces by the release's. The chart draws the keys, the role
secret and the bootstrap password at the first install and keeps them through upgrades and an uninstall (`secrets.create: false`
expects them to exist). The roles run as 65532 and the config service as 65534, read-only, with no capability, under the shipped
seccomp profile as a `Localhost` profile, so they run on io_uring; `values.yaml` sets the images, the pull policy, the storage
instance's partitions, the cluster domain, the volumes, the seccomp profile and the bench.

`scripts/cluster_local.sh`, behind the four `cluster-local` make targets, needs Apple's `container` (1.7 or later) on a Mac with
Apple silicon and `helm` on the host. It renders the chart with `helm template` and drives the cluster `ultragraphdb` from inside
its control-plane node, so the host needs no `kubectl`:

* `up` (`make cluster-local CPUS=12 MEMORY=16g PARTITIONS=1`, a node of that many CPUs, as the script asks `container` for one
  fewer, since it gives a machine one more than asked) builds each image's `oci_tarball` with `-c opt`, loads it into `container`'s
  image store and tags its arm64 manifest `ultragraphdb/<role>:local`; creates the cluster when absent with
  `--shared-image-store`, its kubeconfig in `target/cluster_local/`; applies kind's `standard` StorageClass; installs the seccomp
  profile on every node; drops each node's record of the images, so the pods take the ones just loaded; makes the namespace and
  the chart's Secrets where absent, as `helm template` reads no cluster and would draw new values; applies the rest of the chart
  with `imagePullPolicy: Never`, restarting the workloads when the cluster already ran; waits for every rollout; and reports the
  `container` version, the nodes' sizes, the pods and the driver each role's runtimes started on.
* `bench` (`make cluster-local-bench BENCH="--op direct --connections 16 --seconds 20"`, those arguments the default) renders the
  chart's bench Job with those arguments, runs it against `frontend-client` and prints its JSON line.
* `status` and `down` report on and delete the cluster.

Two tools run through Cargo, as approved exceptions to building and testing with Bazel alone, because neither is
hermetic nor has a fixed end:

* `scripts/mutants.sh <crate> --diff <base> [--re <regex>]`: mutation testing with `cargo-mutants`, which
  changes one expression at a time in a copy of the tree and runs the crate's suite. One run covers every line of the
  crate changed since `<base>`, untracked files included, so a crate pays its baseline once; a file argument is
  refused. The script allows three runs per crate and base, takes one machine-wide lock, and runs eight jobs. A survivor is a gap in the suite until a test kills it or an entry in `.cargo/mutants.toml` records why the
  mutant cannot change behaviour; one still open after three runs is recorded for the stage's final verification.
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
`tests/bdd.rs` is each crate's own `bdd` target, the client-side scenarios with the crate's harness or server included by
path: the core's, its steps under `tests/bdd_steps/`, what the core decides; a shell's, automatic credit and TLS.
`ultragraphdb_client_core` is tested synchronously, with no runtime: `tests/utils/harness.rs` drives
the core against `TestServer` over the simulated network, under drop, delay, duplicate and split,
moving time itself. `TestServer` is the minimal server covering the core: the transport's
`ServerConnection` for the connection's rules, one fixed answer per request, the session table, and
faults a test names. It is `src/utils_test/`, behind the
`test-utils` feature, so the shells' tests serve the same server; the core's `BUILD.bazel` sets
`crate_features = ["test-utils"]`, and its own dev-dependency on itself turns it on under Cargo.
`ultragraphdb_client_tokio` and `ultragraphdb_client_compio` run the same conversations through
their shells, over the simulated network and a TCP loopback, in clear text and TLS, with the server served from
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
  value encodings), golden vectors generated from the Go source sit under
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

## Service binaries

Every service binary, `ultragraphdb_frontend`, `ultragraphdb_compute`, `ultragraphdb_storage`, `ultragraphdb_config` and `ultragraphdb_supervisor`, is two files
and no library: `src/main.rs`, its allocator and `main`, and `src/service.rs`, the service's own work. What more than one service shares,
or what is worth testing apart from a process, stays in `ultragraphdb_common/`.

Each has a test-only sister crate, named for it with `_tests`, holding its full integration test: its binaries copied into the sandbox and
started as processes through `service_utils`, with no container, tagged `integration_test` and `exclusive`. A `service.rs` is covered there.
`ultragraphdb_tests` holds what they share and the whole-cluster scenarios.

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
* Every error is `pub struct FooError(pub FooErrorEnum)` over an exhaustive enum, the shape of `deep_causality_physics`'
  `PhysicsError`, with `Display`, `std::error::Error` and one constructor per variant named as the variant in snake case
  (`FooErrorEnum::Variant` is built by `FooError::variant(..)`), so no constructor needs a lint exception. The inner
  field is the one public field a public type carries, so callers match on `e.0`.

Panics:
* Code under `src/` does not panic on any input. Where Dgraph panics, the port returns its crate's error with a clear
  message; the golden file keeps Go's panic as evidence, and the test names the case as a deviation. An `expect`
  remains only where a local invariant proves it cannot fire, and its message states the invariant.
* Tests do not panic either: `unwrap`, `expect` and `panic!` are denied in tests as in `src/`. A test or a cucumber
  step returns `ultragraphdb_tool_bdd::TestResult` and fails through `?`, with `.context("what")?` naming an absent
  value or a failed result, and through `assert!`/`assert_eq!`.

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
  `Cargo.toml`; new crates MUST include this. The workspace's table is the only lint table, Ultra Style's lint set: no
  crate adds lints of its own, and a lint the crates do not meet yet stays in the table commented out, with its count
  of violations, until they do.
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
