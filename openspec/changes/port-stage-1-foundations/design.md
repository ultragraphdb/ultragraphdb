## Context

ultragraphdb ports Dgraph v25.5.0 (the fork at `../../fork/dgraph`, commit `9fbf35614`) to Rust. The plan is in
`openspec/notes/dgraph-port-overview.md`. The port runs in five stages, each one change set, ordered by Dgraph's
package import graph (`go list` over the fork, GraphQL, test harnesses and tools excluded):

| Stage | Change | Go packages | Production lines |
|---|---|---|---|
| 1 | `port-stage-1-foundations` | `x` (pure parts), `protos/pb`, `codec`, `algo`, `types`, `types/facets`, `lex`, `dql`, `tok`, `tok/index`, `tok/options`, `tok/constraints`, `schema` (parsing, state), `chunker` (parsers) | ~18k |
| 2 | `port-stage-2-storage` | `posting`, `tok/hnsw`, storage parts of `schema` and `worker` (apply, schema change, rebuild) | ~9k |
| 3 | `port-stage-3-single-node` | `worker/task.go` and its helpers, `query`, `edgraph`, `acl`, Zero's oracle and leases, the Alpha and Zero binaries | ~24k |
| 4 | `port-stage-4-replication` | `raftwal`, `conn`, `worker/draft.go`, `proposal.go`, `snapshot.go`, `groups.go`, Zero's Raft | ~7k |
| 5 | `port-stage-5-sharding-and-operations` | tablets, predicate moves, cross-group queries and commits, backup, restore, export, the bulk and live loaders, import | ~14k |

Stage 1 covers the packages at the bottom of that graph. None of them reads storage or opens a socket. Every
later stage names their types: keys, posting and UID-pack messages, scalar values, tokens, parsed queries and
schemas, edges.

The fork adds code upstream Dgraph lacks (the ngram tokenizer among it); the port follows the fork.

## Goals / Non-Goals

**Goals:**

- Port the stage-1 packages with Dgraph's behaviour, its error texts included, so that later stages and the
  differential runs against Go Dgraph see the same results.
- Byte-identical keys, packed UID lists, value encodings and tokens, checked against golden vectors.
- The BDD and TDD harness every stage uses, running under Cargo and Bazel with equal counts.
- The crate layout of the port, so later stages add crates beside these.

**Non-Goals:**

- Storage, networking, query execution: stages 2 to 5.
- Loading the schema from storage (`schema.Init`, `Load`, `LoadFromDb`): stage 2.
- Reading compressed or encrypted input files (`chunker`'s file and stream readers): stage 5, with the loaders.
- Tenancy resolution and JWTs (`x/tenancy.go`, `x/jwt_helper.go`): stage 3.
- Custom tokenizer plugins (Go `plugin`) and the count-min sketch (`algo/cm-sketch.go`, used only by posting
  statistics).

## Decisions

### Crates follow Dgraph's packages, grouped by layer

| Crate | Path | Ports |
|---|---|---|
| `proto_pb` | `ugdb_proto/proto_pb` | `protos/pb.proto`, with the Badger `pb.proto` messages it imports (`KV`, `KVList`) |
| `core_keys` | `ugdb_core/core_keys` | `x/keys.go`, the reserved and pre-defined predicate tables, the pure helpers of `x` |
| `core_codec` | `ugdb_core/core_codec` | `codec` and the group-varint encoding |
| `core_algo` | `ugdb_core/core_algo` | `algo` without the count-min sketch |
| `core_types` | `ugdb_core/core_types` | `types` and `types/facets` |
| `core_lex` | `ugdb_core/core_lex` | `lex` |
| `core_tok` | `ugdb_core/core_tok` | `tok`, `tok/index`, `tok/options`, `tok/constraints` |
| `core_dql` | `ugdb_core/core_dql` | `dql` |
| `core_schema` | `ugdb_core/core_schema` | `schema/parse.go`, `schema/state.go`, the in-memory part of `schema/schema.go` |
| `core_chunker` | `ugdb_core/core_chunker` | `chunker`'s RDF and JSON parsers |
| `test_bdd` | `ugdb_test/test_bdd` | The traceability check and shared step helpers |

One crate per Go package keeps the mapping from a Rust file to its Dgraph source obvious, which the attribution
rule needs anyway. The crate graph mirrors Dgraph's imports, and Cargo refuses a cycle.

Alternative: one `ugdb_core` crate holding everything. That rebuilds and re-tests all of stage 1 on any change,
and loses the import graph as a check.

### Byte-identical formats, checked by golden vectors

Keys, UID packs, value encodings and tokens equal Dgraph's byte for byte. Query results depend on tokens, since
an index lookup is a token lookup, and on token order, since sorting and ranges walk index keys. Identical bytes
are the simplest way to guarantee identical results, and they keep Go-written backups and exports readable later.

The golden generator is a Go program under `tools/golden/`, with a `go.mod` that points at the fork via a
`replace`. It writes JSON files under each crate's `testdata/golden/`, recording the Dgraph commit, and
`scripts/golden.sh` runs it. Bazel and Cargo read the committed files; neither needs Go.

Alternative: assert only semantic equality (same query results). That needs a running stage 3 before any
tokenizer can be checked.

### BDD with cucumber-rs; TDD with ported Go tests

- Each crate has `features/<capability>/*.feature`, one Gherkin scenario per spec scenario, word for word, and
  `tests/bdd.rs` running them with `cucumber`. `Cargo.toml` declares `[[test]] name = "bdd" harness = false`;
  `BUILD.bazel` declares a `rust_test` with `use_libtest_harness = False`, the feature files as `data`, tagged
  `bdd-test`.
- `test_bdd` holds the traceability test. It reads every `#### Scenario:` under `openspec/specs/` and
  `openspec/changes/*/specs/`, and every `Scenario:` under `**/features/`, and fails on a name present on one
  side only.
- Dgraph's Go tests are ported to `tests/` as `AGENTS.md` describes, a ported test file named after its Go file.
  Table-driven Go tests keep their tables, so a case maps back to its Go line.
- `scripts/test.sh` runs `--test_tag_filters=bdd-test` after the unit tests.

Alternative: plain `#[test]` functions named after scenarios. Spec and test then drift apart without anything
noticing.

### External libraries for the byte-exact formats

| Format | Library | Notes |
|---|---|---|
| UID deltas | ported group-varint | The format is small; a hand port matches it exactly |
| Hash and ngram tokens | `blake2` | BLAKE2b-256 |
| Passwords | `bcrypt` | Verifies `$2a$` hashes from Go |
| Snowball stemmers (da, fi, hu, nl, no, ro, ru, sv, tr) | `rust-stemmers` | Checked per language against golden vectors |
| English (Porter), Lucene light stemmers (de, es, fr, it, pt), ar, ckb, hi | ported from bleve | bleve's English stemmer is Porter, not Snowball English |
| Stop words (28 languages) | ported tables | Copied from bleve's lists |
| Word boundaries, NFKC | `unicode-segmentation`, `unicode-normalization` | Unicode version pinned to bleve's where they differ |
| Collation keys | to be chosen by the spike in task group 2 | See Risks |
| S2 cells and covers | an S2 port | See Risks |
| WKB, GeoJSON | `geo-types`, `wkb`, `geojson` | WKB little-endian, as go-geom writes it |
| Big floats | `dashu-float` or a ported formatter | Go's shortest `g` text at 200-bit precision; pure Rust, so it builds for static musl |

Each choice is confirmed by the golden vectors of its tokenizer or type before the tokenizer is called ported.

### GraphQL cut, its reserved names kept

`x.GqlError` and `ExportedGQLSchema` are not ported. The `dgraph.graphql.*` predicates and types stay in the
reserved and initial schema tables, because Go-written data holds them.

### The vector index registry entry precedes the index

`tok` registers the hnsw factory and the schema parser validates `@index(hnsw(...))` against its options. Stage 1
ports the registry entry with the option set; the index itself is stage 2.

## Risks / Trade-offs

- [Collation keys differ from Go's `x/text/collate`] → Exact-with-language tokens and language sorting depend on
  them. Task group 2 opens with a spike comparing candidate libraries (ICU4X, a port of `x/text/collate`) on the
  golden strings; a port of Go's tables is the fallback.
- [The S2 region coverer differs from Go's `golang/geo`] → Geo tokens would differ and geo queries would miss.
  The geo tokenizer is accepted only when its golden vectors pass; a port of the coverer is the fallback.
- [Unicode versions differ between bleve's segmenter and the Rust crates] → Word boundaries could split
  differently on recent scripts. The golden corpus includes text from every script bleve's tests cover.
- [Dgraph's error texts are part of the contract] → Clients and tests match on them. Error texts are copied,
  and the ported parser tests assert them.
- [Bazel and cucumber-rs] → `cucumber` expects to find feature files relative to the crate. The first task
  proves a feature run under both build systems before any other scenario is written.
- [Blank-node names in JSON input carry a random component] → Differential tests normalise `_:dg.<id>.<n>` to
  `<n>`.

## Migration Plan

New crates only; nothing existing changes besides `scripts/test.sh` and the workspace manifests.

## Open Questions

- Which collation library produces Go's `x/text/collate` keys, or is a port of its tables needed?
- Which S2 implementation reproduces `golang/geo`'s region covers?
