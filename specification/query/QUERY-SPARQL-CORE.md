# The SPARQL core

This file specifies the subset of SPARQL 1.1 Query and SPARQL 1.1 Update (W3C Recommendations, 21 March 2013), with
one construct for RDF 1.2's reifiers, the triple term, that UltraGraphDB's query core executes: the query forms,
patterns, filters, solution modifiers and updates it accepts, their meaning over the RDF view a UGS schema declares,
how they read and write through the engine's transactions, what they return, and how a query outside the core is
refused. The core is the fragment the DeepBrain MVP needs: a schema is created, data is written, read, updated and
deleted, and every common operation of DeepBrain's Gen5 schema is expressed in it (section 13). Everything else,
`OPTIONAL`, `UNION`, `MINUS`, full property paths, `CONSTRUCT`, `DESCRIBE`, `ASK`, `BIND` and `VALUES` beyond what is
named here, named-graph management and the SPARQL Protocol, is the full-conformance stage's, and a query using any of
it is refused naming the feature (section 11).

`QUERY-GQL-CORE.md` is the sibling: the two cores read one store, and a question asked in both answers alike (section
12). `SCHEMA.md` gives the vocabulary and the RDF view the core sees; `PROTOCOL.md` the bytes a query, its parameters
and its result travel in; `openspec/notes/UltraGraphDB Dual Query Engine Plan.md`, "The query core", the design. The
reference semantics is the SPARQL 1.1 algebra (Query, section 18), and the W3C test suite's cases inside the core are
the oracle (section 12).

## Contents

1. Conventions
2. The dataset a query sees
3. Lexical structure
4. Grammar of the core
5. Terms, values and errors
6. Graph patterns and their evaluation
7. Solution modifiers and aggregates
8. Updates
9. Extension functions: vector and text search
10. Transactions, consistency and the protocol
11. Diagnostics and the feature register
12. Conformance: the oracles
13. DeepBrain in the core
14. What this specification asks of the others
15. Decisions

## 1. Conventions

| Term | Meaning |
|---|---|
| Query, Update | SPARQL 1.1 Query Language and SPARQL 1.1 Update, cited as `Query §n` and `Update §n` |
| RDF 1.2 | RDF 1.2 Concepts and Abstract Data Model (W3C Candidate Recommendation, 7 April 2026): terms, quads, reifiers and triple terms, which the mapping contract keeps. The triple term's written form `<<( s p o )>>` is the one taken from the SPARQL 1.2 Query Working Draft of 25 October 2026, frozen at that draft; nothing else of SPARQL 1.2 is in the core (section 15) |
| Tenant, namespace, graph | As `../schema/SCHEMA.md` section 1: a tenant isolates; a namespace is an installed schema with its graphs; a graph is an RDF named graph |
| Solution, solution sequence | A solution mapping from variables to terms and a sequence of them with multiplicity, `Query §18.1`; the core evaluates every operator as a bag |
| Live | An instance that is not deleted and, on a `@bitemporal` type, not retracted (`../schema/SCHEMA.md` section 8.4) |
| Core | The forms this file admits; a query is in the core when every construct it uses is, otherwise it is refused whole (section 11) |
| SHALL, refused | A rule the implementation holds; a refusal is a diagnostic of section 11 and no part of the request runs |

Keywords are case-insensitive. Where the Recommendation leaves a choice to the implementation this file states it and
marks it **[choice]**; where this file narrows the Recommendation it marks it **[core]** and section 11 names the
feature.

## 2. The dataset a query sees

A query runs against the **RDF dataset** of one tenant, fixed by the transaction (section 10): its default graph is
the tenant's `@default` graph (`../schema/SCHEMA.md` section 6.11), and its named graphs are every declared graph of the
tenant's namespaces the principal may read, each under the IRI `SCHEMA.md` section 6.11 gives it. `FROM` and `FROM
NAMED` are refused (section 11): the dataset is the tenant's. A graph of another tenant does not exist to the query.

The dataset is the RDF view `../schema/SCHEMA.md` section 11.2 derives from the schema and the mapping contract, with no store
state visible that the view does not name:

| In the view | From the schema |
|---|---|
| `?s rdf:type ?C` | An instance's concrete type and every supertype, statement types included, answered from the type prefix; one triple per type, in the instance's graph |
| `?s {base}attr ?v` | An attribute's value as a literal of `../schema/SCHEMA.md` section 5.1's datatype; one triple per member of a `Set`; an `Interval` as `{attr}_start`, `{attr}_end` and the precision literals; a bitemporal instance's `asserted_start` and `asserted_end` as `xsd:dateTimeStamp`, and `live` as `xsd:boolean` |
| `?a {base}nav ?b` | An edge-encoded relation instance: the asserted triple from the anchor under the anchor's navigation name; the inverse navigation name's triple is answered as `owl:inverseOf` promises, from the reverse index |
| `?r rdf:reifies <<( ?a {base}nav ?b )>>` and `?r {base}attr ?v` | The instance's reifier, named by its reifier IRI, and the attributes it carries; the one SPARQL 1.2 construct of the core (section 6.4) |
| `?r rdf:type {base}Rel`, `?r {base}Rel_role ?p` | A node-encoded relation instance, a node with its role triples, named by its handle IRI; a specialized role answers its own and the super role's triple |
| **Navigation property** `?a {base}nav ?b` of a node-encoded relation | A derived triple, one per instance, from the participant that gains the navigation name to the participant the name reaches: `?u db:parents ?p` stands for `?r db:SubOrganizationOf_unit ?u . ?r db:SubOrganizationOf_parent ?p`. Read-only; published as `owl:propertyChainAxiom`; it exists so that a property path walks a relation node in one step, as it walks an edge |
| A keyless entity, a relation node, a reifier | An IRI (`../schema/SCHEMA.md` section 7.3); a blank node in the view is an ungoverned one under the skolem base, as the mapping contract presents it |

A predicate, class or graph a query names must be one the schema declares or the view derives (`SPQ0202`, `SPQ0203`); on
a closed graph a predicate no type declares matches nothing, and the planner refuses it instead, since a misspelt IRI is
otherwise an empty answer. Undeclared data under `@open` is reachable under its own predicates and the class
`{base}Resource`. A predicate or class IRI whose local name a namespace's migration retired (`../schema/SCHEMA-EVOLUTION.md`
section 4.2) resolves to the item that holds the name now during the migration's deprecation window, with a warning
naming the current IRI (`SPQ0210`), and after the window is refused with the current IRI in the message (`SPQ0202`,
`SPQ0203`).

## 3. Lexical structure

`Query §19.8` as narrowed here. Source is UTF-8, bounded by the protocol's query section, `max_frame` less 512 bytes,
which `../binary_protocol/PROTOCOL.md` section 7.4 refuses before the core sees it; nesting is bounded at 64 (`SPQ0001`).

| Token | Form |
|---|---|
| IRI | `<…>` absolute, or a prefixed name `prefix:local` with `PREFIX` declared, the tenant's vocabulary prefixes (`../schema/SCHEMA.md` section 6.1) implied, and `rdf:`, `rdfs:`, `xsd:`, `geo:` and `ugdb:` implied under their standard namespaces, `ugdb:` section 9's; a relative IRI is refused (`SPQ0003`) since no base is set |
| Variable | `?name` or `$name` |
| Parameter | A variable the prepared statement names as a parameter (section 10); it is bound before evaluation and never projected unbound |
| Literal | `"…"` or `'…'` with SPARQL's escapes, `"…"@lang`, `"…"^^<datatype>`; integers, decimals, doubles, `true`, `false` as `Query §19.8`; a literal that is not valid for its datatype is refused (`SPQ0003`) where the datatype is one of section 5.1's |
| Blank node | `_:label` and `[]`, in query patterns as non-projectable variables, in update data as fresh instances |
| Triple term | `<<( s p o )>>` with `s`, `p`, `o` terms or variables (section 6.4) |
| Comment | `#` to the end of the line |

## 4. Grammar of the core

```
request        = { prefix } ( select | update ) ;
prefix         = "PREFIX" pname ":" iriref ;

select         = "SELECT" [ "DISTINCT" ] ( "*" | sel_item { sel_item } ) where modifiers ;
sel_item       = var | "(" expr "AS" var ")" ;
where          = "WHERE" group ;
group          = "{" { element } "}" ;
element        = triples | graph_block | subselect | filter ;
triples        = triple { "." triple } [ "." ] ;
triple         = subject predicate_list ;
predicate_list = path object_list { ";" path object_list } ;
object_list    = object { "," object } ;
subject        = var | iri | blank | triple_term ;
object         = var | iri | blank | literal | triple_term | collection ;
collection     = "(" { object } ")" ;
triple_term    = "<<(" subject iri object ")>>" ;
path           = seq { "|" seq } ;
seq            = step { "/" step } ;
step           = [ "^" ] ( iri | "a" ) [ "+" | "*" | "?" ] | "(" path ")" [ "+" | "*" | "?" ] ;
graph_block    = "GRAPH" iri group ;
subselect      = "{" select "}" ;
filter         = "FILTER" ( "(" expr ")" | "NOT" "EXISTS" "{" triple "}" | call ) ;
modifiers      = [ "GROUP" "BY" group_key { group_key } ] [ "ORDER" "BY" order_key { order_key } ]
                 ( [ "LIMIT" int ] [ "OFFSET" int ] | "OFFSET" int "LIMIT" int ) ;
group_key      = var | "(" expr [ "AS" var ] ")" ;
order_key      = [ "ASC" | "DESC" ] "(" expr ")" | var ;

expr           = or_expr ;
or_expr        = and_expr { "||" and_expr } ;
and_expr       = cmp_expr { "&&" cmp_expr } ;
cmp_expr       = sum_expr [ ( "=" | "!=" | "<" | ">" | "<=" | ">=" ) sum_expr ] ;
sum_expr       = mul_expr { ( "+" | "-" ) mul_expr } ;
mul_expr       = unary { ( "*" | "/" ) unary } ;
unary          = [ "!" | "-" ] primary ;
primary        = literal | var | "(" expr ")" | call | aggregate ;
call           = ( "BOUND" | "ISIRI" | "ISBLANK" | "ISLITERAL" | "STR" | "LANG" | "DATATYPE" ) "(" expr ")" ;
aggregate      = "COUNT" "(" [ "DISTINCT" ] ( "*" | expr ) ")"
               | ( "SUM" | "AVG" | "MIN" | "MAX" ) "(" [ "DISTINCT" ] expr ")" ;

update         = insert_data | delete_data | modify ;
insert_data    = "INSERT" "DATA" quad_data ;
delete_data    = "DELETE" "DATA" quad_data ;
modify         = [ "WITH" iri ] [ "DELETE" quad_pattern ] [ "INSERT" quad_pattern ] "WHERE" group ;
quad_data      = "{" { ground_triples | "GRAPH" iri "{" ground_triples "}" } "}" ;
quad_pattern   = "{" { triples | "GRAPH" iri "{" triples "}" } "}" ;
```

`ground_triples` are `triples` whose terms are IRIs, literals, blank nodes and parameters (section 10), no other
variables. `a` is `rdf:type`, as `Query §19.8` has it. A collection stands only as a property function's object list
(section 9); elsewhere it is outside the core (section 11, `collection`). `LIMIT` and `OFFSET` come in either order,
as `Query §19.8`'s `LimitOffsetClauses` admits. A `modify` holds
`DELETE`, `INSERT` or both. A request is one query or one update; a sequence of updates separated by `;` is outside
the core (section 11): the transaction sequences requests.

## 5. Terms, values and errors

### 5.1 Terms

Terms are RDF 1.2's: IRIs, blank nodes, literals with a datatype and, for `rdf:langString`, a language tag and base
direction, and triple terms. Term equality is RDF term equality (`Query §17.4.1.7`): two literals are the same term
when their lexical forms, datatypes and tags are equal. The value-space comparisons below apply where `Query §17.3`
defines them:

| Datatypes | Comparison and order |
|---|---|
| `xsd:integer` and its subtypes, `xsd:decimal`, `xsd:float`, `xsd:double` | Numeric, with `Query §17.3.1`'s promotion; exact between integers and decimals, as `xsd:double` when a float or double takes part |
| `xsd:string`, simple literals | By Unicode code point; `rdf:langString` equal when tag and form are equal, ordered by form **[choice]** |
| `xsd:boolean` | `false < true` |
| `xsd:dateTime`, `xsd:dateTimeStamp` | By instant in UTC; a value with no timezone compares only with values with none **[choice]**, `Query §17.3.1`'s `op:dateTime-equal` without the implicit timezone |
| `xsd:date`, `xsd:time` | By value, each kind with itself |
| `xsd:duration` and subtypes | Equality only |
| `geo:geoJSONLiteral`, `ugdb:float32vector`, `ugdb:measure`, `ugdb:bfloat16vector`, `rdf:JSON`, `xsd:base64Binary`, `xsd:anyURI` | Term equality only |

A comparison the table does not define is a type error (`Query §17.2`), which `FILTER` treats as `false` and
`ORDER BY` orders by section 7.3's rule.

### 5.2 Expressions

`Query §17` as narrowed: `||`, `&&`, `!` with the effective Boolean value and error rules of `Query §17.2.2`; the
comparison operators over section 5.1; `+`, `-`, `*`, `/` over numerics with promotion, `/` giving `xsd:decimal` for
two integers and a division by zero an error; `BOUND`, `ISIRI`, `ISBLANK`, `ISLITERAL`, `STR`, `LANG`, `DATATYPE` as
`Query §17.4`. Every other built-in is outside the core (section 11). An expression's error is a value: it makes a
`FILTER` fail the solution and leaves a projected variable unbound, as the Recommendation has it; no error aborts a
query.

### 5.3 Static checks

Every IRI a pattern uses as a predicate or class is resolved against the tenant's catalog before evaluation
(`SPQ0202`, `SPQ0203`), except on a graph whose schema is `@open`, where an undeclared predicate is admitted and its
object typed as any term; a predicate declared as a property is refused in a position where its object must be a node
and the reverse (`SPQ0301`); a literal compared with an attribute of an incompatible datatype is refused at prepare
time (`SPQ0301`) where the attribute's type is known from the predicate, so a misspelt datatype is found before the
query runs. A variable projected that no pattern binds is refused (`SPQ0206`). The result's column datatypes are the
attributes' where a variable is bound only by declared predicates, and any term otherwise; the reply carries them
(section 10).

## 6. Graph patterns and their evaluation

### 6.1 Basic graph patterns

A group of triple patterns over one graph is a basic graph pattern, evaluated as `Query §18.3`: the bag of solution
mappings such that substituting the variables yields triples of the graph, blank nodes in the pattern as variables
that are not projected. Within one `WHERE`, the elements are joined in order (`Query §18.2.2`'s `Join`), `FILTER`s
apply to the group they stand in, and a `GRAPH <g>` block evaluates its group over the named graph `g`. The default
graph is `../schema/SCHEMA.md`'s `@default` graph, so a pattern outside every `GRAPH` block reads it.

This is simple entailment over the view: an `rdf:type` pattern matches the instance's concrete type and every
supertype, since the view holds a triple for each; a navigation property's triple matches each relation instance
once; no other inference is made.

The view holds **live** instances alone (section 1; `../schema/SCHEMA.md` section 8.4, rule 4): no triple of the view
names a deleted or retracted instance, as subject or object, so a retracted membership never matches
`?m a db:MemberOf`, a provenance link of a retracted statement is not answered, and `live` is `true` wherever a
query binds it, until UQL's `with retracted` reads. A read-only transaction at a named read version sees
the instances live at that version.

### 6.2 Property paths [core]

A path is an alternative of sequences of steps, each step a predicate IRI, its inverse `^p`, or either under `+`,
`*` or `?`, or a parenthesised path under them (`Query §9`). Evaluation follows `Query §18.4`: a sequence is a chain
through fresh variables; an alternative a union; `^p` the triple reversed; `p?`, `p+` and `p*` **set** semantics, one
solution per distinct pair of endpoints reachable, with `p*` and `p?` including the endpoint itself, computed by a
frontier expansion that deduplicates nodes per hop and terminates on cycles. Negated property sets `!(…)` and a path
with an unbound predicate are outside the core (section 11). A navigation property of section 2 is a step like any
predicate, so `db:parents+` walks the organisation tree.

### 6.3 `FILTER NOT EXISTS`

`FILTER NOT EXISTS { s p o }` with one triple pattern whose subject is a term or a variable bound by the enclosing
group **[core]**
(`SPQ0401` otherwise) keeps a solution when no triple matches under that solution: the absence of a property, which
DeepBrain's "open" and "live" reads need. A general `NOT EXISTS`, `EXISTS` and `MINUS` are outside the core.

### 6.4 Reifiers

`?r rdf:reifies <<( ?a db:asserted_by ?s )>>` binds `?r` to the reifier of an edge-encoded relation instance with
attributes, and `?r db:method ?m` reads them; `<<( … )>>` holds terms or variables in subject and object and an IRI
as predicate, and may stand only as the object of `rdf:reifies` (`SPQ0402`). A triple term elsewhere, and SPARQL 1.2's
other additions, are outside the core.

### 6.5 Sub-select

`{ SELECT … WHERE … }` inside a group evaluates as `Query §18.2.5` has it and joins its projected variables with the
enclosing group; its modifiers apply inside. One level of nesting is admitted (`SPQ0403`).

### 6.6 Monotonicity and read levels

A query is **monotonic** when it holds no `NOT EXISTS`, no `!`, no `BOUND`, no aggregate and no `ORDER BY` with
`LIMIT`, and no extension function of section 9; such a query may run at the monotonic-fast level the transaction's
Begin names (`../binary_protocol/PROTOCOL.md` section 11.11), the reply flagging it as `SCHEMA.md` section 9.6 states when it read
mutable data. Every other query runs at a snapshot, and the reply says so.

## 7. Solution modifiers and aggregates

### 7.1 Projection

`SELECT ?a (expr AS ?b)` projects; `SELECT *` every variable in scope, in first-mention order; `DISTINCT` removes
duplicate solutions, unbound equal to unbound. A projected name is unique (`SPQ0501`).

### 7.2 Grouping and aggregation

`GROUP BY` keys the solutions as `Query §18.5`; a projection with an aggregate and no `GROUP BY` groups into one
group, and a projected variable that is neither a key nor inside an aggregate is refused (`SPQ0502`). Aggregates
`COUNT`, `COUNT(*)`, `SUM`, `AVG`, `MIN`, `MAX` with `DISTINCT` as `Query §18.5.1`: `COUNT` of the solutions where the
expression is bound, `SUM` and `AVG` over numerics with promotion, an error over a non-numeric, `MIN` and `MAX` by
section 7.3's order; an aggregate over no solution is unbound, `COUNT` 0. `HAVING`, `SAMPLE`, `GROUP_CONCAT` are
outside the core.

### 7.3 Ordering and slicing

`ORDER BY` sorts by its keys as `Query §15.1` and `§18.5.6`: unbound first, then blank nodes, IRIs by code point,
literals by section 5.1 within one datatype; across datatypes in the order numeric, string, boolean, dateTime, date,
time, duration, every other by lexical form **[choice]**; errors after every value. `ASC` is the default. `LIMIT` and
`OFFSET` after ordering, as `Query §15.4`; without `ORDER BY` the order is the store's and stable within one
transaction, and a limited result continues through the protocol's resume key (section 10).

## 8. Updates

An update changes the dataset in the transaction of the request; nothing is visible outside it before the commit,
and every instance the transaction creates or changes is validated whole at commit against the schema (`../schema/SCHEMA.md`
section 8), triples assembled into instances as section 8.1 there states. A refusal aborts the transaction and reaches
the client as section 11 states.

### 8.1 `INSERT DATA`

Ground triples, in the default graph or a `GRAPH` block, each a declared predicate on a declared type:

- A subject's `rdf:type` triple to a concrete type creates an instance of it (`UGS1002` for none, `UGS1003` for an
  abstract type, `UGS1004` for two concrete types); the key attributes must be present (`UGS1006`); the subject's
  IRI must be the one the schema mints for the key (`SPQ0601` otherwise, naming the expected IRI), or a blank node,
  which the engine replaces by the minted IRI, a handle for a keyless type.
- An attribute triple writes the attribute; a `Set` attribute one member per triple; an interval's two bounds; a
  literal of the wrong datatype is refused (`UGS1011`); a second value for a single-valued attribute that holds one
  is refused (`UGS1036`), never a replacement, which `DELETE … INSERT … WHERE` is (section 8.3).
- `?a db:nav ?b` with a declared navigation name of an edge-encoded relation creates its instance, written under
  either navigation name, the inverse normalised to the anchor's triple; attributes of it go on its reifier,
  `_:r rdf:reifies <<( a db:nav b )>>` and `_:r db:attr v`, which the engine binds to the reifier it allocates; on a
  `@multi` relation every navigation triple written creates one instance, the reifier the only name telling the
  instances of one triple apart; a navigation property of a node-encoded relation is refused (`SPQ0602`).
- A node-encoded relation instance is a blank node or handle IRI typed with the relation's class, with one role
  triple per role and its attributes; validated whole at commit (`UGS1015` for a missing required role).
- A `@set` relation's existing combination resolves to the existing instance.

### 8.2 `DELETE DATA`

Ground triples that must exist:

- The `rdf:type` triple of an instance's concrete type deletes the instance: an edge or relation node with the
  relations over it; an entity with live relations naming it under its `@on_delete` actions, `cascade` by default and
  `restrict` refusing (`UGS1020`). On a `@bitemporal` type the delete is a retraction (`../schema/SCHEMA.md` section 8.4),
  `@on_retract` applying.
- An attribute triple clears an optional attribute or one member of a `Set`; a required attribute is refused
  (`UGS1005`); on a `@bitemporal` type the change is a revision.
- A navigation triple of an edge-encoded relation deletes the instance; on a `@multi` relation the bare triple names
  no one instance and is refused (`SPQ0603`), the reifier's `rdf:type` triple deleting the instance it names. A role
  triple of a required role is refused (`UGS1015`), the relation's `rdf:type` triple deletes the instance.
- A triple that does not exist is no error, as `Update §3.1.2` has it; the statement writes nothing for it.

### 8.3 `DELETE … INSERT … WHERE`

The `WHERE` group is evaluated as a query at the transaction's snapshot; for each solution the `DELETE` template's
triples are removed and then the `INSERT` template's added, as `Update §3.1.3` orders them, each triple under
sections 8.2 and 8.1. A template variable unbound in a solution drops that triple. Blank nodes in the `INSERT`
template are fresh per solution. `WITH <g>` sets the graph the templates and the `WHERE` default to. The read and the
write are one read-write transaction: the solutions are the request's read set, validated at commit, so a concurrent
change to what the `WHERE` matched aborts and the client runs the request again. Changing an attribute is `DELETE
{ ?s db:p ?old } INSERT { ?s db:p "new" } WHERE { ?s db:p ?old }`, a revision on a `@bitemporal` type.

Insert-where-absent, the form DeepBrain's writes take in SPARQL: `INSERT { … } WHERE { FILTER NOT EXISTS { <s>
rdf:type db:Person } }` evaluates the `WHERE` to one empty solution when the subject does not exist and inserts
once; the `NOT EXISTS` is recorded as the transaction's absent-term read, so a concurrent creation aborts this one,
and a schema key refuses a duplicate whatever request wrote it.

## 9. Extension functions: vector and text search

The two retrieval operators are **property functions** [choice], triple patterns whose predicate the engine
evaluates:

```
?node ugdb:nearest ( db:embedding $q 8 ?score ) .
?node ugdb:search  ( db:statement "turbine blade" 8 ?score ) .
```

`ugdb:` is `https://w3id.org/ultragraphdb/ns/`. The object list names the attribute's predicate, which must carry
`@index(ann)` or `@index(fulltext)` (`SPQ0702`), the query vector as a `ugdb:float32vector` literal or a parameter,
or the query text, then `k ≥ 1`, then a variable the score binds to. The pattern binds `?node` to the `k` best
instances in order and `?score` to the similarity or BM25 score; it stands as the first pattern of its group
(`SPQ0701`), the rest of the group filtering and expanding it. Both run at a snapshot (section 6.6). Scoping a search
by a preceding pattern is the agentic projection's, at a later stage.

## 10. Transactions, consistency and the protocol

- **A request is a query or an update in a transaction.** The client begins a transaction naming the tenant and,
  for a read-only transaction, its level; sends each request on a stream of its own with its parameters; commits or
  aborts. Section 14 asks the protocol to carry the text.
- **Parameters.** A prepared request names the variables it takes as parameters; execution binds each to a term
  before evaluation, as a `VALUES` row with one solution would, and a parameter unbound at execution is refused
  (`../binary_protocol/PROTOCOL.md` code 11). A parameter in a template or a pattern takes any term; in an `rdf:type` or predicate
  position only an IRI.
- **Read-only and read-write.** A query runs in a read-only transaction at its level, or in a read-write one at the
  partition's latest state; an update needs a read-write transaction (`SPQ0801`). An `INSERT DATA` whose triples the
  catalog classifies coordination-free is what a DirectWrite carries; the frontend decides from the catalog.
- **Results.** A query's result is a stream of columnar batches, one column per projected variable, terms in the
  encoding of `../binary_protocol/PROTOCOL.md` section 10, `ABSENT` for unbound, IRIs by text, reifiers and handles by their IRIs; an
  update's result is the count of triples it inserted and deleted. The reply carries the epoch read at, the monotonic
  flag and, for a limited query, the resume key.
- **Prepared statements.** A request is prepared once per connection with its parameters' types; preparing resolves
  every IRI against the tenant's current catalog version and records the schema id, so a schema change stales it as
  `../binary_protocol/PROTOCOL.md` section 11.7 has it.
- **Partitions.** A read-only query reads across partitions at its snapshot, the planner routing each pattern as the
  quad-reads specification states; an update's reads and writes stay in its transaction's partition in this stage, a
  request reaching another refused as `../binary_protocol/PROTOCOL.md` section 11.11 has it, until stage 8 sequences it.

## 11. Diagnostics and the feature register

### 11.1 Diagnostics

A refusal names a code, a location, a reason and, where one exists, a fix.

| Code | Condition |
|---|---|
| `SPQ0001` | The request past its nesting bound |
| `SPQ0002` | A token the lexer does not know |
| `SPQ0003` | A malformed or relative IRI, or a literal invalid for a datatype of section 5.1 |
| `SPQ0004` | A token the grammar does not expect; the message names what was expected |
| `SPQ0101` | A feature outside the core (section 11.2), named |
| `SPQ0201` | No default graph in the tenant and a pattern outside every `GRAPH` block, or an unknown graph IRI |
| `SPQ0202` | A predicate the catalog does not declare and the view does not derive; the message lists the nearest |
| `SPQ0203` | A class the catalog does not declare |
| `SPQ0206` | A projected variable no pattern binds |
| `SPQ0210` | Warning: a predicate or class IRI under a retired name, resolved during its migration's deprecation window; the message names the current IRI |
| `SPQ0301` | A predicate in a position its kind forbids, or a literal of a datatype incompatible with the attribute's |
| `SPQ0401` | `NOT EXISTS` with more than one triple or an unbound subject |
| `SPQ0402` | A triple term outside the object of `rdf:reifies` |
| `SPQ0403` | A sub-select nested more than one level |
| `SPQ0501` | Two projections with one name |
| `SPQ0502` | A projected variable neither grouped nor aggregated |
| `SPQ0601` | An `INSERT DATA` subject IRI that is not the one the schema mints for its key |
| `SPQ0602` | An insert of a navigation property of a node-encoded relation |
| `SPQ0603` | A `DELETE DATA` navigation triple of a `@multi` relation, which names no one instance |
| `SPQ0701` | A property function pattern that is not the first of its group |
| `SPQ0702` | A property function over an attribute without the index it needs, or an argument of the wrong type |
| `SPQ0801` | An update in a read-only transaction |
| `SPQ1001` | A result or write set past the transaction's bounds (`../binary_protocol/PROTOCOL.md` code 6) |

A schema refusal at commit reaches the client with its `UGS1xxx` number and the instance; a conflict or timeout as
`../binary_protocol/PROTOCOL.md` section 9.4 states.

### 11.2 The feature register

| Feature | Refused as | Stage |
|---|---|---|
| `OPTIONAL` | `optional` | Full conformance |
| `UNION`, `MINUS`, `EXISTS`, general `NOT EXISTS` | `union`, `minus`, `exists` | Full conformance |
| `BIND`, `VALUES` | `bind`, `values` | Full conformance; parameters stand in for `VALUES` |
| Negated property sets, a path with a variable predicate | `property path` | Full conformance |
| An RDF collection outside a property function's object list | `collection` | Full conformance |
| `CONSTRUCT`, `DESCRIBE`, `ASK` | `query form` | Full conformance |
| `FROM`, `FROM NAMED`, `GRAPH ?g` | `dataset clause` | The dataset is the tenant's; `GRAPH ?g` full conformance |
| `HAVING`, `SAMPLE`, `GROUP_CONCAT`, `REDUCED` | `aggregate` | Full conformance |
| Built-in functions beyond section 5.2: string, numeric, date, hash, `IF`, `COALESCE`, `IN`, `REGEX`, casts | `function` | Full conformance |
| `LOAD`, `CLEAR`, `CREATE`, `DROP`, `COPY`, `MOVE`, `ADD`, `INSERT`/`DELETE` with `USING`, `DELETE WHERE` shorthand, update sequences | `update form` | Full conformance; a graph exists by the schema and is dropped by it |
| Triple terms outside `rdf:reifies`, `TRIPLE`, `SUBJECT`, `isTRIPLE` | `rdf-star` | Full conformance |
| Entailment regimes beyond simple entailment | `entailment` | None |
| The SPARQL Protocol over HTTP, the Graph Store Protocol | `protocol` | Full conformance |

## 12. Conformance: the oracles

1. **The W3C SPARQL 1.1 test suite** (`w3c/rdf-tests`): every query and update evaluation test whose query and data lie
   inside the core runs as a result test, the data loaded as `@open` data on a schema that declares nothing, the
   expected solutions compared as bags with unbound as unbound; every test outside the core runs as a refusal test
   naming the feature. The selection is a manifest in the corpus, one line per test, reviewed by hand once. 2.
   **spareval** is the reference evaluator for core queries over the same data where the W3C suite has no case: the
   harness runs a query through Oxigraph's `spareval` on the view's quads and through the core, and compares the bags.
   3. **Differential, against GQL.** Every scenario of section 13 and every MGQL query of `QUERY-GQL-CORE.md` section 12
      is written in both cores over one dataset, and the answers are compared row by row under this correspondence: a GQL
      node is the IRI SPARQL binds; a GQL edge is the reifier IRI; a GQL property is the literal; GQL `NULL` is unbound; a
      GQL quantified edge `+`, `*` or `?` with `DISTINCT` is the path operator of the same name, and a bounded GQL `{m,n}`,
      which SPARQL 1.1 has no path for, runs in a differential scenario over data whose longest chain is within the bound,
      where `{1,n}` is `+` and `{0,n}` is `*`; a GQL `COUNT(DISTINCT x)` is `COUNT(DISTINCT ?x)`; a GQL `IS NULL` is
   `FILTER NOT EXISTS`; three-valued `UNKNOWN` and SPARQL's error both fail the filter.

## 13. DeepBrain in the core

The Gen5 schema (`scrith/openspec/notes/schema/gen5/DeepBrainSchema.ugs`) installed in a tenant as its namespace
`deep_brain`, graph `ontology`, the default graph; `db:` is its vocabulary prefix, implied. The same operations as
`QUERY-GQL-CORE.md` section 13, as DeepBrain writes them in SPARQL where SPARQL states them best.

**Create**, insert-where-absent, DeepBrain's idiom:

```
INSERT {
  <https://example.org/deep_brain#Node/AGENT%3Aada>  a db:Person ;
      db:id "AGENT:ada" ; db:personal_name "Ada" ; db:asserted_by <https://example.org/deep_brain#Node/SRC%3Ahr> .
  <https://example.org/deep_brain#Node/AGENT%3Aacme> a db:Company ;
      db:id "AGENT:acme" ; db:label "Acme" ; db:asserted_by <https://example.org/deep_brain#Node/SRC%3Ahr> .
  _:m a db:MemberOf ;
      db:MemberOf_member <https://example.org/deep_brain#Node/AGENT%3Aada> ;
      db:MemberOf_collection <https://example.org/deep_brain#Node/AGENT%3Aacme> ;
      db:valid_start "2026-01-01T00:00:00Z"^^xsd:dateTimeStamp ;
      db:method "import" ; db:confidence 0.98e0 ;
      db:asserted_by <https://example.org/deep_brain#Node/SRC%3Ahr> .
}
WHERE { FILTER NOT EXISTS { <https://example.org/deep_brain#Node/AGENT%3Aada> a db:Person } }
```

The engine mints the membership's handle for `_:m`; `rank` and `inferred` take the schema's defaults. The subject
IRIs are the ones the schema mints for the keys, which the client computes from `{base}Node/{id}` with the key
percent-encoded, or writes as blank nodes with `db:id` and lets the engine mint.

**Read.** A node by key; a unit's open members with their roles; every unit below a unit through the navigation
property; a count per unit:

```
SELECT ?rank ?since WHERE {
  ?p db:id $id ; db:rank ?rank ; db:asserted_start ?since }

SELECT ?member ?rank WHERE {
  ?u db:id $unit . ?m db:MemberOf_collection ?u ; db:MemberOf_member ?x ; db:rank ?rank . ?x db:id ?member .
  FILTER NOT EXISTS { ?m db:valid_end ?e } }

SELECT DISTINCT ?unit ?label WHERE {
  ?u db:id $unit ; db:units+ ?d . ?d db:id ?unit ; db:label ?label }

SELECT ?unit (COUNT(DISTINCT ?m) AS ?open_memberships) WHERE {
  ?u db:id $unit ; db:units* ?d . ?d db:id ?unit . ?m db:MemberOf_collection ?d .
  FILTER NOT EXISTS { ?m db:valid_end ?e } }
GROUP BY ?unit
```

What the graph held at an epoch, DeepBrain's `believed at`, is a read-only transaction at a named read version
(`../binary_protocol/PROTOCOL.md` section 6.2) over the same queries, within the versions the partitions retain and refused as
`PROTOCOL.md` section 9.4 states past them, until UQL names it in the query.

**Update.** A re-ranking, a revision on the bitemporal membership; a label change on an organisation:

```
DELETE { $m db:rank ?old } INSERT { $m db:rank "preferred" } WHERE { $m db:rank ?old }

DELETE { ?o db:label ?old } INSERT { ?o db:label "Acme Holdings" }
WHERE { ?o db:id "AGENT:acme" ; db:label ?old }
```

**Delete.** Retracting a membership by its handle, and retracting a person:

```
DELETE DATA { $m a db:MemberOf }

DELETE { ?p a db:Person } WHERE { ?p db:id "AGENT:ada" }
```

The first closes the membership's `asserted`; the second retracts Ada and, by `@on_retract`'s default `cascade`, every
bitemporal relation naming her. Her provenance links are not written: `AssertedBy` is `@append_only` and not
bitemporal, so each instance is live while its participants are and leaves the view with her, answered again at an
earlier read version (`../schema/SCHEMA.md` section 8.4, rule 6). `MemberOf`'s `restrict open` refuses an organisation's
retraction while open memberships name it (`UGS1020`).

**Retrieve.** The nearest concepts to an embedding with their schemes; the claims a text query matches:

```
SELECT ?id ?label ?scheme ?score WHERE {
  ?c ugdb:nearest ( db:embedding $q 8 ?score ) .
  ?c db:id ?id ; db:label ?label ; db:scheme ?s . ?s db:label ?scheme }

SELECT ?id ?statement ?score WHERE {
  ?c ugdb:search ( db:statement $q 8 ?score ) . ?c db:id ?id ; db:statement ?statement }
```

**Schema.** Installing and reading the schema are the UGS install and Describe; the vocabulary document and SHACL
shapes `../schema/SCHEMA.md` section 11.2 publishes are what a SPARQL client reads for the terms. What the core leaves to UQL:
`believed at` and `with retracted` in the query, typed trees, the `edge` accessor; to the full-conformance stage:
`OPTIONAL` for "a membership and its role if any", `UNION` across kinds, and `BIND`.

## 14. What this specification asks of the others

- **`../binary_protocol/PROTOCOL.md`.** Carried as of 2026-11-05, as for GQL: Query with the language tag SPARQL (section 7.4 there),
  Prepare and Execute over a text (section 8.1), Result batches with named columns and unbound cells, an update's
  counts, and codes 28 and 29 (section 9.4). Carried 2026-11-08: a warning, `SPQ0210` among them, travels with a
  reply (section 4.9 there).
- **`../schema/SCHEMA.md`.** Carried: section 11.2's RDF view holds the navigation property of section 2 for node-encoded
  relations, a derived `owl:propertyChainAxiom` over the two role properties, read-only, and states that `rdf:reifies`
  with a triple term is how a reifier is reached. Section 8.4's revision rule is what an attribute change on a
  bitemporal instance relies on. Carried 2026-11-05: `live` in section 11.2's view, `UGS1036` for a second value written
  to a single-valued attribute, and section 8.4's rule 6 for an append-only link naming a retracted participant.
- **`mapping-contract`.** Carried 2026-11-08: `../schema/SCHEMA-APG.md`'s definitions 17.1 and 18.1 state the contract, a
  reifier presented through `rdf:reifies` and a triple term among it, and stage 6's change set carries it into the
  capability specification (`SCHEMA.md` section 15).
- **`quad-reads`.** A path step with a bound subject is a forward read; `^p` and a bound object a reverse read; a
  property function a value-index or vector-index lookup over the tenant's partitions.

## 15. Decisions

| Question | Decision |
|---|---|
| The dataset | The tenant's: the `@default` graph and every declared graph; no `FROM` |
| Reifiers | RDF 1.2's `rdf:reifies` with a triple term, the one construct beyond SPARQL 1.1 in the core, since edge properties are otherwise unreachable; the data model is the RDF 1.2 Concepts Candidate Recommendation, the written form the frozen draft syntax of section 1 |
| SPARQL 1.2 | Not followed. The core is SPARQL 1.1, a Recommendation since 2013. SPARQL 1.2 Query and Update are Working Drafts, revised monthly through October 2026, with the Working Group chartered to April 2027 and no implementation report; building on a draft binds the core to text that moves. Decided 2026-11-05: reviewed again in mid or late 2027, against the Recommendation if one exists by then |
| Navigation properties | Derived one-step properties for node-encoded relations, so a property path walks a relation node as it walks an edge; read-only |
| Property paths | Alternatives, sequences, inverse, `+`, `*` and `?` with set semantics by frontier expansion; nothing else until full conformance. SPARQL 1.1 has no bounded `{m,n}` path, so a bounded GQL quantifier has no counterpart here |
| Writes to `@multi` relations | Decided 2026-11-05: every navigation triple inserted creates one instance; a bare navigation triple under `DELETE DATA` names no one instance and is refused, the reifier's type triple deleting one |
| `NOT EXISTS` | One triple pattern with a bound subject: the absence read, recorded in the read set |
| Retrieval | Property functions `ugdb:nearest` and `ugdb:search`, first in their group |
| Insert-where-absent | `INSERT … WHERE { FILTER NOT EXISTS { … } }`, one request, the absence validated at commit |
| Parameters | Named variables bound before evaluation, as a one-row `VALUES`; `VALUES` itself waits |
| Cross-datatype order | Numeric, string, boolean, dateTime, date, time, duration, then by lexical form; errors last |
| Updates on bitemporal types | A revision for an attribute change, a retraction for the type triple's deletion, as `../schema/SCHEMA.md` section 8.4 |
| Oracles | The W3C suite inside the core as result tests and outside it as refusal tests; spareval where the suite has no case; GQL differentially |
