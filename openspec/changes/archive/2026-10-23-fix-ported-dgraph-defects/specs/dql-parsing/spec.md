## ADDED Requirements

### Requirement: Every failing variable is reported
When several variables or arguments of a query fail their checks, the parse error SHALL report each failure with
Dgraph's text for it, in the order of the variables' names, joined by newlines as Go's `errors.Join` joins errors.
Go reports one of them, chosen by map iteration order (ledger 12).

#### Scenario: Two undeclared variables are both named
- **WHEN** `query test($a: int)` is parsed with the variables `$a: 3`, `$after: 0x123` and `$b: 5`
- **THEN** the error holds `Type of variable $after not specified` and `Type of variable $b not specified`, in that
  order

### Requirement: Query nesting is bounded
The parser SHALL refuse a query nested more than 1,000 levels deep in its text, with `Query nested more than 1000 levels
deep`; nesting is blocks inside blocks, parentheses in filters and math expressions, and functions inside functions. A flat chain of operators (`a or b or c …`, `x + y + z …`) adds no level, whatever its length, as in Dgraph.
Parsing, and every walk over the query's trees, SHALL use bounded stack whatever the trees' depth. Dgraph's parser recurses without a bound and ends the process once the goroutine stack reaches its limit
(ledger 32).

#### Scenario: A query nested 1,001 levels deep is refused
- **WHEN** a query whose selection blocks nest 1,001 levels deep is parsed
- **THEN** parsing fails with `Query nested more than 1000 levels deep` and nothing overflows the stack

#### Scenario: A long flat filter chain parses
- **WHEN** a filter of 5,000 `eq` functions joined by `or` is parsed, cloned and dropped
- **THEN** it parses into one filter tree and nothing overflows the stack

#### Scenario: A query nested 1,000 levels deep parses
- **WHEN** a query whose selection blocks nest 1,000 levels deep is parsed
- **THEN** it parses into a tree 1,000 levels deep

### Requirement: Edges are built from typed objects
A value edge SHALL be built from an N-Quad's object value, and an N-Quad with neither an object value nor an object id
SHALL be refused with `Unknown value type for nquad`. A predicate-deletion edge SHALL be built from the attribute and
the namespace, carrying the bytes `_STAR_ALL` as a string value with the operation DEL, the edge Dgraph's DropAttr
builds. Dgraph builds it from an N-Quad and panics when that N-Quad has no object value (ledger 13, 29).

#### Scenario: An N-Quad without an object is refused as a value edge
- **WHEN** an N-Quad with a subject and a predicate and no object becomes an edge
- **THEN** the conversion fails with `Unknown value type for nquad` and nothing panics

#### Scenario: A predicate-deletion edge carries the star value
- **WHEN** the deletion edge of predicate `name` in namespace 0 is built
- **THEN** its value is `_STAR_ALL`, its type is string and its operation is DEL
