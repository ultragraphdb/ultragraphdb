## ADDED Requirements

### Requirement: Results equal Go Dgraph's
For the same data, schema and DQL, a query SHALL return the same result as Go Dgraph v25.5.0, after normalising
assigned uids and the latency fields; vector cases SHALL use data small enough that the index search is exact.

#### Scenario: The ported query suites agree with Go Dgraph
- **WHEN** the ported cases of `query/query0_test.go` to `query4_test.go`, `query_facets_test.go`,
  `rdf_result_test.go` and the vector and ngram suites run against ultragraphdb and against Go Dgraph on the same
  data
- **THEN** every case gives the same normalised result or the same error text

### Requirement: Root functions and filters
Every DQL function SHALL behave as in Dgraph, at the root and in `@filter` with `and`, `or` and `not`: `eq`, `le`, `ge`,
`lt`, `gt`, `between`, `anyofterms`, `allofterms`, `anyoftext`, `alloftext`, `anyof`, `allof`, `regexp`, `match`,
`ngram`, `near`, `within`, `contains`, `intersects`, `has`, `uid`, `uid_in`, `type` and `similar_to`. The comparisons and
`regexp` SHALL run in a filter on an unindexed predicate; every other indexed function SHALL be refused without its
index, at the root and in a filter, with Dgraph's errors. `checkpwd` SHALL be a selection on a password predicate.

#### Scenario: eq at the root of an unindexed predicate is refused
- **WHEN** `{ q(func: eq(nick, "x")) { uid } }` runs and `nick` has no index
- **THEN** the query fails with `Predicate nick is not indexed`

#### Scenario: eq in a filter works without an index
- **WHEN** `{ q(func: has(name)) @filter(eq(nick, "x")) { uid } }` runs and `nick` has no index
- **THEN** it returns the nodes whose `nick` is `x`

#### Scenario: regexp at the root uses the trigram index
- **WHEN** `regexp(name, /^Al.*/)` runs at the root on a predicate indexed by trigram
- **THEN** it returns the names matching the expression

#### Scenario: A term function in a filter needs its index
- **WHEN** `@filter(anyofterms(bio, "rust"))` runs and `bio` has no term index
- **THEN** the query fails with Dgraph's error that the predicate is not indexed

#### Scenario: Geo functions answer over the geo index
- **WHEN** `near`, `within`, `contains` and `intersects` run over the golden polygons
- **THEN** each returns Dgraph's result

#### Scenario: similar_to returns the nearest vectors
- **WHEN** `similar_to(emb, 3, $v)` runs on an hnsw-indexed predicate
- **THEN** it returns the 3 nearest nodes

#### Scenario: checkpwd verifies a password
- **WHEN** `{ q(func: uid(0x1)) { checkpwd(password, "secret1") } }` runs and the node's password is `secret1`
- **THEN** it returns `"checkpwd(password)": true`, and false for any other string

### Requirement: Pagination and ordering
`first`, `offset` and `after` SHALL paginate as in Dgraph, a negative `first` taking from the end, ignoring `offset`, and
being refused with sorting; `orderasc` and `orderdesc` SHALL sort by one or more keys, with nodes lacking the key last,
and by language when tagged.

#### Scenario: A negative first takes from the end
- **WHEN** a block with 10 results asks for `first: -3`
- **THEN** it returns the last 3

#### Scenario: A negative first with sorting is refused
- **WHEN** a block asks for `first: -3` and `orderasc: name`
- **THEN** it fails with `We do not yet support negative or infinite count with sorting`

#### Scenario: Multi-key sorting puts missing values last
- **WHEN** nodes are sorted by `orderasc: age, orderdesc: name` and some lack `age`
- **THEN** the order follows `age`, then `name` descending, with the nodes lacking `age` last

### Requirement: Variables, aggregation and math
Query and value variables SHALL carry uids and values between blocks; `min`, `max`, `sum`, `avg` and `count` SHALL
aggregate; `math()` SHALL compute over value variables, as Dgraph does.

#### Scenario: A value variable aggregates across blocks
- **WHEN** one block collects `a as age` and another asks `sum(val(a))`
- **THEN** the result is the sum of the ages

#### Scenario: math computes over variables
- **WHEN** a block asks `math(ceil(a / 2))` for a value variable `a`
- **THEN** each node's result equals Dgraph's

### Requirement: Graph operations
Graph operations SHALL produce Dgraph's results: `@groupby`, `@recurse` with `depth` and `loop`, `shortest`
with `numpaths` and facet weights, `@cascade` with fields, `@normalize`, `expand(_all_)` and expansion by type.

#### Scenario: groupby counts per group
- **WHEN** friends are grouped by `age` with `count(uid)`
- **THEN** each group lists its age and its count

#### Scenario: recurse stops at its depth
- **WHEN** `@recurse(depth: 2, loop: false)` follows `friend`
- **THEN** the result holds two levels and no repeated node

#### Scenario: k shortest paths follow facet weights
- **WHEN** `shortest(from: 0x1, to: 0x5, numpaths: 2)` follows `road @facets(weight)`
- **THEN** it returns the two lightest paths in order of weight

#### Scenario: cascade drops incomplete nodes
- **WHEN** `@cascade` applies to a block requesting `name` and `friend`
- **THEN** nodes lacking either are absent

#### Scenario: expand all follows the node's types
- **WHEN** `expand(_all_)` is asked of a node of type `Person`
- **THEN** the result holds every predicate of `Person` the node has

### Requirement: Facets, languages and counts
Facets SHALL be retrieved, filtered, ordered and assigned to variables; language lists SHALL select values in order,
`@.` selecting the untagged value, else any tagged one; `count(predicate)` and the count index SHALL answer as Dgraph
does.

#### Scenario: A language list falls back in order
- **WHEN** `name@fr:en` is asked of a node holding `name@en` only
- **THEN** the English value is returned

#### Scenario: Facets filter edges
- **WHEN** `friend @facets(eq(close, true))` is asked
- **THEN** only edges whose `close` facet is true are followed

#### Scenario: A count index answers count at the root
- **WHEN** `{ q(func: eq(count(friend), 3)) { uid } }` runs on a counted predicate
- **THEN** it returns the nodes with exactly three friends

### Requirement: Schema queries
`schema {}` and `schema(pred: [...]) { ... }` SHALL return the schema as Dgraph does, filtered by the caller's
permissions.

#### Scenario: A schema query returns the requested fields
- **WHEN** `schema(pred: [name]) { type index }` runs
- **THEN** the result lists `name` with its type and index flag only

### Requirement: Pagination past a list's end returns nothing
A filtered read with `after:` past the last UID of a list SHALL return no UIDs from that list. Dgraph ends the Alpha
process on it, so the differential runs against Go Dgraph exclude it (ledger 3).

#### Scenario: after past the last UID under a filter returns nothing
- **WHEN** a query's filter intersects a rolled-up list and `after:` lies past the list's last UID
- **THEN** the query returns no UIDs from that list and the server keeps serving
