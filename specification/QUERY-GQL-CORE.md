# The GQL core

This file specifies the subset of ISO/IEC 39075:2024, GQL, that UltraGraphDB's query core executes: the statements,
patterns and expressions it accepts, their meaning over the graph a UGS schema declares, how they read and write
through the engine's transactions, what they return, and how a query outside the core is refused. The core is the
fragment the DeepBrain MVP needs: a schema is created, data is written, read, updated and deleted, and every common
operation of DeepBrain's Gen5 schema is expressed in it (section 13). Everything of GQL outside it, `OPTIONAL MATCH`,
path variables and path modes, shortest paths, set operations, `OTHERWISE`, `LET`, `FOR`, graph type statements, is the
full-conformance stage's, and a query using any of it is refused naming the feature (section 11).

`QUERY-SPARQL-CORE.md` is the sibling: the two cores read one store, and a question asked in both answers alike
(section 12). `SCHEMA.md` gives the graph the core sees; `PROTOCOL.md` the bytes a statement, its parameters and its
result travel in; `openspec/notes/UltraGraphDB Dual Query Engine Plan.md`, "The query core", the design. The read
fragment's reference semantics is MGQL (Thimmaiah, Lin and Gligoric, OOPSLA 2026; `2608.24565v1.pdf` beside this
file), a mechanised, executable semantics of GQL whose evaluator this core's results are checked against.

## Contents

1. Conventions
2. The graph a query sees
3. Lexical structure
4. Grammar of the core
5. Values, types and null
6. Patterns and their matching
7. Statements and their semantics
8. Writes
9. Procedures: vector and text search
10. Transactions, consistency and the protocol
11. Diagnostics and the feature register
12. Conformance: the oracles
13. DeepBrain in the core
14. What this specification asks of the others
15. Decisions

## 1. Conventions

| Term | Meaning |
|---|---|
| ISO | ISO/IEC 39075:2024, first edition; a section cited as `ISO §n` |
| MGQL | The mechanised semantics of the read fragment: its calculus (paper figure 2), evaluator (`Semantics.lean`) and 350 executable assertions, the oracle of section 12 |
| Tenant, namespace, graph | As `SCHEMA.md` section 1: a tenant isolates; a namespace is an installed schema with its graphs; a graph is what a query is evaluated against |
| Binding table | A bag of records, each a finite map from variable names to values, as ISO §4.15 and MGQL definition 5.1 |
| Live | An instance that is not deleted and, on a `@bitemporal` type, not retracted (`SCHEMA.md` section 8.4) |
| Core | The statements, clauses, patterns and functions this file admits. A query is in the core when every construct it uses is; otherwise it is refused (section 11), never evaluated in part |
| SHALL, refused | A rule the implementation holds; a refusal is a diagnostic of section 11 and no part of the statement runs |

Keywords are written in upper case here and are case-insensitive in a query. A grammar in this file is EBNF as
`SCHEMA.md` section 4 uses it. Where ISO leaves a choice to the implementation, this file states the choice and marks
it **[choice]**; where this file narrows ISO, it marks the narrowing **[core]** and section 11 names the feature a
query outside it is refused with.

## 2. The graph a query sees

A query runs against one **working graph** of one tenant: the graph named by `USE`, else the tenant's `@default`
graph (`SCHEMA.md` section 6.11); a tenant without one and a query without `USE` is refused (`GQL0201`). The working
graph is a domain graph exposed together with the shared graphs its schema imports, as `SCHEMA.md` section 11.3
states, so a bridge is one edge like any other. A query names no other tenant, and a graph of another tenant does not
exist to it. The transaction fixes the tenant (section 10).

The working graph is the GQL view `SCHEMA.md` section 11.3 derives from the schema, with no store state visible that
the view does not name:

| In the view | From the schema |
|---|---|
| Node, with labels | An entity, or a node-encoded relation instance; its labels are its concrete type and every supertype, statement types included; `rdf:type` is never a property |
| Edge, with a type | An edge-encoded relation instance: its type is the navigation name declared on the relation's anchor role, from the anchor to the other participant. A `@symmetric` relation's edge is undirected; every other edge is directed |
| Edge, with a role type | A role edge of a node-encoded relation instance: from the relation node to the participant, typed `{Relation}_{role}`, and for a specialized role also the super role's type |
| **Navigation edge** | For a node-encoded relation, a derived, directed edge typed by a navigation name of the relation, from the participant that gains the name to the participant of the other role the name reaches; `(u)-[:parents]->(p)` stands for `(u)<-[:SubOrganizationOf_unit]-(:SubOrganizationOf)-[:SubOrganizationOf_parent]->(p)`. It is read-only, has no properties and no element id, and exists so that a walk through a relation node is one hop, as it is through an edge-encoded relation |
| Property | An attribute, under its name; an absent optional attribute is `NULL`; an interval attribute is a record with the fields `start`, `end`, `start_precision`, `end_precision`; a bitemporal instance has the read-only properties `asserted`, a record as an interval's, and `live` |
| Element id | The instance's IRI (`SCHEMA.md` section 7.3), returned by `ELEMENT_ID` as a string; an edge's is its reifier IRI, a navigation edge has none |

A label, edge type or property a query names must be declared on the working graph's schema (`GQL0202`, `GQL0203`,
`GQL0204`): the graph is closed, as MGQL's closed graphs are, and the type system of section 5 refines every variable
by the schema. Undeclared data under `@open` is reachable by its label `Resource` and property names as strings.

## 3. Lexical structure

ISO §21 as narrowed here. Source is UTF-8, at most 1 MiB (`GQL0001`), nesting of parentheses, brackets and braces
bounded at 64 (`GQL0001`).

| Token | Form |
|---|---|
| Identifier | A regular identifier, `[A-Za-z_][A-Za-z0-9_]*`, case-sensitive, at most 128 bytes; or a delimited identifier in backquotes, `` `member of` ``, any text with `` ` `` doubled |
| Keyword | Case-insensitive; a reserved word is never a regular identifier (`GQL0002`); a delimited identifier may spell one |
| Parameter | `$` and an identifier, typed by the prepared statement (section 10) |
| Integer | Decimal digits with optional `_` between digits; refused when it does not fit a 64-bit signed integer (`GQL0003`) |
| Float | Digits, `.`, digits, with an optional exponent; `INF` and `NAN` are refused |
| String | `'…'` or `"…"`, the quote doubled inside, escapes `\\`, `\'`, `\"`, `\n`, `\t`, `\r`, `\u{XXXX}`; any other `\` refused (`GQL0003`) |
| Boolean | `TRUE`, `FALSE`; `UNKNOWN` is the null of the Boolean type and may be written |
| Null | `NULL` |
| Temporal | `DATE 'YYYY-MM-DD'`, `ZONED DATETIME 'YYYY-MM-DDThh:mm:ss[.f](Z\|±hh:mm)'`, `LOCAL DATETIME 'YYYY-MM-DDThh:mm:ss[.f]'`, `ZONED TIME 'hh:mm:ss[.f](Z\|±hh:mm)'`, `DURATION 'P…'` as ISO 8601; a literal naming no instant is refused (`GQL0003`) |
| Comment | `//` and `--` to the end of the line; `/* … */` |

Reserved words of the core: `AND`, `AS`, `ASC`, `ASCENDING`, `BY`, `CALL`, `COUNT`, `DELETE`, `DESC`, `DESCENDING`,
`DETACH`, `DISTINCT`, `FALSE`, `FILTER`, `GROUP`, `IN`, `INSERT`, `IS`, `LIMIT`, `MATCH`, `NEXT`, `NODETACH`, `NOT`,
`NULL`, `NULLS`, `OFFSET`, `OR`, `ORDER`, `REMOVE`, `RETURN`, `SET`, `SKIP`, `TRUE`, `UNKNOWN`, `USE`, `WHERE`, `YIELD`,
and the temporal and type words `DATE`, `DURATION`, `LOCAL`, `ZONED`, `DATETIME`, `TIME`. Every other ISO reserved
word is reserved too, so that a query of the full language never parses differently in the core (`GQL0002` names
the word and, where it begins a feature outside the core, section 11's refusal applies instead).

## 4. Grammar of the core

```
request        = [ "USE" graph_name ] linear_query ;
graph_name     = identifier [ "." identifier ] ;                 (* namespace-qualified when ambiguous *)
linear_query   = statement { "NEXT" statement } ;
statement      = { reading } [ writing ] [ return ]
               | call_stmt { reading } [ writing ] [ return ] ;
reading        = match | filter ;
match          = "MATCH" path_pattern { "," path_pattern } [ "WHERE" expr ] ;
filter         = "FILTER" expr ;
writing        = insert | set | remove | delete ;
insert         = "INSERT" insert_pattern { "," insert_pattern } ;
set            = "SET" set_item { "," set_item } ;
set_item       = variable "." property "=" expr ;
remove         = "REMOVE" variable "." property { "," variable "." property } ;
delete         = [ "DETACH" | "NODETACH" ] "DELETE" variable { "," variable } ;
return         = "RETURN" [ "DISTINCT" ] ( "*" | projection { "," projection } )
                 [ "GROUP" "BY" expr { "," expr } ] [ order ] [ offset ] [ limit ] ;
projection     = expr [ "AS" identifier ] ;
order          = "ORDER" "BY" sort_key { "," sort_key } ;
sort_key       = expr [ "ASC" | "ASCENDING" | "DESC" | "DESCENDING" ] [ "NULLS" ( "FIRST" | "LAST" ) ] ;
offset         = ( "OFFSET" | "SKIP" ) int ;
limit          = "LIMIT" int ;
call_stmt      = "CALL" procedure "(" [ expr { "," expr } ] ")" "YIELD" identifier { "," identifier } ;
procedure      = identifier "." identifier ;

path_pattern   = node_pattern { edge_pattern node_pattern } ;
node_pattern   = "(" [ variable ] [ ":" label_expr ] [ property_map ] ")" ;
edge_pattern   = "-[" [ edge_body ] "]->" [ quantifier ]
               | "<-[" [ edge_body ] "]-" [ quantifier ]
               | "-[" [ edge_body ] "]-" [ quantifier ]
               | "~[" [ edge_body ] "]~" [ quantifier ]
               | "-->" | "<--" | "--" | "~~" ;
edge_body      = [ variable ] [ ":" label_expr ] [ property_map ] ;
quantifier     = "{" int "," [ int ] "}" | "{" int "}" | "*" | "+" | "?" ;
label_expr     = label_term { "|" label_term } ;
label_term     = label_factor { "&" label_factor } ;
label_factor   = [ "!" ] ( identifier | "%" | "(" label_expr ")" ) ;
property_map   = "{" [ property ":" expr { "," property ":" expr } ] "}" ;
insert_pattern = insert_node { insert_edge insert_node } ;
insert_node    = "(" [ variable ] [ ":" identifier { "&" identifier } ] [ property_map ] ")" ;
insert_edge    = "-[" [ variable ] ":" identifier [ property_map ] "]->"
               | "<-[" [ variable ] ":" identifier [ property_map ] "]-" ;

expr           = or_expr ;
or_expr        = and_expr { "OR" and_expr } ;
and_expr       = not_expr { "AND" not_expr } ;
not_expr       = [ "NOT" ] test_expr ;
test_expr      = cmp_expr [ "IS" [ "NOT" ] ( "NULL" | "TRUE" | "FALSE" | "UNKNOWN" ) ] ;
cmp_expr       = sum_expr [ ( "=" | "<>" | "<" | "<=" | ">" | ">=" ) sum_expr
               | "IN" "[" [ expr { "," expr } ] "]" ] ;
sum_expr       = mul_expr { ( "+" | "-" ) mul_expr } ;
mul_expr       = unary { ( "*" | "/" | "%" ) unary } ;
unary          = [ "-" ] primary ;
primary        = literal | parameter | variable { "." property } | "(" expr ")"
               | aggregate | function ;
aggregate      = "COUNT" "(" "*" ")"
               | ( "COUNT" | "SUM" | "AVG" | "MIN" | "MAX" ) "(" [ "DISTINCT" ] expr ")" ;
function       = "ELEMENT_ID" "(" variable ")" ;
```

`variable`, `label`, `property` and `identifier` are identifiers; `literal` is a token of section 3. A `statement`
holds at least one clause. A `writing` clause follows every `reading` clause of its statement and precedes its
`return`; a `return` ends a statement, and `NEXT` passes its columns on as bound variables (section 7.5). A request is
one linear query; a session's statements are each a request.

## 5. Values, types and null

### 5.1 Types

The value types are the GQL types `SCHEMA.md` section 5.1 maps the schema's types to, and nothing else reaches a
query:

| GQL type | From UGS | Comparison and order |
|---|---|---|
| `BOOLEAN` | `Bool` | `FALSE < TRUE`; three-valued with `UNKNOWN` (null) |
| `INT8` … `INT64`, `UINT8` … `UINT64`, `INT128` | The integer types | Numeric |
| `FLOAT32`, `FLOAT64` | `Float32`, `Float64` | Numeric; `NaN` and the infinities never arise, since the schema refuses them at write |
| `DECIMAL(p, s)` | `Decimal(p, s)` | Numeric, exact |
| `STRING` | `String`, `Text`, `Iri`, enums, `Unit`, `QuantityKind`, catalog-type values, `Geo` as GeoJSON | By Unicode code point; an enum member compares as its name; a `Text`'s language tag is not a value in the core |
| `BYTES` | `Bytes` | Equality only |
| `DATE`, `ZONED DATETIME`, `LOCAL DATETIME`, `ZONED TIME`, `DURATION` | The temporal types | A `DATE` with a `DATE`, a `ZONED DATETIME` with a `ZONED DATETIME` in UTC, a `LOCAL DATETIME` with a `LOCAL DATETIME`; `DURATION` equality only; across kinds no comparison (`UNKNOWN`) |
| `RECORD` | `Interval<T>`, `record`, `Measure<K>` and `Measure`, `asserted` | Field access by `.`; equality field by field; no order |
| `LIST<T>` | `List<T>`, `Set<T>`, `Vector` | Equality element by element; no order; `IN` tests membership |
| Node, edge | Instances | Equality by identity; no order |

A numeric comparison across integer, decimal and float widths compares the values, exactly for integers and
decimals, and as `FLOAT64` when a float is involved; arithmetic takes the widest operand's type, integer arithmetic
that overflows is refused at run time (`GQL1001`), and `/` on two integers is integer division, truncating toward
zero, division by zero refused (`GQL1002`). A comparison between two values of different kinds above, a string with a
number, a node with anything but a node, is `UNKNOWN` **[choice]**, as MGQL's `evalRelOp` answers and ISO leaves to
the implementation; no comparison raises an error.

### 5.2 Null

`NULL` is the absent value: an optional attribute not set, an unmatched `?` edge, a property asked of a value that
has none, an aggregate over no value. Every operator propagates null as Kleene three-valued logic: a comparison or
arithmetic with a null operand is `UNKNOWN` or `NULL`; `AND`, `OR` and `NOT` as MGQL's `Kleene`; `IS NULL`, `IS NOT
NULL`, `IS TRUE`, `IS FALSE`, `IS UNKNOWN` are the only tests that turn null into `TRUE` or `FALSE`. A `WHERE` or
`FILTER` keeps a record only when its predicate is `TRUE`. Two nulls are not equal (`=` is `UNKNOWN`), and are equal
for `DISTINCT`, grouping and `IN` **[choice]**, as ISO §4.15.5 and §14.11 have it.

### 5.3 Static typing

Every variable, property access and expression has a static type before the query runs, refined by the schema as
MGQL section 3 refines them: a node variable's type is the set of concrete types its label expression admits on the
working graph, an edge variable's the set of relation kinds, and a property access on a variable is typed by the
attribute the schema declares on every type in that set, the union of their types when they differ, nullable when the
attribute is optional on any or absent on some. A property no type in the set declares is refused (`GQL0204`) with
the types that declare it named; an operator applied to types it does not take is refused (`GQL0301`); a label
expression no concrete type satisfies is refused (`GQL0205`), since MGQL's empty type says no row can exist. The
result's column types are the projections' types, which the reply carries (section 10).

## 6. Patterns and their matching

### 6.1 Match semantics

`MATCH` evaluates its path patterns against the working graph and produces a binding table: one record per
**homomorphism** from the pattern to the graph, as ISO §16 and MGQL's atom and path judgements define it, under the
ISO default match mode **REPEATABLE ELEMENTS** and the ISO default path mode **WALK** **[core]**: a node or an edge
may be bound by several variables, and a path may repeat nodes and edges. Several path patterns in one `MATCH` are
joined on their shared variables (MGQL's `bindingTableJoin`); a variable bound in an earlier statement or by `NEXT` is
a constant the pattern must agree with. A `WHERE` on the `MATCH` filters the table. The match mode and path mode
cannot be written (section 11).

### 6.2 Atoms

| Atom | Matches |
|---|---|
| `(n)` | Every node; `(n:L)` every node whose labels satisfy `L`: an identifier is satisfied by a node carrying that label, `%` by a node with any label (every node, since every instance has a type), `!l` by the complement, `a&b` and `a\|b` by both and either |
| `(n {p: v})` | As above, and the property `p` equal to `v`, a literal or parameter; absent is not equal |
| `-[e:T]->` | A directed edge of type `T` from the left atom's node to the right's; a label expression over edge types as for nodes; a property map as for nodes |
| `<-[e:T]-` | A directed edge from the right atom's node to the left's |
| `-[e:T]-` | A directed edge in either direction, or an undirected one |
| `~[e:T]~` | An undirected edge: a `@symmetric` relation's |
| `-->`, `<--`, `--`, `~~` | The same with no variable, type or properties |

An edge type is an edge-encoded relation's navigation name, a role type `{Relation}_{role}`, or a navigation name of
a node-encoded relation (section 2), and a label expression over edge types resolves each name as the schema declares
it; a role type's specialized and super names both match the role edge. The anonymous atoms of a pattern are given
fresh names, as MGQL's renaming does, and never appear in a result.

### 6.3 Quantified edges

An edge pattern may carry a quantifier: `{m,n}` with `0 ≤ m ≤ n`, `{m,}` with no upper bound, `{m}`, `*` for `{0,}`,
`+` for `{1,}`, `?` for `{0,1}`. It stands for a chain of `m` to `n` edges of that pattern, each hop's intermediate
node unconstrained, in the pattern's direction.

**Semantics [core].** The core answers a quantified edge by **reachability**: for each binding of the left node, the
distinct nodes reachable by a chain within the bounds, one record per distinct end node, computed by a frontier
expansion that deduplicates nodes per hop and so terminates on cycles and unbounded quantifiers. Under ISO's WALK mode
the same pattern yields one record per walk, so to keep the core's results equal to ISO's wherever the core accepts a
query, a statement holding a quantified edge, other than `?`, SHALL project with `DISTINCT`, and every aggregate in it
SHALL be `DISTINCT` (`GQL0401` names the fix); a `COUNT(*)` over such a statement is refused the same way. No variable
may be declared on a quantified edge or inside it (`GQL0402`): group variables, the lists MGQL types with `List`, are
path variables' business, outside the core. `?` is MGQL's optional edge: a zero-hop record binding the two node atoms
to one node with the edge variable `NULL`, beside every one-hop record. `{0,n}` and `*` include the zero-hop record,
which binds the end to the start.

### 6.4 Property maps and parameters

A property map's values are literals and parameters; an expression over variables in a map is outside the core
(`GQL0403`). A parameter is typed by the prepared statement, and a property compared with a parameter of another
type is refused when prepared (`GQL0301`).

## 7. Statements and their semantics

### 7.1 The pipeline

A linear query is evaluated clause by clause, each clause a function from a binding table to a binding table, as
ISO §4.3.6 and MGQL section 5.3 have it: `MATCH` begins from the one empty record, or from the table the previous
clause or `NEXT` produced, and extends it; `FILTER` and `WHERE` keep records; a writing clause (section 8) changes
the graph and passes its table on; `RETURN` projects, groups, orders and slices it into the statement's result. A
statement without `RETURN` that ends a request returns the empty table with no columns.

### 7.2 Projection

`RETURN e1 AS a1, …`: each projection evaluates its expression per record; an unaliased projection takes its text as
its column name; a variable alone keeps its name; `RETURN *` projects every variable in scope, in the order they were
bound. Column names are unique (`GQL0501`). `DISTINCT` removes duplicate records, nulls equal.

### 7.3 Aggregation and grouping

An aggregate in a projection groups the table. The grouping keys are the `GROUP BY` expressions when given, each of
which must appear as a projection; without `GROUP BY`, every projection that holds no aggregate is a key, ISO §14.11's
implicit grouping; a projection mixing an aggregate with a non-aggregated variable is refused (`GQL0502`). With no key
and an aggregate, the table is one group, one record even over no input. Per group:

| Aggregate | Value |
|---|---|
| `COUNT(*)` | The records in the group |
| `COUNT(e)` | The records where `e` is not null |
| `SUM(e)` | The sum of the non-null values, in the widest type; `NULL` over none |
| `AVG(e)` | `SUM / COUNT` as `FLOAT64`, or `DECIMAL` over decimals; `NULL` over none |
| `MIN(e)`, `MAX(e)` | The least and greatest non-null value by section 5.1's order; `NULL` over none; refused when the type has no order (`GQL0301`) |
| `DISTINCT` | Over the distinct non-null values |

MGQL's `evalAggOnValues` is the reference for `COUNT`, `SUM`, `MIN` and `MAX` over integers.

### 7.4 Ordering and slicing

`ORDER BY` sorts the projected table by its keys in turn, each an expression over the projected columns or the
variables in scope; `ASC` is the default. Nulls sort last ascending and first descending unless `NULLS FIRST` or
`NULLS LAST` says otherwise **[choice]**. Values of different kinds under one key sort in the order boolean,
number, string, date, zoned datetime, local datetime, zoned time, duration, node, edge, record, list **[choice]**,
and within a kind by section 5.1; records, lists, nodes and edges in an order the implementation keeps stable within
one result and need not state. `OFFSET` skips, `LIMIT` keeps; both after ordering; without `ORDER BY`, the order is
the store's and a `LIMIT` picks from it. A limited result continues through the protocol's resume key (section 10),
stable at the transaction's snapshot.

### 7.5 `NEXT`

`statement NEXT statement`: the second statement begins with the first's result table, its columns bound as
variables, and may match, filter, write and return over them; every variable not returned by the first is out of
scope. A `NEXT` after a statement without `RETURN` passes the empty record (`GQL0503` when the second statement
names a variable of the first).

### 7.6 Monotonicity and read levels

A statement is **monotonic** when it holds no `NOT`, no `IS NULL`, no aggregate, no `ORDER BY` with `LIMIT`, no
negated label and no procedure of section 9; such a statement may run at the monotonic-fast level the transaction's
Begin names (`PROTOCOL.md` section 11.11), and the reply flags it as `SCHEMA.md` section 9.6 states when it read
mutable data. Every other statement runs at a snapshot whatever the Begin named, and the reply says so.

## 8. Writes

A writing clause changes the working graph in the transaction of the request; nothing is visible outside the
transaction before its commit, and every instance the transaction creates or changes is validated whole at the
commit, against the schema (`SCHEMA.md` section 8): a refusal there aborts the transaction and reaches the client as
section 11.3 states.

### 8.1 `INSERT`

`INSERT (n:Person {id: 'AGENT:ada', personal_name: 'Ada'})-[:asserted_by]->(s)` creates nodes and edges:

- A node with labels creates an entity of the **one concrete type** the labels name (`GQL0601` for none or two; a
  supertype's label beside the concrete one is allowed and redundant), with the property map's attributes; the key
  attributes must be present (`UGS1006`); the IRI is minted from the key, or a handle for a keyless type, and the
  node variable is bound to it for the rest of the statement.
- An edge `-[:T {…}]->` from a bound or inserted node to another creates an instance of the edge-encoded relation
  whose anchor navigation name is `T`, in the anchor's direction (`GQL0602` for a navigation edge of section 2, a
  role type or the wrong direction); its property map's attributes are the instance's.
- A node-encoded relation instance is inserted as its node with the relation's label and its attributes, and one
  role edge per role, `-[:MemberOf_member]->(p)`, in the same statement or transaction; the instance is validated
  whole at commit (`UGS1015` for a missing required role).
- A node without labels, or with `Resource`, is admitted only under `@open` (`UGS1002`).
- `INSERT` of a `@set` relation whose combination exists resolves to the existing instance, as `SCHEMA.md` section
  6.4 states; its property map updates it.

### 8.2 `SET` and `REMOVE`

`SET n.p = e` writes the attribute `p` of the instance `n` is bound to; `REMOVE n.p` clears an optional one. On a
`@bitemporal` type each is a **revision**: a new version of the instance, the old one readable at its epochs
(`SCHEMA.md` section 8.4). A key or `@immutable` attribute, a required attribute under `REMOVE`, an `@append_only`
instance and a derived name are refused (`UGS1007`, `UGS1012`, `UGS1005`, `UGS1013`, `UGS1029`). `SET` on an edge
variable writes the relation instance's attribute; on a navigation edge it is refused (`GQL0603`).

### 8.3 `DELETE` and `DETACH DELETE`

`DELETE n` deletes the instance `n` is bound to: an edge or relation node is deleted with the relations over it; an
entity with live relations naming it is refused (`GQL0604`) unless `DETACH DELETE`, which applies each relation's
`@on_delete` action, `cascade` by default and `restrict` refusing (`UGS1020`), as `SCHEMA.md` section 8.5 states.
`NODETACH DELETE` is `DELETE`. On a `@bitemporal` type a delete is a **retraction** (`SCHEMA.md` section 8.4): the
instance stays readable through `believed at` reads, and `DETACH` applies `@on_retract`. A navigation edge cannot be
deleted (`GQL0603`); the relation node can.

### 8.4 Order within a statement

Reading clauses see the graph as the transaction's snapshot has it, before the statement's own writes; a writing
clause runs once per record of the table it receives, in table order, and later clauses of the same statement see
its writes. Two writes to one attribute in one statement are the last's.

## 9. Procedures: vector and text search

The two retrieval operators the plan places outside the standard languages are procedures:

| Procedure | Arguments | Yields |
|---|---|---|
| `vector.search(label, property, vector, k)` | A label as a string, an attribute of `Vector` type declared with `@index(ann)` on it, a `LIST<FLOAT64>` of the index's dimension, `k ≥ 1` | `node`, the nearest `k` instances by the index's metric, and `score`, the similarity, best first |
| `text.search(label, property, query, k)` | A label, a `Text` attribute with `@index(fulltext)`, a query string, `k ≥ 1` | `node` and `score`, BM25, best first |

`CALL … YIELD node, score` begins a statement; `MATCH`, `FILTER`, writes and `RETURN` follow over the yielded
variables. A procedure the catalog does not offer, or an argument of the wrong type or outside its index, is refused
(`GQL0701`, `GQL0702`). Both run at a snapshot (section 7.6). Scoping a search by a preceding pattern, the agentic
projection's filtered search, is the full-conformance stage's; in the core the pattern follows the search.

## 10. Transactions, consistency and the protocol

- **A request is a statement in a transaction.** The client begins a transaction (`PROTOCOL.md` section 6.2) naming
  the tenant and, for a read-only transaction, its level; sends each linear query as one request on a stream of its
  own, with its parameters; and commits or aborts. A request's text and parameters travel as section 14 asks the
  protocol to carry them.
- **Read-only and read-write.** A query holding no writing clause runs in a read-only transaction at its level, or
  in a read-write one at the transaction's partition's latest state; a query with a writing clause requires a
  read-write transaction (`GQL0801`), and its reads are recorded and validated at commit as every read-write
  transaction's are. A query with a writing clause and no reading clause, whose writes the catalog classifies
  coordination-free, is what a DirectWrite carries (`PROTOCOL.md` section 6.6); the frontend decides from the
  catalog's request-typing facts (`SCHEMA.md` section 9.6), not the client.
- **Prepared statements.** A query is prepared once per connection with the types of its parameters, and executed
  with values by position; preparing resolves every name against the tenant's current catalog version and records the
  schema id, so a schema change stales the statement as `PROTOCOL.md` section 11.7 has it. A parameter's type is one of
  section 5.1's; a node parameter is an IRI string.
- **Results.** A statement's result is a stream of columnar batches: one column per projection, named and typed, a
  cell per record, nodes and relation nodes by their IRI, edges by their reifier IRI, scalars in the term encoding
  of `PROTOCOL.md` section 10, `ABSENT` for null, records and lists as section 14 asks. A limited result carries a
  resume key; the reply carries the epoch read at and the monotonic flag.
- **Partitions.** A read-only query reads across partitions at its snapshot; a read-write query's reads and writes
  stay in its transaction's partition in this stage and a request reaching another is refused as `PROTOCOL.md`
  section 11.11 has it, until stage 8 sequences it. The planner routes a pattern as the quad-reads specification
  states: a bound node asks its home, an unbound subject fans out.

## 11. Diagnostics and the feature register

### 11.1 Diagnostics

A refusal names a code, a location in the query text, a reason and, where one exists, a fix, in the style of
`SCHEMA.md` section 10.4. The codes:

| Code | Condition |
|---|---|
| `GQL0001` | The query past its size or nesting bound |
| `GQL0002` | A reserved word as an identifier, or a token the lexer does not know |
| `GQL0003` | A malformed literal |
| `GQL0004` | A token the grammar does not expect; the message names what was expected |
| `GQL0101` | A feature outside the core (section 11.2), named |
| `GQL0201` | No working graph: no `USE` and no `@default` graph in the tenant, or an unknown graph |
| `GQL0202` | An unknown label; the message lists the nearest declared ones |
| `GQL0203` | An unknown edge type |
| `GQL0204` | A property no type the variable may hold declares; the message names the types that declare it |
| `GQL0205` | A label expression no concrete type satisfies |
| `GQL0206` | An unbound variable |
| `GQL0207` | A variable bound twice as different kinds, a node and an edge |
| `GQL0301` | An operator, function or comparison applied to types it does not take; a parameter of another type than its use |
| `GQL0401` | A quantified edge in a statement that does not project `DISTINCT` or aggregate `DISTINCT` |
| `GQL0402` | A variable on or inside a quantified edge |
| `GQL0403` | A property map value that is not a literal or parameter |
| `GQL0501` | Two projections with one name |
| `GQL0502` | A projection mixing an aggregate with a non-aggregated variable, or a `GROUP BY` key that is not projected |
| `GQL0503` | A variable of a statement before `NEXT` that was not returned |
| `GQL0601` | An inserted node with no concrete type or two |
| `GQL0602` | An inserted edge that is a navigation edge, a role type, or against the anchor's direction |
| `GQL0603` | `SET`, `REMOVE` or `DELETE` on a navigation edge |
| `GQL0604` | `DELETE` of an entity that live relations name, without `DETACH` |
| `GQL0701` | An unknown procedure |
| `GQL0702` | A procedure argument of the wrong type, or naming an attribute without the index it needs |
| `GQL0801` | A writing clause in a read-only transaction |
| `GQL1001` | Integer overflow at run time |
| `GQL1002` | Division by zero |
| `GQL1003` | A result past the transaction's bounds, or a write past the write set's (`PROTOCOL.md` code 6) |

A schema refusal at commit reaches the client with its `UGS1xxx` number and the instance, as `SCHEMA.md` section
8.7 states; a conflict or timeout as `PROTOCOL.md` section 9.4 states.

### 11.2 The feature register

Every ISO feature the core leaves out is refused with `GQL0101` naming it. The register, which the
full-conformance stage grows into the complete one:

| Feature | Refused as | Stage |
|---|---|---|
| `OPTIONAL MATCH` | `optional match` | Full conformance |
| Path variables, `PATH`, path modes `TRAIL`, `ACYCLIC`, `SIMPLE`, `WALK` written out, match modes written out | `path pattern` | Full conformance |
| Group variables: a variable on or inside a quantifier | `group variable` | Full conformance |
| Quantified path patterns with parentheses | `quantified path pattern` | Full conformance |
| Shortest paths, `ANY`, `ALL`, `SHORTEST` | `shortest path` | Full conformance |
| `UNION`, `EXCEPT`, `INTERSECT`, `OTHERWISE` | `composite query` | Full conformance |
| `LET`, `FOR`, `ORDER BY` as a statement | `let`, `for`, `order statement` | Full conformance |
| Graph type and schema statements, `CREATE GRAPH`, `DROP GRAPH` | `catalog statement` | The schema is UGS; no stage |
| `SESSION` and `TRANSACTION` statements | `session statement` | The protocol's Begin and Commit; no stage |
| String, list, temporal and mathematical functions beyond section 4 | `function` | Full conformance |
| `CASE`, `COALESCE`, `EXISTS`, subqueries | `expression` | Full conformance |
| Undirected `INSERT` edges, `MERGE` | `merge` | Not adopted: `MATCH` then `INSERT` in one transaction, as the plan decides |

## 12. Conformance: the oracles

Three oracles, all run by `make test`:

1. **MGQL.** The read fragment's results are compared with the mechanised evaluator's. A harness encodes each MGQL
   test and example query (`Test.lean`, `Examples/`, the six LDBC queries of `LDBCBench.lean`) as GQL text and its
   graph as UGS data on a schema of one label per MGQL node schema, runs it through the core, and compares binding
   tables as bags. MGQL evaluates under Cypher's `TRAIL` and `DIFFERENT EDGES`, so the comparison filters the core's
   table by MGQL's `Record.isTrail`, which on every MGQL graph without a repeated edge leaves it unchanged; a query of
   MGQL's fragment outside the core, a composite query or a group-referenced quantifier, is a refusal test instead.
   The six LDBC queries in the core's syntax:

   ```
   MATCH (n:Person {id: $personId})-[:IS_LOCATED_IN]->(p:City)
   RETURN n.firstName AS firstName, n.lastName AS lastName, p.id AS cityId                      // IS1
   MATCH (n:Person {id: $personId})-[:KNOWS]-(friend)
   RETURN friend.id AS personId, friend.firstName AS firstName, friend.lastName AS lastName    // IS3
   MATCH (m:Message {id: $messageId})
   RETURN m.creationDate AS messageCreationDate, m.content AS messageContent                   // IS4
   MATCH (m:Message {id: $messageId})-[:HAS_CREATOR]->(p:Person)
   RETURN p.id AS personId, p.firstName AS firstName, p.lastName AS lastName                   // IS5
   MATCH (start:Person {id: $personId})<-[:HAS_CREATOR]-(msg1:Message),
         (comment:Comment)-[:REPLY_OF]->(msg1), (comment)-[:HAS_CREATOR]->(person:Person)
   RETURN person.id AS personId, person.firstName AS personFirstName, person.lastName AS personLastName,
          comment.id AS commentId, comment.content AS commentContent                            // IC8
   MATCH (p:Person {id: $personId})-[:KNOWS]-(friend:Person), (message:Message)-[:HAS_CREATOR]->(friend)
   WHERE message.creationDate < $maxDate
   RETURN friend.id AS personId, friend.firstName AS personFirstName,
          message.id AS messageId, message.content AS messageContent                           // IC2
   ```

   Each must return MGQL's golden rows (the assertions of `LDBCBench.lean` section 4), and the core's inferred column
   types must equal the schemas MGQL's certified checker prints.
2. **Differential, against SPARQL.** Every scenario of section 13 and every MGQL query is also written in the SPARQL
   core over the same data, and the two answer sets are compared as bags of rows under the correspondence
   `QUERY-SPARQL-CORE.md` section 12 states.
3. **The openCypher TCK** runs at the full-conformance stage; in the core, the scenarios of its features inside the
   core run as refusal or result tests, each feature named.

## 13. DeepBrain in the core

The Gen5 schema (`scrith/openspec/notes/schema/gen5/DeepBrainSchema.ugs`) installed in a tenant as its namespace
`deep_brain`, graph `ontology`, the default graph. Every operation the DeepBrain MVP makes, in the core.

**Create.** A person, an organisation, a membership with its envelope, and a provenance link, in one read-write
transaction:

```
INSERT (ada:Person {id: 'AGENT:ada', personal_name: 'Ada', inferred: FALSE, rank: 'normal'})
INSERT (acme:Company {id: 'AGENT:acme', label: 'Acme', inferred: FALSE, rank: 'normal'})
INSERT (src:Source {id: 'SRC:hr', label: 'HR system', source_type: 'system', inferred: FALSE, rank: 'normal'})
INSERT (ada)-[:asserted_by]->(src), (acme)-[:asserted_by]->(src)
INSERT (m:MemberOf {valid: {start: ZONED DATETIME '2026-01-01T00:00:00Z'}, method: 'imported',
                    confidence: 0.98, inferred: FALSE, rank: 'normal'})
       -[:MemberOf_member]->(ada), (m)-[:MemberOf_collection]->(acme), (m)-[:asserted_by]->(src)
RETURN ELEMENT_ID(m) AS membership
```

`rank` and `inferred` may be omitted, since the schema defaults them. The membership's handle IRI comes back and
names it from now on. A `SubOrganizationOf` is inserted the same way, its node in the tree's root partition; the
schema's `@acyclic`, `@no_overlap` and `UniquePreferred` checks run at commit.

**Read.** A node by its key; a unit's open members with their roles; every unit below a unit, DeepBrain's
`InUnit`, through the navigation edge `units` of `SubOrganizationOf`; a count per unit:

```
MATCH (p:Node {id: $id}) RETURN p.label, p.rank, p.asserted.start AS since

MATCH (u:Organization {id: $unit})<-[:MemberOf_collection]-(m:MemberOf)-[:MemberOf_member]->(x)
WHERE m.valid.end IS NULL
RETURN x.id AS member, m.rank AS rank

MATCH (u:Organization {id: $unit})-[:units]->{1,32}(d:Organization)
RETURN DISTINCT d.id AS unit, d.label AS label

MATCH (u:Organization {id: $unit})-[:units]->{0,32}(d)<-[:MemberOf_collection]-(m:MemberOf)
WHERE m.valid.end IS NULL
RETURN DISTINCT d.id AS unit, COUNT(DISTINCT m) AS open_memberships
```

The last two project `DISTINCT` because they quantify (section 6.3). A read of every kind of relation from a
statement, `out(is Governs)` in UQL, is `MATCH (s:Node {id: $id})<-[:Governs_governed]-(g:Governs)`.

**Update.** A re-ranking and a confidence change are revisions of the bitemporal membership; a label change on a
mutable attribute of an organisation likewise:

```
MATCH (m:MemberOf) WHERE ELEMENT_ID(m) = $membership
SET m.rank = 'preferred', m.confidence = 0.99

MATCH (o:Organization {id: 'AGENT:acme'}) SET o.label = 'Acme Holdings'
```

Both keep the instance's identity; `believed at` reads of the UQL stage, and the SPARQL core's reads at an epoch, see
the earlier version.

**Delete.** Retracting a membership, and retracting a person with everything that names them:

```
MATCH (m:MemberOf) WHERE ELEMENT_ID(m) = $membership DELETE m

MATCH (p:Person {id: 'AGENT:ada'}) DETACH DELETE p
```

The first closes the membership's `asserted`; the second retracts Ada and, by `@on_retract`'s default `cascade`,
every relation naming her, while `MemberOf`'s `restrict open` on the collection side refuses a retraction of an
organisation with open memberships (`UGS1020`).

**Retrieve.** The nearest concepts to an embedding, and the claims a text query matches:

```
CALL vector.search('Concept', 'embedding', $q, 8) YIELD node, score
MATCH (node)-[:in_scheme]->(s:Scheme) RETURN node.id, node.label, s.label, score

CALL text.search('Claim', 'statement', $q, 8) YIELD node, score
RETURN node.id, node.statement, score
```

**Schema.** Installing, extending and reading the schema are the UGS install and Describe of `SCHEMA.md` sections
10.1 and 10.3, not GQL statements; a client that wants a schema's labels and properties reads Describe. What the core
leaves to UQL: `believed at` and `with retracted` reads over the transaction-time history, the typed tree results,
and the `edge` accessor; what it leaves to the full-conformance stage: `OPTIONAL MATCH` for "a membership and its
role if any", and `UNION` across kinds.

## 14. What this specification asks of the others

- **`PROTOCOL.md`.** Carried as of 2026-11-05: Query (section 7.4 there) carries the language, the text and named
  parameters on a stream of its own and answers Result frames; Prepare and Execute take a text with typed parameters
  and answer named, typed columns (section 8.1); Result batches carry column names, the monotonic flag and null as
  `ABSENT`, lists and records as `rdf:JSON` literals (section 9.1); an update answers its counts; code 29 carries a
  refused query's diagnostic and code 7 a schema rule's refusal at commit (section 9.4).
- **`SCHEMA.md`.** Section 11.3's GQL view gains the navigation edge of section 2, derived from a node-encoded
  relation's role quads, read-only, and section 11.2's RDF view its counterpart, a derived property; section 8.4's
  revision rule is what `SET` on a bitemporal instance relies on.
- **`quad-reads`.** A pattern with a bound node and an edge type is a forward read of that node's lists; a reverse
  navigation is a reverse read through the referrer set; the planner's choice of direction is the dual-engine plan's.

## 15. Decisions

| Question | Decision |
|---|---|
| Match and path modes | ISO's defaults, `REPEATABLE ELEMENTS` and `WALK`, which are SPARQL's basic graph pattern semantics, so one question answers alike in both cores; written modes are the full-conformance stage's |
| Quantified edges | Reachability, one record per distinct end node, and the statement must project or aggregate `DISTINCT`, so the core's answer equals ISO's walk semantics wherever the core accepts the query, and terminates where ISO's would not |
| Navigation edges | A node-encoded relation's navigation names are derived one-hop edges in the view, so a walk through relation nodes is one hop as through edges; read-only, with no properties |
| Incomparable values | `UNKNOWN`, never an error, as MGQL answers; integer overflow and division by zero are errors |
| Null ordering | Last ascending, first descending by default; `NULLS FIRST` and `NULLS LAST` override |
| Grouping | `GROUP BY` explicit, or implicit over the non-aggregated projections |
| Updates on bitemporal types | A revision, `SCHEMA.md` section 8.4; a delete a retraction |
| Retrieval | Procedures `vector.search` and `text.search`, root of a statement; scoped search waits for the agentic projection |
| `MERGE`, `WITH` | Not in the core: `MATCH` then `INSERT` in one transaction; `NEXT` |
| The reference | ISO 39075:2024 by its text; MGQL as the executable oracle of the read fragment, including its type checker's schemas as the expected column types |
