## ADDED Requirements

### Requirement: Every failing variable is reported
When several variables or arguments of a query fail their checks, the parse error SHALL report each failure with
Dgraph's text for it, in the order of the variables' names, joined by newlines as Go's `errors.Join` joins errors.
Go reports one of them, chosen by map iteration order (ledger 12).

#### Scenario: Two undeclared variables are both named
- **WHEN** `query test($a: int)` is parsed with the variables `$a: 3`, `$after: 0x123` and `$b: 5`
- **THEN** the error holds `Type of variable $after not specified` and `Type of variable $b not specified`, in that
  order

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
