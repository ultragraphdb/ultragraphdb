## Why

Every later stage of the Dgraph port speaks in the vocabulary of Dgraph's pure packages: its key layout, its
packed UID lists, its scalar types and their encodings, its tokenizers, and its DQL, schema and mutation
parsers. These packages read no storage and open no socket, so they can be ported and proven first, against
Dgraph's own tests and against byte-level vectors taken from the Go source, before anything depends on them.
Stage 1 also sets up the test harness every stage uses: BDD scenarios from these specs, written before the
code, and TDD for each ported Go file.

## What Changes

- A BDD harness: every spec scenario runs as a Gherkin scenario of the same name under `cucumber`, in Cargo and
  Bazel alike; a traceability check fails when an archived spec's scenario has no feature and lists a change's
  scenarios without features as pending.
- Golden vectors: a Go program built against the Dgraph source writes the byte-exact outputs of keys, packed
  UIDs, value encodings and tokens, and the Rust tests read them.
- Dgraph's internal protobuf messages (`protos/pb.proto`) generated as `ultragraphdb_proto_pb`, beside the existing
  `ultragraphdb_proto_dgraph`.
- The pure packages ported, in import order: `tok/constraints`, `tok/options`, `tok/index`, the pure parts of
  `x`, `lex`, `codec`, `algo`, `types`, `tok`, `types/facets`, `dql`, `schema` (parsing and in-memory state), and
  `chunker` (RDF and JSON parsing).
- GraphQL-only code in these packages is left out (`x.GqlError`, `ExportedGQLSchema`). The reserved
  `dgraph.graphql.*` predicates and types stay, because data written by Go Dgraph holds them.

## Capabilities

### New Capabilities

- `port-testing`: BDD scenarios traced to specs, TDD for ported Go tests, golden vectors from the Go source,
  in both build systems.
- `key-encoding`: the byte layout of data, index, reverse, count, schema, type and split keys, namespaced
  attribute names, and the reserved and pre-defined predicates.
- `uid-lists`: packed UID lists (UidPack, group-varint blocks) and the sorted-set operations over them.
- `scalar-types`: Dgraph's scalar type identifiers, conversions, binary encodings, value inference, ordering,
  passwords, and facet values.
- `tokenizers`: the index tokenizers, their identifier bytes and sortable encodings, language handling,
  stemming, stop words and the tokenizer registry.
- `dql-parsing`: DQL queries, variables, fragments, language lists, mutation blocks, upsert blocks, and N-Quads
  converted to directed edges.
- `schema-parsing`: the schema language (predicates, types, directives, index options) and the in-memory
  schema state.
- `mutation-input`: RDF and JSON mutation input parsed into N-Quads.

### Modified Capabilities

None; `openspec/specs/` holds no specs yet.

## Impact

- New crates `ultragraphdb_core_*`, `ultragraphdb_proto_pb` and `ultragraphdb_test_bdd`; new test-only dependencies `cucumber` and its
  runtime, and libraries for the byte-exact formats (BLAKE2b, bcrypt, Snowball stemmers, S2, WKB).
- `scripts/test.sh` runs the `bdd-test` tag beside `doc-test` and `unit-test`.
- A Go toolchain is needed to regenerate golden vectors, not to build or test.
- Ported files carry Dgraph's copyright line, their source path and a change notice, as `AGENTS.md` requires.
