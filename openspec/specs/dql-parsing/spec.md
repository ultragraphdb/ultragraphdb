# dql-parsing Specification

## Purpose
DQL as the port parses it: queries, variables, fragments, language lists, mutation blocks, upsert blocks, and
N-Quads converted to directed edges.
## Requirements
### Requirement: DQL queries parse into Dgraph's query tree
A DQL query SHALL parse into the same tree Dgraph's `dql` package builds: blocks, root functions, filters,
directives, pagination and ordering arguments, child predicates, aliases, language lists, facets, variables and
math expressions.

#### Scenario: Parsed queries match Dgraph's trees
- **WHEN** each query of the golden DQL corpus is parsed
- **THEN** the tree, serialized as JSON, equals the tree Dgraph's parser produced

#### Scenario: Dgraph's parser tests pass
- **WHEN** the ported cases of Dgraph's `dql/parser_test.go` run
- **THEN** every case gives Dgraph's result or Dgraph's error text

### Requirement: Variables are defined before use
A query SHALL be refused when it uses a variable no block defines, naming the variables; a variable SHALL be
usable only in the positions Dgraph allows.

#### Scenario: An undefined variable is refused
- **WHEN** a query uses `uid(aliceInWonderland)` and defines no such variable
- **THEN** parsing fails with `Some variables are used but not defined` naming `aliceInWonderland`

#### Scenario: Count of a value variable is refused
- **WHEN** a query asks for `count(val(u))`
- **THEN** parsing fails with `Count of a variable is not allowed`

#### Scenario: uid of a variable as a child is refused
- **WHEN** a query requests `uid(a)` as a child predicate
- **THEN** parsing fails with `Cannot do uid() of a variable`

### Requirement: Query parameters are typed
A query's declared parameters (`query test($a: int)`) SHALL be substituted from the request variables and
checked against their declared type.

#### Scenario: An int parameter takes an int
- **WHEN** `query test($a: int)` is parsed with `$a = "3"`
- **THEN** the parameter is the int 3

#### Scenario: An int parameter refuses a float
- **WHEN** `query test($a: int)` is parsed with `$a = "3.3"`
- **THEN** parsing fails with `Expected an int but got 3.3`

### Requirement: Language lists
A predicate SHALL accept a language list `@en:ru:hu`, the wildcard `@*`, and the untagged fallback `@.`, and
SHALL refuse two language lists on one predicate.

#### Scenario: A language list parses in order
- **WHEN** `name@en:ru:hu` is parsed
- **THEN** the predicate's languages are `en`, `ru`, `hu` in that order

#### Scenario: Two language lists are refused
- **WHEN** `name@en@ru` is parsed
- **THEN** parsing fails with `Expected directive or language list, got @ru`

### Requirement: Fragments
Fragments SHALL be expanded where they are spread, and a cycle among fragments SHALL be refused.

#### Scenario: A fragment cycle is refused
- **WHEN** fragment `a` spreads `b` and `b` spreads `a`
- **THEN** parsing fails with `Cycle detected`

### Requirement: Mutation and upsert blocks
A mutation block (`set`, `delete`) SHALL parse into its set and delete N-Quad text, unparsed, and an upsert block SHALL
parse into its query, its mutations, and each mutation's condition as written. The keyword `mutation` alone SHALL be
refused.

#### Scenario: An empty upsert block is refused
- **WHEN** `upsert {}` is parsed
- **THEN** parsing fails with `Empty mutation block`

#### Scenario: An unclosed upsert block is refused
- **WHEN** `upsert {` is parsed
- **THEN** parsing fails with `Unclosed upsert block`

#### Scenario: A condition spanning lines parses
- **WHEN** an upsert block's mutation carries `@if(eq(len(m), 1) AND gt(len(f), 0))` split across lines
- **THEN** the mutation's condition is the text `@if(...)` as written, newlines included

#### Scenario: The mutation keyword is refused
- **WHEN** `mutation { set { _:a <name> "x" . } }` is parsed as a mutation
- **THEN** parsing fails with `Unexpected token: [mutation]`

### Requirement: N-Quads become directed edges
An N-Quad SHALL convert into a directed edge with entity, attribute, value or value id, value type, language and
facets, as Dgraph's `dql/mutation.go` converts it, the operation left at its zero value, SET, for the caller to set.

#### Scenario: A value N-Quad becomes a value edge
- **WHEN** `<0x1> <name> "A"@en .` is converted
- **THEN** the edge has entity 1, attribute `name`, value bytes `A` of type default, language `en` and operation SET

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

