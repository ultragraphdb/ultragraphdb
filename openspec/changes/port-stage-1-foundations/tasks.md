Every task group ports one Go package and runs in the same order: write the feature scenarios and port the Go
tests (they fail), port the code (they pass), refactor with both green. A group is done when its BDD suite and
its unit tests pass under Cargo and Bazel with equal counts.

## 1. Harness

- [ ] 1.1 Add `cucumber` and `futures` to `[workspace.dependencies]` and the `cucumber` `crate.annotation` (`rustc_env` `CARGO_MANIFEST_DIR = "."`) to `MODULE.bazel`, as proved in the design
- [ ] 1.2 Extend `scripts/test.sh` with the `bdd-test` run
- [ ] 1.3 Create `ugdb_test/test_bdd` with the traceability test (archived scenarios, pending scenarios, unique names) and the declaration test; scenarios: port-testing
- [ ] 1.4 Add `scripts/bdd_counts.sh`, comparing each crate's scenario counts under Cargo and Bazel
- [ ] 1.5 Create `tools/golden/` (Go module with a `replace` to `../../fork/dgraph`) and `scripts/golden.sh`; each vector file records the Dgraph commit
- [ ] 1.6 Generate `proto_pb` from `protos/pb.proto` and the Badger messages it imports, built by Cargo and Bazel as `proto_dgraph` is

## 2. Spikes for the byte-exact libraries

- [ ] 2.1 Golden vectors for exact-with-language tokens and language sort; port `x/text/collate` with its CLDR 23 tables for the golden languages and measure the tables' size
- [ ] 2.2 Golden vectors for geo tokens over `types/testdata`; compare candidate S2 implementations and record the choice
- [ ] 2.3 Golden vectors for every stemmer and stop-word list; confirm `rust-stemmers` per Snowball language and list the stemmers to port from bleve

## 3. `tok/constraints`, `tok/options`, `tok/index` → `core_tok` (interfaces)

- [ ] 3.1 Port `tok/options` tests; scenario: hnsw options are validated
- [ ] 3.2 Port the option parsing and the vector index interfaces

## 4. `x` (pure parts) → `core_keys`

- [ ] 4.1 Golden vectors for every key kind; port `keys_test.go`, `reserved_namespace_test.go`, the pure cases of `x_test.go`; scenarios: key-encoding
- [ ] 4.2 Port the key builders, `Parse`, split keys, namespaced attributes, the reserved and pre-defined tables, the reserved-namespace registry, `RemoveDuplicates`, `PredicateLang`, `ToHex`
- [ ] 4.3 Leave out `GqlError` and `ExportedGQLSchema`; keep the `dgraph.graphql.*` reserved names

## 5. `lex` → `core_lex`

- [ ] 5.1 Port `iri_test.go`; tests for the state-function lexer and item iterator
- [ ] 5.2 Port the lexer, IRI checks and quoted strings

## 6. `codec` → `core_codec`; `algo` → `core_algo`

- [ ] 6.1 Golden vectors for UidPack encodings; port `codec_test.go`; scenarios: uid-lists (packing, decoding, seeking)
- [ ] 6.2 Port group-varint, the encoder, decoder and seek
- [ ] 6.3 Port `algo` tests (packed, uidlist, heap); scenarios: uid-lists (set operations)
- [ ] 6.4 Port the sorted-list and packed-list operations

## 7. `types` → `core_types`

- [ ] 7.1 Golden vectors for every type's binary and string forms; port the conversion, value, scalar type, password, sort, geo, s2 and s2index tests; scenarios: scalar-types (all but facet values)
- [ ] 7.2 Port type identifiers, `Convert`, `Marshal`, `ParseTime`, `TypeForValue`, sorting and comparison
- [ ] 7.3 Port passwords (bcrypt), geo values (WKB, GeoJSON) and the S2 index helpers with the library chosen in 2.2
- [ ] 7.4 Port Go's big-float parse (`floatconv.scan`, `pow5`, the scanner's syntax) and shortest formatting (`ftoa.roundShortest`, `%g`) onto `dashu-float` and `dashu-int`; golden vectors include exact midpoints and powers of two; scenarios: big floats

## 8. `tok` → `core_tok`

- [ ] 8.1 Golden vectors for every tokenizer, ngram and geo tokens sorted; port `tok_test.go`, `langbase_test.go`, `stemmers_test.go`, `stopwords_test.go`; scenarios: tokenizers
- [ ] 8.2 Port the registry, identifier bytes, sortable encodings, and the term, exact, hash, sha256, int, float, bigfloat, bool, datetime and trigram tokenizers
- [ ] 8.3 Port language bases, the Unicode 8.0.0 word-break tables, stop words, stemmers and the fulltext, CJK and ngram analysis with ngram query tokens
- [ ] 8.4 Port exact-with-language and geo tokens with the collation port and the S2 library chosen in 2.1 and 2.2
- [ ] 8.5 Register the hnsw factory entry with its option and metric names and the `__vector_` suffix constants

## 9. `types/facets` → `core_facets`

- [ ] 9.1 Scenarios: scalar-types (facet values)
- [ ] 9.2 Port facet inference, encoding, term tokens and `SortAndValidate`

## 10. `dql` → `core_dql`

- [ ] 10.1 Golden corpus of parsed query trees as JSON; port `parser_test.go`, `upsert_test.go`, `state_test.go`, `dql_test.go`; scenarios: dql-parsing
- [ ] 10.2 Port the DQL lexer states and the query parser: blocks, functions, filters, directives, pagination, ordering, facets, languages, aliases
- [ ] 10.3 Port variables, typed parameters, fragments, math expressions and their checks
- [ ] 10.4 Port mutation blocks, upsert blocks and their conditions as raw text
- [ ] 10.5 Port `dql/mutation.go`: `ParseUid`, `TypeValFrom`, and N-Quad to directed-edge conversion

## 11. `schema` (parsing, state) → `core_schema`

- [ ] 11.1 Golden corpus of schema updates; port `parse_test.go` with its storage-free cases; scenarios: schema-parsing
- [ ] 11.2 Port the schema lexer and parser, directive and tokenizer checks, namespaces, type declarations and vector index specifications
- [ ] 11.3 Port the in-memory schema state and the initial schema and types per namespace, `dgraph.graphql.*` entries included

## 12. `chunker` (parsers) → `core_chunker`

- [ ] 12.1 Port `rdf_parser_test.go` (130 cases) and the storage-free cases of `json_parser_test.go`; scenarios: mutation-input
- [ ] 12.2 Port the RDF line parser, typed literals, languages, facets and the star form
- [ ] 12.3 Port the JSON parser (serde_json in place of simdjson), facets and blank nodes

## 13. Close the stage

- [ ] 13.1 No stage-1 scenario pending in the traceability test; `scripts/bdd_counts.sh` reports equal counts
- [ ] 13.2 `make test` green; `make fix` and `make format` clean
- [ ] 13.3 Every ported file carries Dgraph's copyright line, its source path and a change notice
- [ ] 13.4 Update the crate table in `AGENTS.md`
