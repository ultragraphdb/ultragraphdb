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

The fork is v25.5.0 plus six performance and dependency commits that do not change behaviour; the port follows the
fork.

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
| `ultragraphdb_proto_pb` | `ultragraphdb_proto_pb` | `protos/pb.proto`, with the Badger `pb.proto` messages it imports (`KV`, `KVList`, `Match`) |
| `ultragraphdb_core_keys` | `ultragraphdb_core_keys` | `x/keys.go`, the reserved and pre-defined predicate tables, the pure helpers of `x` |
| `ultragraphdb_core_codec` | `ultragraphdb_core_codec` | `codec` and the group-varint encoding |
| `ultragraphdb_core_algo` | `ultragraphdb_core_algo` | `algo` without the count-min sketch |
| `ultragraphdb_core_types` | `ultragraphdb_core_types` | `types` |
| `ultragraphdb_core_lex` | `ultragraphdb_core_lex` | `lex` |
| `ultragraphdb_core_tok` | `ultragraphdb_core_tok` | `tok`, `tok/index`, `tok/options`, `tok/constraints`, and from `tok/hnsw` the option and metric names, `AllowedOptions` and the `__vector_` suffix constants |
| `ultragraphdb_core_text` | `ultragraphdb_core_text` | From Go's `golang.org/x/text`: `collate` with its CLDR 23 tables, and the part of `language` that reduces a tag to its base; BSD-3-Clause, its notice kept in every ported file |
| `ultragraphdb_core_facets` | `ultragraphdb_core_facets` | `types/facets`, which imports `tok` while `tok` imports `types` |
| `ultragraphdb_core_dql` | `ultragraphdb_core_dql` | `dql` |
| `ultragraphdb_core_schema` | `ultragraphdb_core_schema` | `schema/parse.go`, `schema/state.go`, the in-memory part of `schema/schema.go` |
| `ultragraphdb_core_chunker` | `ultragraphdb_core_chunker` | `chunker`'s RDF and JSON parsers |
| `ultragraphdb_tool_bdd` | `ultragraphdb_tool_bdd` | The traceability check and shared step helpers |

One crate per Go package keeps the mapping from a Rust file to its Dgraph source obvious, which the attribution
rule needs anyway. The crate graph mirrors Dgraph's imports, and Cargo refuses a cycle.

Alternative: one `ultragraphdb_core` crate holding everything. That rebuilds and re-tests all of stage 1 on any change,
and loses the import graph as a check.

### Byte-identical formats, checked by golden vectors

Keys, UID packs, value encodings and tokens equal Dgraph's byte for byte. Query results depend on tokens, since
an index lookup is a token lookup, and on token order, since sorting and ranges walk index keys. Identical bytes
are the simplest way to guarantee identical results, which the differential runs against Go Dgraph compare.

The golden generator is a Go program under `tools/golden/`, with a `go.mod` that points at the fork via a
`replace`. It writes JSON files under each crate's `testdata/golden/`, recording the Dgraph commit, and
`scripts/golden.sh` runs it. Ngram and geo tokens come out of Dgraph in map-iteration order, so the generator sorts
them bytewise and the Rust tests compare sorted. Bazel and Cargo read the committed files; neither needs Go.

Alternative: assert only semantic equality (same query results). That needs a running stage 3 before any
tokenizer can be checked.

### BDD with cucumber-rs; TDD with ported Go tests

- Each crate has `features/<capability>/*.feature`, one Gherkin scenario per spec scenario, word for word, and
  `tests/bdd.rs` running them with `cucumber` under `futures::executor::block_on`. `Cargo.toml` declares
  `[[test]] name = "bdd" harness = false`; `BUILD.bazel` declares a `rust_test` with `use_libtest_harness = False`, the
  feature files as `data`, tagged `bdd-test`.
- The runner finds its features at runtime: under Bazel in `$TEST_SRCDIR/$TEST_WORKSPACE/<package>/features`, under
  Cargo in `$CARGO_MANIFEST_DIR/features`, read from the environment and never through `env!`, because `rules_rs`
  refuses any compiled output that embeds the sandbox path.
- `cucumber` 0.23 embeds `env!("CARGO_MANIFEST_DIR")` as a fallback for relative feature paths
  (`src/parser/basic.rs:83`), which `rules_rs` refuses; `MODULE.bazel` sets that variable to `.` for the crate with a
  `crate.annotation(crate = "cucumber", rustc_env = ...)`. The runner passes absolute paths, so the fallback is never
  used.
- Proved on 2026-10-20 in a scratch copy of this workspace (Bazel 9.3.0, `rules_rs` 0.1.0, `cucumber` 0.23.0): both
  build systems ran the same 2 scenarios; a failing step failed the Bazel target; missing feature files failed it with
  a parsing error rather than passing on zero scenarios.
- `ultragraphdb_tool_bdd` holds the traceability test. It reads every `#### Scenario:` under `openspec/specs/` and
  `openspec/changes/*/specs/`, and every `Scenario:` under `**/features/`. It fails on an archived spec's
  scenario without a feature, on a feature no spec declares, and on a duplicated scenario name; a change's
  scenarios without features are listed as pending, because all five stages' changes exist before their code.
  A stage's change is archived only when none of its scenarios is pending.
- `ultragraphdb_tool_bdd` also holds the declaration test, which checks that every crate with features declares its suite for
  both build systems. Equal scenario counts under Cargo and Bazel are compared by `scripts/bdd_counts.sh` when a
  stage closes, because a test cannot run both build systems.
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
| NFKC | `unicode-normalization` | Unicode 17.0.0, as Go's `x/text` v0.41 |
| Word boundaries | tables generated from Unicode 8.0.0 `WordBreakProperty.txt` | bleve's `blevesearch/segment` v0.9.1 is built from Unicode 8.0.0, and `unicode-segmentation` compiles in a later version that cannot be pinned |
| Collation keys, language bases | `ultragraphdb_core_text`, a port of Go's `x/text/collate` with its CLDR 23 tables and of `x/text/language`'s base inference | Go writes its own key format (`00 00` level separators), which ICU and ICU4X keys do not reproduce |
| S2 cells and covers | a port of `golang/geo`'s `s2` (Apache-2.0) and of the Go math functions it calls, in `ultragraphdb_core_types` | The `s2` crate (0.2.0) has a region coverer but no `Loop` or `ShapeIndex`, on which loop covers depend; Rust's `libm` differs from Go's math by up to 1,022 ulp |
| WKB, GeoJSON | a port of go-geom v1.6.1's geometries and its `wkb` and `geojson` encodings (BSD-2-Clause), in `ultragraphdb_core_types` | Dgraph's errors carry go-geom's and `encoding/json`'s texts (`unexpected EOF`, a type error with its path) and its JSON prints coordinates as Go prints a float64; WKB little-endian, as go-geom writes it |
| Big floats | Go's `math/big` parse, format, `Mul` and `Quo`, ported onto `dashu-int` arithmetic | See "Big floats: Go's algorithms on dashu" |

Each choice is confirmed by the golden vectors of its tokenizer or type before the tokenizer is called ported.

### Big floats: Go's algorithms on dashu

Dgraph stores and prints a bigfloat with Go's `big.Float` at 200 bits (`UnmarshalText`, `MarshalText`). Go's parse is
not correctly rounded: it keeps the decimal mantissa exact and multiplies or divides by 5^n held at 264 bits
(`math/big/floatconv.go`), which is inexact for n above about 113. Go's shortest formatting uses a symmetric half-ulp
interval, also at powers of two, where a correct round trip has a narrower interval below (`math/big/ftoa.go`,
`roundShortest`). Measured on 2026-10-20 against 2,751 Go vectors (36 hand-picked, 2,000 random up to 70 digits and
exponents of ±400, 400 exact midpoints between adjacent 200-bit values, 315 powers of two from 2^-1100 to 2^1100);
formatting is checked on the 2,746 that Go accepts, parsing on the 2,430 written as plain decimals:

| Candidate | Parse | Format |
|---|---|---|
| `dashu-float` 0.6.2 alone: correctly rounded parse, shortest round-trip search | 187 of 400 midpoints differ; random and hand-picked decimals agree | 36 of 315 powers of two differ; the other 2,431 agree |
| `astro-float` 0.9.6 alone | 644 of 2,000 random agree | 372 of 2,746 agree |
| Go's `scan`, `pow5`, `roundShortest` and `%g` ported onto `dashu` | All 2,430 decimal inputs agree | All 2,746 agree |

Go's `Mul` and `Quo` are correctly rounded single operations on exact operands; the port computes each exactly on
`dashu-int` and rounds it once to the requested precision, so replaying Go's steps reproduces Go's results, its
misroundings included. The measurement replayed them with `dashu-float`'s operations at an explicit precision
(`Context::<HalfEven>::new(p).mul`, `div`), which round the same way. `astro-float` allocates precision in whole
64-bit words (200 bits become 256), so it cannot hold Go's 200-bit values. The port is about 250 lines plus Go's
scanner syntax (underscores, base prefixes, `p` exponents, `Inf`), which the measurement did not cover. `dashu-int`
and `dashu-base` are pure Rust and build for static musl.

### GraphQL cut, its reserved names kept

`x.GqlError` and `ExportedGQLSchema` are not ported. The `dgraph.graphql.*` predicates and types stay in the
reserved and initial schema tables, because Go-written data holds them.

### The vector index registry entry precedes the index

`tok` registers the hnsw factory and the schema parser validates `@index(hnsw(...))` against its options. Stage 1
ports the registry entry with the option set; the index itself is stage 2.

## Risks / Trade-offs

- [Porting `x/text/collate` is a large table port] → Exact-with-language tokens and language sorting depend on its
  keys. Task group 2 opens with a spike that ports it for the golden strings' languages first and measures the
  tables' size.
- [Go's arithmetic differs between architectures] → Go on arm64 fuses multiply-adds and saturates out-of-range
  float-to-int conversions, amd64 does neither fusing nor saturation. The port computes without fusing and saturates;
  golden vectors are generated with fusing off. Both are Dgraph portability bugs, recorded in
  `openspec/notes/dgraph-bugs-found-in-port.md`.
- [Word-break tables generated from Unicode 8.0.0 diverge from current Unicode] → That is the point: they split as
  bleve does. The golden corpus includes text from every script bleve's tests cover.
- [Dgraph's error texts are part of the contract] → Clients and tests match on them. Error texts are copied,
  and the ported parser tests assert them.
- [A `cucumber` release changes how it embeds its manifest path] → The annotation and the build would then diverge;
  the declaration test and the first BDD suite fail at once if it does.
- [Blank-node names in JSON input carry a random component] → Differential tests normalise `_:dg.<id>.<n>` to
  `<n>`.

## Migration Plan

New crates only; nothing existing changes besides `scripts/test.sh` and the workspace manifests.

## Open Questions

None: the S2 question is settled by the port of `golang/geo`, measured against about 527,000 Go checks.
