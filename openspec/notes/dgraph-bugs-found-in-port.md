# Dgraph bugs found while porting

The port reproduces Dgraph's behaviour, bugs included, unless a spec declares a behaviour change; step 3 of the plan
(`dgraph-port-overview.md`) fixes known bugs on the tested Rust code. This ledger holds what the port found, with the
evidence, so step 3 starts from it. Paths are in the Dgraph fork at commit `9fbf35614` (v25.5.0).

| # | Where | What happens | Called in production | The port |
|---|---|---|---|---|
| 1 | `algo/packed.go`, `IntersectSortedPacked` | The loop assigns to a shadowed `out` (`out := IntersectWithLinPacked(out, ls[i].l)`), so the result is the intersection of the two shortest lists only; with three lists where the third shares less, it returns `[2,3,9]` where the answer is `[9]` | No; tests only | Reproduces it |
| 2 | `algo/packed.go`, `DifferencePacked` | Once the last block of `v` is exhausted, it stops after `u`'s current block and drops the remaining UIDs of `u`: `2..15` minus a shorter `v` returns `2..10` | No; tests only | Reproduces it |
| 3 | `algo/packed.go`, `IntersectCompressedWith` | Panics with an index out of range when `afterUID` lies past the pack's last UID and the binary strategy is chosen: `SeekToBlock` indexes `Blocks[len(Blocks)]` | Yes | Reproduces the panic |
| 4 | `codec/codec.go`, `Decoder.SeekToBlock` | Panics on a decoder never sought when the target lies in the first block | Via 3 | Reproduces the panic |
| 5 | `codec/codec.go`, `Decoder.LinearSeek` | Never returns for a target of `math.MaxUint64` | Unclear | Behaviour change: stops past the last block and returns nothing |
| 6 | `x/keys.go`, `Parse` | Panics with an index out of range on a key ending right after its attribute that is not a schema or type key | Yes, on corrupt keys | Reproduces the panic |
| 7 | `posting/index.go`, `DeletePredicate`, `DeleteData` | Drop only the default `0x00` prefix and leave split-list parts (`0x04`) behind | Yes | Stage 2 declares a behaviour change: removes them |
| 8 | `schema/schema.go`, `DeleteAllForNs` | Deletes a namespace's schema in memory only; the schema and type keys survive and return at restart | Yes | Stage 2 declares a behaviour change |
| 9 | `edgraph/zero.go`, `AllocateIDs` | Leases UIDs whatever lease type is asked (`worker/mutation.go:608` overwrites the type) | Yes | Stage 3 declares a behaviour change |
| 10 | `types/conversion.go`, float to int and to datetime | Go converts a float outside int64 as its platform does: amd64 yields `MinInt64`, arm64 saturates; `9223372036854775808.0` passes the range check and converts to `MinInt64` on amd64 and `MaxInt64` on arm64, so one Dgraph version stores different values on the two architectures | Yes | Saturates, as arm64 and Rust's `as` do |
| 11 | `types/s2*.go` through `golang/geo` | Go on arm64 fuses multiply-adds, which changes the last bit of S2 vertices; geo index tokens could therefore differ between amd64 and arm64 nodes. No covering or token differed in the vectors tested | Yes | Computes without fusing, as amd64 Go does; golden vectors are generated with fusing off (`scripts/golden.sh`) |
| 12 | `dql/parser.go` | Parse outcomes depend on Go's map iteration order: of 2,346 golden inputs generated 41 times, 293 gave different outcomes between runs (which error is reported, or which of several failing arguments is named) | Yes | Deterministic: arguments sit in an ordered map, so the same input always reports the same error |
| 13 | `dql/mutation.go`, N-Quad conversion | An N-Quad with neither an object id nor an object value dereferences nil and panics | Yes, on malformed input | Treats the missing value as an empty string |
| 14 | `tok/tokens.go`, `GetTokenizerForLang` | Has no case for the ngram tokenizer, so ngram indexing always uses English stop words and stemming while `GetNGramTokens` and `GetNGramQueryTokens` use the requested language: a query on a non-English ngram index can build tokens the index never stored | Yes | Reproduces it |
| 15 | `tok/options/allowed_options.go`, `GetOptionList` | The loop never advances its index: the result holds one name and empty strings | No; nothing calls it | Returns the names, sorted |
| 16 | bleve v2.5.7, lowercase filter | Moves only the runes it changes: after `İ` (2 bytes) lowercases to `i` (1 byte), later runes keep stale bytes and the term is cut (`İб` becomes `69 B0 D0`) | Yes, in term, fulltext and ngram tokens | Reproduces it, so tokens equal Go's |
| 17 | bleve v2.5.7, `cjk_bigram` filter | Reads past the end of a term holding invalid UTF-8 into the underlying buffer, and would panic past the buffer | Yes | Reproduces the read and stops at the buffer's end instead of panicking |
| 18 | blevesearch/segment v0.9.1 | An invalid byte after a Han, Kana, Hangul or non-word segment ends segmentation and drops the rest of the input | Yes | Reproduces it |
| 19 | `chunker/rdf_parser.go` through `types.toBinary` | `<a> <p> "null"^^<geo:geojson> .` panics with `interface conversion: interface {} is *[]uint8, not []uint8` | Yes, on RDF mutation input | Refuses the line with `invalid ObjectValue` |
| 20 | `types/facets/utils.go`, `ToBinary` | Asserts the marshalled value is `[]byte` without checking, so a value whose Go type does not match the facet type panics | Unclear; the JSON parser passes inferred types | Returns an error |
| 21 | `chunker/chunk.go`, `DataFormat` | Its comment says the file extension has precedence; the code lets the format option `rdf` win over a `.json` name | Yes, in the loaders | Follows the code |
| 22 | `chunker/json_parser.go`, `ParseJSON` | Input holding several top-level JSON values keeps only the last: `[{"name":"a"}] [{"name":"b"}]` yields `b` alone | Yes | Reproduces it |
| 23 | `chunker/json_parser.go`, list values | A list under a language-tagged key keeps the tag in the predicate: `"tags@en": ["a"]` gives predicate `tags@en` with an empty language | Yes | Reproduces it |
| 24 | `chunker/json_parser.go`, `chunker.ParseJSON` | Parses with simdjson where `simdjson.SupportedCPU()` holds and with `encoding/json` elsewhere, arm64 included; by the code, the two differ on delete input `0.0` (simdjson reads `*`) and on integers above `MaxInt64` (simdjson: unexpected type) | Yes | Follows `encoding/json`; the golden vectors come from arm64 |

