## ADDED Requirements

### Requirement: JSON output
Results SHALL encode as Dgraph's JSON: block names as keys, lists of nodes, uids in hexadecimal, values in their types'
JSON forms, facets under `predicate|facet`; over gRPC `txn`, `latency` and `metrics` SHALL be response fields, and over
HTTP the body SHALL be the `data` and `extensions` envelope.

#### Scenario: JSON matches Go Dgraph byte for byte
- **WHEN** the ported query suites run with JSON output
- **THEN** each `json` payload equals Go Dgraph's after normalising uids

#### Scenario: Facets appear beside their predicate
- **WHEN** `friend @facets(close)` is asked
- **THEN** each edge's `close` appears as `friend|close`

### Requirement: RDF output
With `resp_format` RDF, results SHALL encode as Dgraph's N-Quads, and queries using groupby, uid counts, normalize,
ignorereflex, checkpwd, facets or geo values SHALL be refused, as Dgraph refuses them.

#### Scenario: RDF output matches Go Dgraph
- **WHEN** the ported `rdf_result_test.go` cases run
- **THEN** each `rdf` payload equals Go Dgraph's after normalising uids

#### Scenario: RDF output refuses facets
- **WHEN** a query requesting facets asks for RDF output
- **THEN** it is refused with Dgraph's error

### Requirement: Metrics and debug output
The response SHALL report `num_uids` per predicate and `_total`, the latency phases, and with `debug` the uids of
returned nodes.

#### Scenario: num_uids counts the uids touched
- **WHEN** a query touches 5 uids of `name`
- **THEN** `metrics.num_uids` reports 5 for `name` and a `_total`

### Requirement: Output limits
Results SHALL be bounded by Dgraph's limits, with its errors: `@normalize` by `normalize-node` (10,000 by default).

#### Scenario: A normalize result beyond the limit is refused
- **WHEN** a `@normalize` query would produce more rows than the configured limit
- **THEN** it fails with `Couldn't evaluate @normalize directive - too many results`
