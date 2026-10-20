## ADDED Requirements

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
