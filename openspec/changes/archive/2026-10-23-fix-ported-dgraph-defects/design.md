## Context

`openspec/notes/dgraph-bugs-found-in-port.md` lists 31 Dgraph defects the port found, each confirmed in the Go source
at commit `9fbf35614` (v25.5.0), most by running Go against the fork. Stage 1 reproduces them, so its crates equal Go
byte for byte, panics included. Step 3 of the plan fixes them on the tested Rust code, and this change is that step for
every entry. Entries 7, 8, 9 and 28 concern stages not yet built, so for them this change adds requirements and the
design those stages follow.

The remedies fall in two classes:

- **Class 1:** a Rust type makes the defect impossible to write.
- **Class 2:** the correct behaviour is established from evidence: Go's documentation of the function, its tests, its
  other code path doing it right, or the user-facing help.

The user decided the five questions neither class settled (2026-10-23). A separate workspace policy makes every panic a
`Result`; this change relies on it and does not restate it.

## Goals / Non-Goals

**Goals:**

- Every ledger entry fixed or guarded, with a scenario wherever clients can see the change.
- Golden files stay a record of Go's behaviour; the Rust tests declare each deviation from them by case name.

**Non-Goals:**

- Re-architecting any package beyond the types named here.
- Reading Go-written stores or backups: data from Go Dgraph enters through export and the loaders, which build every
  index from values.

## Decisions

### Class 1: types that rule the defect out

- **Decoders exist only once positioned (4).** `Decoder::unpositioned` goes; `Decoder::sought(pack, uid, whence)`
  builds and seeks in one step, and `Decoder::new` is `sought(pack, 0, Start)`. Every value of the type holds the
  unpack of its current block, so `seek_to_block` on an unsought decoder cannot be written. Alternative: a runtime
  check in `seek_to_block`, which leaves the invalid state constructible.
- **The next block's base is an `Option` (5).** `peek_next_base` returns `Option<u64>`; `linear_seek` loops
  `while let Some(next) = … && seek >= next`. The `u64::MAX` sentinel collided with a valid target, and with the option
  the loop ends because each step moves to a block that exists.
- **An N-Quad's object is an enum built once (13, 29).** `enum Object<'a> { Uid(&'a str), Value(&'a api::Value) }` comes
  from one fallible `NQuad::object()`, following `x.ValueType`'s order. Value edges take the `&api::Value`. The
  predicate-deletion edge becomes the infallible `delete_predicate_edge(attr, namespace)`, so no N-Quad, and no missing
  value, is involved. The wire type `api::NQuad` keeps both optional fields, because `api.proto` is served unchanged.
- **A facet value carries its type (20).** `enum FacetValue { Int(i64), Float(f64), Bool(bool), DateTime(DateTime),
  Str(String) }` replaces the `(Value, ValType)` pair. `to_binary` encodes each variant directly and keeps a `Result`
  only for a datetime's zone offset. A facet string that a `\x` or octal escape makes invalid UTF-8 is refused when the
  facet is parsed (`Facet value of <key> is not valid UTF-8`), as the user decided on 2026-10-23.
- **String values are valid UTF-8, so analysis takes `&str` (17, 18, 31, with Q4).** Converting to the string or default
  type refuses invalid UTF-8 (`Value is not valid UTF-8 for type string`), which makes the string value variant a
  `String`. The text-analysis pipeline then takes `&str`:
  - `analyze`, `word_segments`, `to_lower`, `nfkc`, the stemmers and `cjk_bigram`;
  - `AnalysisToken` holds a `String` and loses its `tail` into the rest of the input.

  An over-read past a term, a segment ending on an invalid byte, and a truncated lead absorbing the next word cannot
  then be written. Go's RDF, DQL and JSON parsers already replace invalid bytes; the only producer left was the raw
  binary-to-string conversion, which now refuses.

  Alternative (the user declined it under Q4): tokenize the U+FFFD-replaced text and store the bytes unchanged.
- **Argument maps are ordered (12).** This is already in place: `BTreeMap` for variables and arguments.

### Class 2: fixes from established behaviour

| # | Fix | Evidence |
|---|---|---|
| 1 | Fold every further list into the intersection | `IntersectSorted` does; the doc comment; the early `break` reads the accumulator |
| 2, 25, 26 | Decode both packs, call `difference`, re-encode with `u`'s block size | The plain `Difference`; Dgraph's packed tests assert the plain results |
| 3 | `blocks.get(prev)` in `seek_to_block`: an exhausted decoder searches the whole pack and returns empty | The linear strategy and the in-memory path return empty for the same input |
| 27 | Unpack whenever the target block differs from the block held | `codec.go:229`, "we need to search the entire pack" |
| 4, empty block | `uids.last()` in place of `uids[len-1]`: an empty block contributes nothing | Same as a block whose UIDs all lie below the target |
| 6 | Return `InvalidFormat` when the type byte is missing | Every other truncation in `Parse` returns it |
| 7 | Drop the `0x04` twin of every `0x00` prefix a drop removes | Dgraph's index, count and reverse drops do |
| 8 (Q1) | Drop `[0x00,ns]`, `[0x04,ns]`, `[0x01,ns]`, `[0x02,ns]` in one durable step, then write the namespace's initial schema at ts 1 | `draft.go:358`; single deletions persist (`schema.go:89-131`) |
| 9 | `enum LeaseKind { Uid, Namespace, Timestamp }`, set once at the API edge and routed by an exhaustive match; timestamps go through Zero's `Timestamps` | `hooks/config.go:21-22`; Zero's `/assign` routes each kind |
| 10 (Q2) | The guard becomes `vc >= -2^63 && vc < 2^63` for int, and applies to float and bigfloat seconds for datetime | The error text; the scalar-types spec; the user's Q2 |
| 14 | `get_tokenizer_for_lang` gives ngram its language; the `_ =>` arm becomes an explicit list | `NGramTokenizer.lang`; the query path; the fulltext case |
| 16, 30 | The unchanged branch moves each rune to the write position; the final-sigma rule applies on the widening path too | The function's comment and its test (`ὈΔΥΣΣ` → `ὀδυσς`) |
| 19 | Keep the refusal `invalid ObjectValue`; one `marshal_binary(&Val) -> Result<Vec<u8>, TypesError>` in core_types removes the empty-bytes fallbacks in types, facets and dql | `ValidateAndConvert` refuses the same value; RFC 7946 §3.2 |
| 21 | `rdf` or `json` as the option wins; otherwise the extension decides | Both loaders' `--format` help |
| 22 | Refuse anything but whitespace after the first JSON value; drop the re-decode | `json.Unmarshal`'s `checkValid`; simdjson refuses it too |
| 23 | Split `@lang` off the predicate for each list item | Every other branch of `mapToNquads` does |
| 28 | The bulk loader leases namespaces from the namespace counter | Entry 9's routing |

### The user's decisions, made concrete

- **Q3, every failure (12).** Each failing variable or argument contributes Dgraph's text for it, in key order, joined by
  newlines as Go's `errors.Join` joins errors. Key order is the order the ordered map already gives; source order would
  need an insertion-ordered map, and client variables have no source position.
- **Nesting depth (ledger 32).** The user set the limit at 1,000 levels of nesting written in the query's text: blocks
  inside blocks, parentheses in filters and math, functions inside functions. A flat operator chain adds no level, as
  Dgraph accepts it, so a filter tree can still be as deep as its chain is long; every walk over the trees (parsing,
  `Clone`, `Drop`, `PartialEq`, `Debug`, `debug_string`, fragment expansion, variable substitution and collection)
  therefore uses an explicit stack.
- **Q5, no Go index keys enter.** Go Dgraph's backups are not restored, so no index key Go wrote reaches the port:
  export and the loaders build every index from values, with the port's tokens. A backup the port writes checks each
  string value for valid UTF-8 before writing it, so a restore reads only what the store accepts.

### Guards for what is right already

- **No fused multiply-add (11).** A workspace `clippy.toml` bans `f64::mul_add` and `f32::mul_add`. One golden S2
  vector whose fused and unfused results differ fails a regression.
- **Go's possible outcomes for map order (12).** The DQL generator records the set of distinct outcomes over its 41
  runs, and the test checks that the port's outcome is in the set. The 293 unstable cases are then compared, not
  skipped.
- **The JSON number outcomes Dgraph's paired tests assert (24)** become scenarios. The amd64 simdjson divergences are
  listed for the differential runs.
- **`GetOptionList` (15)** already returns every name.

### Golden files and deviations

Generators keep recording Go. A test that reads a golden file and expects the port to differ names the cases in a
`DEVIATIONS` table: the case, the ledger entry, and the expected port result. That way a deviation is visible in
review and cannot hide behind a blanket "Go panicked, accept any error".

## Risks / Trade-offs

- **[Refusing invalid UTF-8 strings refuses input Go Dgraph accepts]** → Only gRPC `bytes_val` on a string predicate and
  `\x` escapes in typed string literals produce it; the refusal names the reason. An export holding such values fails
  in the loader at that value.
- **[Index tokens differ from Go Dgraph's for affected values (14, 16, 30)]** → Every index the port holds was built
  by the port: exports and the loaders index from values, and Go backups are not restored. The differential runs
  declare the deviation.
- **[A namespace drop followed by the ts-1 initial schema could be shadowed by a deletion version (8)]** → The drop
  must remove every version physically. The stage-2 kv-store requirement "remove every key under it durably" is the
  test for it.
- **[A rollup that took its timestamp before a drop could write keys back after it (7, 8)]** → Unverified until stage 2
  exists. Stage 2 pauses rollups of a predicate or namespace across its drop, as it pauses them across an index rebuild.
- **[The analysis signatures change across tok (17)]** → Behaviour on valid UTF-8 is unchanged by construction, and the
  existing golden suite over valid input checks it.

## Migration Plan

There are no deployed ports and no outside users. The stage-1 crates change in place. Stages 2, 3 and 5 build to the
requirements added here. The differential runs of stage 3 read their exclusion list from the `DEVIATIONS` tables.

## Open Questions

None.
