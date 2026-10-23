## Why

The port reproduces Dgraph's behaviour, defects included, and records each defect it finds in
`openspec/notes/dgraph-bugs-found-in-port.md`. That ledger now holds 31 entries with evidence. Among them: a client
query can end a Dgraph server (3), two packed set operations return wrong UIDs (1, 2, 25, 26), the term index cannot
find `İstanbul` (16), ngram indexing ignores the language its queries use (14), and the JSON mutation parser keeps the
last of several documents (22). Each entry now has a remedy: a type that makes the defect impossible to write, or a
fix whose correct behaviour the evidence settles. The user decided the remaining questions on 2026-10-23.

## What Changes

- **Types that rule defects out** (class 1): a UID-list decoder exists only once positioned (4); the next block's base
  is an `Option` (5); an N-Quad's object is an enum built once at the wire boundary, and predicate deletion takes an
  attribute instead of an N-Quad (13, 29); a facet value carries its type (20); string values are valid UTF-8, so text
  analysis takes `&str` (17, 18, 31); argument maps are ordered (12, in place).
- **Fixes with established correct behaviour** (class 2): packed intersection and difference equal the plain ones (1,
  2, 25, 26); a seek past the last UID or backwards returns the right block (3, 27); a key that ends after its
  attribute is refused (6); every split part goes with a predicate or namespace drop (7); a namespace's complete drop
  removes its schema and types durably (8); `AllocateIDs` and the bulk loader lease the named kind (9, 28); exactly
  2^63 is refused as an int (10); ngram indexing uses the value's language (14); lowercasing keeps every rune and applies
  the final-sigma rule on every path (16, 30); `"null"^^<geo:geojson>` is refused (19); the loaders' format option
  wins over the file extension (21); trailing data after a JSON document is refused (22); a list under a
  language-tagged key carries the language (23).
- **The user's decisions** (2026-10-23):
  - Q1, entry 8: the complete namespace drop is kept and fixed.
  - Q2, entry 10: float and bigfloat to datetime refuse a value outside int64 seconds, as float to int does.
  - Q3, entry 12: a parse error reports every failing variable and argument.
  - Q4, entries 17, 18 and 31: a string value holding invalid UTF-8 is refused when the mutation arrives.
  - Q5 needs no remedy: Go Dgraph's backups are not restored. Data from Go Dgraph enters through export and the
    loaders, which build every index from values, and a backup the port writes validates its string values as UTF-8.
- **Guards for what is already right**: no fused multiply-add in geometry (11); Dgraph's paired tests of its two JSON
  parsers stand as scenarios (24); `GetOptionList` needs nothing (15).
- **BREAKING** for clients comparing against Go Dgraph:
  - error texts appear where Go panics (3, 6, 19);
  - inputs Go accepts are refused: trailing JSON (22), exactly 2^63 as an int (10), out-of-range datetimes (10), and
    invalid UTF-8 strings (Q4);
  - parse errors list every failure (12);
  - index tokens differ from Go's for affected values (14, 16, 30).

  There are no outside users; the differential runs of stage 3 list these as declared deviations.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

Each delta adds requirements to a capability that one of the stage changes introduces; none is archived yet, so every
delta is `ADDED`.

- `uid-lists` (stage 1): packed operations agree with plain ones; decoders seek without panicking or hanging.
- `key-encoding` (stage 1): a key ending after its attribute is refused.
- `scalar-types` (stage 1): 2^63 and out-of-range datetimes are refused; string values are valid UTF-8.
- `tokenizers` (stage 1): ngram index tokens use the value's language; lowercasing keeps every rune and the final
  sigma.
- `dql-parsing` (stage 1): every failing variable is reported; predicate-deletion edges and value edges are built from
  typed objects.
- `mutation-input` (stage 1): null geometries, trailing JSON and the list language; the format option; the JSON number
  outcomes Dgraph's paired tests assert.
- `schema-changes` (stage 2): a namespace's data drop removes split parts.
- `query-execution` (stage 3): `after:` past a list's last UID under a filter returns nothing.
- `bulk-loader` (stage 5): namespaces are leased from the namespace counter.
- `backup-restore` (stage 5): a backup validates every string value as UTF-8 before writing it.

## Impact

- **Stage-1 crates:** `ultragraphdb_core_codec`, `_algo`, `_keys`, `_types`, `_tok`, `_facets`, `_dql` and `_chunker`.
  Public signatures change in codec (`Decoder`), dql (`to_delete_pred_edge`, value edges), facets (`to_binary`) and
  tok (the analysis functions and `AnalysisToken`).
- **Golden files** stay Go's output; the tests that read them declare each deviation by name.
- **Later stages:** stage 2 (the drops), stage 3 (`AllocateIDs`, value validation, `after:`) and stage 5 (bulk
  loader, restore) adopt the requirements added here when they are built.
- **Policy:** this change assumes the workspace's no-panic policy; a panic Dgraph raises becomes a `Result` with a clear
  error.
