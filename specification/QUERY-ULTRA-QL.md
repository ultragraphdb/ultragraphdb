# The Ultra Query Language (UQL)

* Status: first draft, 2026-11-08. UltraQL follows stage 10 (`ROADMAP.md`).

This file specifies UQL, UltraGraphDB's own query language: the queries, patterns, mutations, transactions and
subscriptions it accepts, their meaning over the graph a UGS schema declares, the typed results they return, how they
are classified and executed, and how a request outside the language is refused. UQL takes every name it uses from the
schema and returns results shaped as the query is shaped: a query is a tree of selections, and its result a typed tree
of the same shape, with no repeated parent values and no regrouping by the client.

UQL is not more expressive than GQL or SPARQL; both can ask what UQL asks. What UQL adds is that the schema's knowledge
shapes the query and its result: names, subtypes and roles are checked before a query runs, n-ary relations and
relations over relations are read through their navigation names, and result types are known at compile time. The two
standard languages stay first-class (`QUERY-GQL-CORE.md`, `QUERY-SPARQL-CORE.md`); all three compile to one kernel and
follow one write classification and one set of read levels.

`SCHEMA.md` gives the schema, its expression grammar, which UQL extends, and its views; `SCHEMA-APG.md` the formal model
a query is evaluated over and its result types are drawn from; `SCHEMA-EVOLUTION.md` how names survive a migration;
`PROTOCOL.md` the frames a request travels in. The design is `openspec/notes/UltraGraphDB Schema and Query Language.md`,
part 2; where this file departs from it, section 22 says so.

## Contents

1. Conventions
2. The graph a query reads
3. Lexical structure
4. Grammar
5. Types and results
6. Queries: selectors, filters and shapes
7. Navigation, relations and paths
8. Explicit patterns
9. Aggregation, ordering and pagination
10. Retrieval
11. Time
12. Mutations
13. Transactions and pipelines
14. Functions and introspection
15. Classification and `explain`
16. Subscriptions
17. The protocol
18. Diagnostics
19. Conformance
20. DeepBrain in UQL
21. What this specification asks of the others
22. Decisions
23. Open questions
24. Sources

## 1. Conventions

| Term | Meaning |
|---|---|
| Tenant, namespace, graph, catalog version, epoch | As `SCHEMA.md` section 1 |
| Element, label, value | As `SCHEMA-APG.md`: an instance, its concrete type, and the record of its attributes and roles |
| Live | An instance that is not deleted and, on a `@bitemporal` type, not retracted; a relation that is not `@bitemporal` is live while every participant of it is (`SCHEMA.md` section 8.4) |
| Shape | A block `{ … }` of fields, the unit a query's result is built from |
| Result type | The type a query's result has, known before it runs (section 5) |
| Read point | The state a request reads: a closed epoch, a named epoch or instant, or the monotonic-fast level (section 11.1) |
| SHALL, refused | A rule the implementation holds; a refusal is a diagnostic of section 18 and no part of the request runs |

Where the design leaves a choice to this file, the choice is marked **[choice]**. A construct that executes only from a
later stage is marked **[stage N]** and refused before it with `UQL0901`, naming the stage.

## 2. The graph a query reads

A request runs against one **working graph** of one tenant: the graph `use` names, else the tenant's `@default` graph
(`SCHEMA.md` section 6.11); a tenant without one and a request without `use` is refused (`UQL0104`). A domain graph is
read together with the shared graphs its schema imports, as the GQL view exposes it (`SCHEMA.md` section 11.3). Every
name a request uses resolves against the catalog version in effect at its read point (`SCHEMA.md` section 13.1); a
retired name of a renamed item resolves during the migration's deprecation window with a warning (`UQL0110`,
`SCHEMA-EVOLUTION.md` section 9).

**The model.** At its read point the working graph is an algebraic property graph `G` on the schema's APG schema
(`SCHEMA-APG.md`, section 6): each instance an element labelled by its concrete type, whose value is the record of its
attributes and roles. A query denotes a function from `G` to a value of its result type:

- A root selector `T(…)` denotes the elements whose label is a concrete type at or below `T` and that satisfy its
  filters: a supertype is the variant of its concrete subtypes (`SCHEMA-APG.md`, section 11).
- A shape denotes a function from an element to a record whose fields are the shape's fields (section 5).
- Result types are **label-free** (`SCHEMA-APG.md`, definition 3.2): an element enters a result only through the
  fields its shape projects, among them `iri`, never as a reference or an internal ID. A result is therefore a closed
  value, meaningful without the graph it was read from.

Reads see the **live slice** by default (`SCHEMA.md` section 8.4, rule 4); the filter items of section 11.2 widen it.
Two reads of one request at one read point see one state, and a query evaluated twice at one read point returns equal
results, up to the order of a list without `order by`.

## 3. Lexical structure

UQL's tokens are UGS's (`SCHEMA.md` section 3): identifiers, literals of every UGS type, including unquoted dates,
date-times, durations such as `P30D`, numbers with `_` between digits, and interval literals, and the comments `//`
and `/* … */`. UQL adds:

| Token | Form |
|---|---|
| Parameter | `$` and an identifier, typed by inference from where it is used (section 5.3) |
| Interval constructor | `e₁ .. e₂` and `e₁ ..`, over expressions as well as literals: closed-open, an open end unbounded |
| Retrieval | `~` before a retrieval operator (section 10) |
| Arrows | `->` and `<-` in explicit patterns (section 8) |

UGS's reserved words are reserved in UQL. UQL's keywords are contextual, as UGS's annotation words are: a keyword in
the position the grammar names, an identifier anywhere else. A request is bounded by the protocol's query section,
`max_frame` less 512 bytes, which `PROTOCOL.md` section 7.4 refuses before the language sees it; nesting of shapes,
parentheses and brackets is bounded at 64 (`UQL0001`).

## 4. Grammar

EBNF as `SCHEMA.md` section 4 writes it. `expr` is `SCHEMA.md` section 4's expression grammar, which UQL extends with
parameters as values, the interval constructor of section 3, the functions of sections 11.3 and 14, and the text
predicates `starts_with(e, s)` and `matches(e, r)` **[choice]**; `function` is its function declaration.

```
request     = [ "use" qident ] [ preamble ] [ readpoint ] body ;
preamble    = "with" function { function } ;
readpoint   = "at" "monotonic" | "as" "of" ( "epoch" int | "time" expr ) ;
body        = query | mutation | pipeline | transaction | explain | subscribe ;

query       = root | match | "query" "{" member { member } "}" ;
member      = ident ":" ( root | match ) ;
root        = selector { "." step } [ retrieval ] { modifier } shape ;
selector    = qident [ filters ] ;                        (* a type, a function or schema.* *)
step        = ident [ filters ] [ bound ] ;
bound       = "+" | "{" int "," int "}" ;
filters     = "(" [ fitem { "," fitem } ] ")" ;
fitem       = expr                                         (* a condition, or an argument of a function *)
            | ident ":" ( expr | selector )                (* a role binding, a named argument *)
            | "is" qident
            | "believed" "at" expr | "with" "retracted" | "open"
            | "valid" ( "at" | "during" ) expr ;
modifier    = "order" "by" sortkey { "," sortkey } | "limit" ( int | param ) | "after" param ;
sortkey     = expr [ "asc" | "desc" ] ;
param       = "$" ident ;

shape       = "{" { field } "}" ;
field       = [ ident ":" ] step { "." step } [ retrieval ] { modifier } [ shape ]
            | ident ":" expr
            | ( "some" | "no" ) step { "." step } [ shape ]
            | "on" qident shape
            | ".." ;

retrieval   = "~" ( single | hybrid ) ;
single      = rkind "(" ident "," expr "," "k" ":" ( int | param ) [ "," "exact" ":" ( "true" | "false" ) ] ")" ;
hybrid      = "hybrid" "(" inner "," inner { "," inner } "," "k" ":" ( int | param )
              [ "," "weights" ":" "[" number { "," number } "]" ] ")" ;
inner       = rkind "(" ident "," expr ")" ;
rkind       = "nearest" | "search" ;

match       = "match" pattern { "," pattern } [ "where" expr ] "return" result ;
pattern     = chain | "try" branch | "or" branch branch { branch } ;
branch      = "{" chain { "," chain } "}" ;
chain       = node { ( "-" ident [ bound ] "->" | "<-" ident [ bound ] "-" ) node } ;
node        = "(" ident [ ":" qident [ filters ] ] ")" ;
result      = ident shape | "{" ident ":" ident shape { ident ":" ident shape } "}" ;

mutation    = write [ "with" "record" param "request" param ] ;
write       = "insert" qident values [ returning ]
            | "upsert" qident filters values [ returning ]
            | "update" selector values [ returning ]
            | "delete" selector
            | "retract" selector [ returning ] ;
values      = "{" [ assign { "," assign } ] "}" ;
assign      = ident { "." ident } ":" ( expr | target | "[" target { "," target } "]" ) ;
target      = selector [ values ] | ident ;               (* ident: a variable of a pipeline or a let *)
returning   = "returning" shape ;

pipeline    = "for" ident "in" ( binding | match ) mutation ;
binding     = selector { "." step } [ retrieval ] { modifier } [ shape ] ;
transaction = "transaction" "{" tstmt { ";" tstmt } [ ";" ] "}" ;
tstmt       = mutation | pipeline | "let" ident "=" write ;
explain     = "explain" ( query | mutation | pipeline | transaction ) ;
subscribe   = "subscribe" query | "resume" "after" param ;
```

A computed field takes a name (`spend: sum(purchases.value)`); a computed field without one is refused (`UQL0108`).

## 5. Types and results

### 5.1 Result types

Every query has a result type, derived from the schema before it runs. Result types are types of `SCHEMA-APG.md`'s
named form without labels, with lists:

| What the query asks | Result type | `SCHEMA-APG.md` type |
|---|---|---|
| A required attribute | Its UGS type `T` (`SCHEMA.md` section 5.1) | A primitive, or a record for intervals, measures and records |
| An optional attribute | `Option<T>` | `optional t` |
| A shape | A record of its fields, named by their aliases or names | `rec φ` |
| A navigation with cardinality exactly one | The participant's record | `rec φ` |
| A navigation with `?` | `Option<Record>` | `optional (rec φ)` |
| A navigation with `*`, a set-valued role | `List<Record>` | A list, part 2's collection |
| A navigation with `+` | A non-empty `List<Record>` | A list with one element at least |
| A supertype with `on` blocks | A tagged union of the subtypes' records, tagged by `type` | `var φ`, one field per concrete subtype |
| `count`; `sum`; `min`, `max`; `avg` | `UInt64`; the attribute's type, widened as section 9.1 states; the attribute's type; `Float64`, or `Decimal` over decimals | A primitive |
| `collect(e)` | `List<T>` of `e`'s type | A list |
| `paths(…)` | `List<Path>`, a `Path` the record `{ nodes: List<Record>, edges: List<Record>, length: UInt32 }` | A record of lists |
| A root query | `List<Record>` of its shape | A list |
| `query { a: …, b: … }` | A record with one list per member | `rec φ` |

A list's order is the `order by` of its level, else the store's, stable within one read point.

### 5.2 Absence and logic

A field that does not apply to an element's type is never in its record: asking for it without `on` is refused
(`UQL0103`). An optional value that is not set is `None`. The two are never confused.

UQL's logic is **two-valued**: a comparison with an absent operand is `false`, and `not`, `and`, `or` are Boolean.
`not (x = 1)` is therefore `true` where `x` is absent, which GQL's three-valued `NOT (x = 1)` is not; the differential
correspondence of section 19 maps UQL's `not p` to GQL's `p IS NOT TRUE`. `is some` and `is none` test presence.

### 5.3 Static typing

Names, types, roles and cardinalities are resolved before a request runs. A name the catalog does not declare is
refused with the nearest declared ones (`UQL0101`); a field declared only on a subtype, with the `on` block that would
admit it as the fix (`UQL0103`); an operator applied to types it does not take (`UQL0201`). A numeric literal takes the
type of the field it is compared with or assigned to, `0.98` a `Float64` for a `Float64` attribute, where its value is
representable in that type, and is refused otherwise (`UQL0201`). A parameter takes the type
of every place it is used; places that disagree, or a parameter whose type no place fixes, are refused (`UQL0202`).
Comparisons across numeric widths, units and temporal kinds follow `SCHEMA.md` sections 5.1, 5.5 and 5.8; a
`LocalDateTime` is never compared with a `DateTime` (`UGS0223`).

## 6. Queries: selectors, filters and shapes

### 6.1 Root selectors

| Selector | Selects |
|---|---|
| `T` | Every live instance of `T` and its subtypes; on a supertype, one range scan per range of its prefix set (`SCHEMA.md` section 7.1) |
| `T(filters)` | Those satisfying every filter item |
| `R(role: selector, …)` | Instances of the relation type `R` whose roles are bound as named: `Supply(buyer: Company(name = $c))` selects what `purchases` reads from that company |
| `f(args)` | The result of a schema function or query-local function (section 14) |
| `schema.types`, … | The catalog graph (section 14.2) |

A filter whose items fix a key, a `@unique` attribute or `iri` by equality is a point lookup; the planner routes it to
the instance's home without a scan (`SCHEMA.md` section 7.3), which keeps a read-write request in its transaction's
partition.

### 6.2 Filters

Filters stand in parentheses on a root selector or on any navigation step; commas mean `and`. An item is an expression
of `SCHEMA.md`'s grammar, evaluated against the element, a role binding, `is T`, which narrows the element to `T` for
the items after it, or a bitemporal or validity item (section 11.2). Inside a filter a name resolves against the
element first and the enclosing instance second (`SCHEMA.md` section 4).

| Kind | Examples |
|---|---|
| Comparison | `=`, `<>`, `<`, `<=`, `>`, `>=` |
| Membership and range | `status in [active, stored]`, `commissioned between 2010-01-01 and 2015-12-31` |
| Text **[choice]** | `starts_with(serial, "EN-")`, `matches(serial, $re)` |
| Type test | `item is Engine` |
| Quantified paths | `some parts(is Engine, thrust_kn > 300)`, `no certification`, `all parts(status = active)` |
| Presence | `firmware is some`, `firmware is none` |
| Boolean | `and`, `or`, `not` |
| Temporal | Section 11.3 |

`matches` takes a regular expression of linear-time dialect, without backreferences or lookaround **[choice]**, so its
cost is bounded by its input.

### 6.3 Shapes

| Field | Example | Result |
|---|---|---|
| Attribute | `serial` | The value; `Option` if optional |
| Built-in field | `type`, `iri`, `asserted`, `live`, `open` | `SCHEMA.md` section 5.11; `type` the concrete type's name, `iri` the instance's IRI |
| Navigation | `owner { name }` | A record, `Option` or list by cardinality (section 7) |
| Function call | `spend(2026-01-01)`, `engines { serial }` | As declared |
| Alias | `registration: tail_number` | The field under the alias |
| Computed field | `age: today() - commissioned` | The expression's value, here a `Duration` |
| All attributes | `..` | Every attribute the type declares, required and optional |
| Subtype block | `on Avionics { firmware }` | The block's fields for elements of that subtype; the result becomes a tagged union |
| Requirement | `some sales(…) { … }`, `no purchases(…)` | Section 6.4 |

An instance's internal ID is never a field; keys and `iri` are its only external identities.

### 6.4 Required and excluded branches

`some nav(…) { … }` keeps a parent only if at least one child matches, and returns the matching children; it is
monotonic. `no nav(…)` keeps a parent only if no child matches, returns nothing for it, and is non-monotonic. Without
either keyword, a navigation that matches nothing returns an empty list or `None`, and the parent stays.

## 7. Navigation, relations and paths

### 7.1 Navigation names

A navigation name is a field of the participant type that gains it (`SCHEMA.md` section 6.4), read in either direction
of its relation; the compiler knows the role each name belongs to.

- **A binary relation's navigation** yields the participant at the other end. Inside its shape, `edge` is the relation
  instance traversed: `edge.since` reads one attribute, and `edge { since  asserted_by { … } }` reads the instance as a
  record with its own attributes and navigations, which is how a relation over a binary relation is read.
- **An n-ary relation's navigation** yields the relation instances; their other roles are fields of the instance.
- The encoding of a relation, edge or node (`SCHEMA.md` section 11.1), never changes what its navigation yields.
- A set-valued role yields a list of its participants.
- A navigation through a relation supertype reaches every subtype; `on` adds a subtype's fields, and a specialized
  role answers to its own name and to the super role's.
- A derived name (`SCHEMA.md` section 6.8) is read as a navigation or attribute of its declared type.

### 7.2 Bounded and transitive steps

`nav+` and `nav{m,n}` expand a navigation transitively and return the distinct elements that end some chain within the
bounds: reachability, one result per distinct end element, terminating on cycles. The evaluation is `QUERY-GQL-CORE.md`
section 6.3's: below the lower bound each level is a set of its own, so `{3,3}` from `a` over `a → b`, `b → a`,
`a → c` returns `{b, c}`; from the lower bound on, an element already returned is not expanded again. Filters precede
the bound and apply at every step: `parents(open){1,32}`. `{0,n}` includes the start element.

### 7.3 Path enumeration

`paths(nav+, to: selector)` and `paths(nav{m,n}, …)` return paths, each a `Path` record (section 5.1). Over a relation
declared `@acyclic` every path is acyclic, and no mode is written. Over any other relation a mode is required,
`mode: trail`, `acyclic` or `simple`, as GQL defines them, and a request without one is refused (`UQL0301`), since
enumeration can grow exponentially. `shortest(nav+, to: selector)` returns one shortest path, and with `all: true` every
shortest path; it requires `to` (`UQL0302`).

## 8. Explicit patterns

Shapes cover trees. Where two branches must meet, a cycle or a join, an explicit pattern binds variables, and
navigation names label its edges:

```
match (c: Company) -sales-> (s) -item-> (e: Engine),
      (c) -purchases-> (p) -item-> (e)
return c { name }
```

- A variable reused across chains is a join. `where expr` relates variables of different chains, `s.signed <
  p.signed` **[choice]**.
- `try { … }` matches a sub-pattern if it can; a variable bound only inside it has an `Option` type in the result.
- `or { … } { … }` matches any of its branches; a variable used after it must be bound in every branch (`UQL0402`).
- A pattern's bindings are matched as GQL's `REPEATABLE ELEMENTS` and `WALK` match them (`QUERY-GQL-CORE.md` section
  6.1). The result holds one record per distinct binding of the variables `return` names **[choice]**, so a returned
  element appears once however many matches reach it, as a tree result has no repeated data.
- `return v { … }` returns a list of `v`'s shape; `return { a: v { … }, b: w { … } }` a list of records.

`or` is monotonic; `try` is not, since a later match can turn an absent branch into a present one.

## 9. Aggregation, ordering and pagination

### 9.1 Aggregation by nesting

An aggregate in a shape is computed per parent: nesting is grouping, and the first version has no `group by`.

```
Company {
  name
  contracts_2025: count(purchases(signed >= 2025-01-01))
  spend:          sum(purchases.value)
  largest:        max(purchases.value)
}
```

`count`, `sum`, `avg`, `min`, `max` and `collect` aggregate the values a path reaches; `count` counts elements, the
others the path's last field. `sum` over integers takes `Int128` and over decimals the widest decimal, refusing an
overflow at run time (`UQL1001`); `min` and `max` refuse a type without an order (`UQL0203`). Over no value, `count` is
0, `collect` the empty list, and the others `None`. Aggregates are non-monotonic.

### 9.2 Ordering and pagination

`order by`, `limit` and `after $cursor` apply at any level, the root and every navigation. `order by` sorts by its keys
in turn, `asc` by default, `None` last ascending and first descending **[choice]**, as the GQL core orders nulls. A
cursor is the opaque value the previous page returned: it fixes the read point and the position, so pages are stable
while new data arrives, and it carries no internal ID (section 21). A cursor past the retention of its read point is
refused as `PROTOCOL.md` code 3 states.

## 10. Retrieval

Retrieval operators select by relevance, at the root or on any navigation, so the graph scopes the candidates before
anything is ranked:

```
Concept ~ nearest(embedding, $q, k: 8) { id  label  score  scheme { label } }

Scheme(id = $scheme) {
  concepts ~ nearest(embedding, $q, k: 5) { id  label  score }
}
```

| Operator | Selects by |
|---|---|
| `~ nearest(attr, $vec, k: n)` | Vector similarity under the attribute's `@index(ann)` metric |
| `~ search(attr, $text, k: n)` | BM25 relevance under the attribute's `@index(fulltext)` tokenizer |
| `~ hybrid(nearest(…), search(…), k: n)` | Several rankings fused by reciprocal rank fusion with constant 60 **[choice]**, or by weighted scores with `weights: [w₁, …]`, one weight per ranking |

- The candidates are the elements the selector or navigation reaches with its filters; the operator returns the `k`
  best of them, best first, and `score` in the shape reads each one's score.
- Over an approximate index the result is the index's approximate top `k`; `exact: true` scores every candidate
  exactly **[choice]**, which a scoped navigation with few candidates makes cheap.
- The attribute must carry the index its operator needs (`UQL0701`); the query value must have the attribute's
  dimension or type (`UQL0702`); `weights` has one weight per ranking (`UQL0703`).
- Retrieval is non-monotonic and runs at a snapshot. `@index(ann)` executes from **[stage 8]** (`SCHEMA.md` section 14);
  scoped retrieval with access-control relations, context shaping and agent write-back are the agentic projection's
  (`openspec/notes/UltraGraphDB Agentic Context Projection.md`).

## 11. Time

### 11.1 Read points

| Clause | Reads |
|---|---|
| None | A snapshot at the latest closed epoch; with a session token, at least the epoch of the session's last write |
| `at monotonic` | The monotonic-fast level (`PROTOCOL.md` section 11.11); refused for a query that is not monotonic (`UQL0410`) |
| `as of epoch e` | A snapshot at epoch `e` |
| `as of time t` | A snapshot at the last epoch closed before `t` |

A read point is the protocol's read version and level (`PROTOCOL.md` section 6.2). `as of` before the retention window
of a type that is neither `@bitemporal` nor `@append_only` is refused naming the window (`PROTOCOL.md` code 3); for
those two classes every past point is answered (`SCHEMA.md` section 8.4). A read point before a namespace's migration
cutover reads the source version (`SCHEMA-EVOLUTION.md` section 7). Every result carries the epoch it was read at and,
for a monotonic-fast read of mutable data, the flag `SCHEMA.md` section 9.6 states.

### 11.2 Bitemporal and validity filters

`SCHEMA.md` section 8.4's filter items, on a root selector or a navigation of a `@bitemporal` type:

- `believed at t` reads the version current at `t`, retracted instances included;
- `with retracted` reads every instance and version;
- `open` keeps live instances whose valid interval has no end;
- `valid at t` keeps instances whose valid interval contains `t`, and `valid during i` those whose valid interval
  intersects `i`, on a type with `@valid` or `@bitemporal(valid: …)`.

`as of` fixes the database's state; `believed at` reads the statements' transaction time within that state. A read of
`believed at` a past instant is immutable, and so monotonic.

### 11.3 Temporal values and operators

- **Intervals.** `a .. b` is closed-open and `a ..` open-ended; an interval has `start`, `end`, an `Option` absent when
  open, and `duration`, absent when open. Durations are ISO 8601: `P30D`, `P1M`, `PT12H`.
- **Arithmetic.** `date + P1M`, `t₂ - t₁` (a `Duration`), `period.start + P7D`.
- **Points and intervals.** `t in i` holds when `i.start <= t < i.end`.
- **Interval relations.** Allen's thirteen relations and the shorthands `intersects`, `disjoint` and `within`, with the
  `certainly` and `possibly` forms over bounds of coarser precision, exactly as `SCHEMA.md` sections 4 and 5.5 define
  them; the plain form means `possibly`.
- **Functions.** `intersection(a, b)`, an optional interval; `span(a, b)`; `date(t)`; `start_of_day(d, zone)`;
  `distinct(a, b)`, which compares two optional values treating absence as a value; `today()` and `now()`, the date and
  time of the read point's epoch close, so a monotonic-fast read uses its replica's epoch.
- **Execution.** An interval operator on an attribute with `@index(interval)` is a range scan on the indexed bounds;
  temporal predicates are comparisons and leave a query's monotonicity unchanged.

## 12. Mutations

### 12.1 Entities

```
insert Engine { serial: "EN-4471-B", commissioned: 2026-10-30, status: active, thrust_kn: 310.5 }
update Asset(serial = "EN-4471-B") { status: stored }
update Asset(status = stored, commissioned < 2000-01-01) { status: retired }
upsert Company(name = "Leonardo") { }
delete Asset(serial = "EN-4471-B")
retract Holds(holder: Person(id = $p), post: Post(id = $post))
```

| Statement | Writes |
|---|---|
| `insert T { … }` | One instance of the concrete type `T` (`UQL0501` for an abstract one); required fields and roles must be given or defaulted, which the commit checks (`UGS1005`, `UGS1015`); a keyless type's handle IRI is minted (`SCHEMA.md` section 7.3) |
| `upsert T(key) { … }` | The instance the key names, inserted if absent and updated if present; the filter must fix `T`'s whole key by equality (`UQL0506`) |
| `update S { … }` | Every instance `S` selects; on a `@bitemporal` type each update is a **revision**, a new version of the same instance (`SCHEMA.md` section 8.4); an `@append_only` type is refused (`UQL0503`) |
| `delete S` | Every instance `S` selects, under `@on_delete`; on a `@bitemporal` type `delete` is refused with `retract` as the fix (`UQL0504`) **[choice]** |
| `retract S` | Closes `asserted` of every instance `S` selects, under `@on_retract`; on a type that is not `@bitemporal` refused (`UQL0505`) |

Writes to a derived name are refused at compile time (`UQL0502`); every other rule is the commit's, refused with its
`UGS1xxx` number (`SCHEMA.md` section 8.7).

### 12.2 Relations and nested inserts

A role is assigned a participant by a selector that resolves to **at most one** instance: one whose filter fixes a key,
a `@unique` attribute or `iri` by equality, which PG-Keys' exclusivity makes single (`SCHEMA.md` section 8.2). A
selector that may resolve to several is refused (`UQL0507`); one that resolves to none fails the commit (`UGS1017`).

```
insert Supply {
  supplier: Company(name = "Leonardo"), buyer: Company(name = $buyer), item: Asset(serial = $s),
  signed: 2026-10-30, value: 1_250_000.00
} returning { iri  signed }

delete Owns(asset: Asset(serial = $s))
```

- An n-ary instance is inserted with all its roles in one statement. An existing combination of a `@set` relation
  resolves to the existing instance.
- **Nested inserts** assign navigation names of the inserted type, creating the relations they belong to in the same
  statement: `owner: Company(name = $c) { edge.since: 2026-11-01 }` creates an `Owns` with its attribute, and
  `parts: [Asset(serial = "EN-1"), Asset(serial = "EN-2")]` one relation instance per participant. A navigation of a
  relation that names the inserted instance, such as `asserted_by` on any statement, writes provenance with the
  statement it describes.
- `returning { shape }` returns any shape of the written instances, read at the commit's epoch.

## 13. Transactions and pipelines

### 13.1 Transactions

- **Every request is atomic.** `transaction { s₁; s₂; … }` groups several writes into one atomic unit sent whole, which
  the engine can sequence without waiting on the client. `let r = insert …` binds an inserted instance for the
  statements after it, as a keyless instance has no key to select it by.
- **Interactive transactions** are the protocol's: a client begins a transaction (`PROTOCOL.md` section 6.2), sends
  several UQL requests, and commits. Reads see the transaction's snapshot; writes are validated at commit.
- **One path per transaction.** The classifier takes the most demanding path among its statements (`SCHEMA.md`
  section 9.2). Until **[stage 8]**, a read-write transaction stays in its partition, and a request reaching another is
  refused as `PROTOCOL.md` section 11.11 states.
- **Dependent reads** **[stage 8]**: a transaction whose writes depend on its reads across groups is run first as a
  reconnaissance, then sequenced with a check that its read set is unchanged, and retried automatically if it changed.

### 13.2 Pipelines

A pipeline feeds one query into one write, in one atomic request:

```
for s in Supply(value > 10_000_000, no certification)
insert Certifies { authority: Authority(name = "EASA"), contract: s, valid: $validity }
returning { contract { iri  value } }
```

- The read stage runs at one snapshot; each result binds the variable, and the write runs once per binding. The whole
  pipeline commits or nothing does.
- The commit path is decided from the write stage's types before the read runs, as any write's is.
- A pipeline whose writes exceed the transaction's write set (`PROTOCOL.md` section 11.9) is refused before commit
  (`UQL0602`); large backfills run as batches with `limit` and `after`.
- A write stage feeding another stage, and a read stage that observes the pipeline's own writes, are outside the
  language (`UQL0601`).

## 14. Functions and introspection

### 14.1 Functions

Schema functions (`SCHEMA.md` section 6.9) are called as navigation names or root selectors. A `with` preamble declares
functions for one request, under the schema's rules: pure, deterministic, typed, non-recursive across the request's
functions, inlined into the plan, and refusing `today()` and `now()` as a schema function does (`UGS0225`). A
query-local function never shadows a schema name (`UQL0105`).

```
with fn Company.big_sales() -> Supply* = sales(value > 1_000_000)

Company { name  spend(2026-01-01)  big_sales order by value desc limit 3 { value  item { serial } } }
```

### 14.2 Introspection

The catalog is the read-only `schema.` graph of `SCHEMA.md` section 10.3, queried like any other: `schema.types`,
`schema.attributes`, `schema.relations`, `schema.roles`, `schema.navigations`, `schema.enums`, `schema.functions`,
`schema.derivations`, `schema.graphs`, `schema.quantities`, `schema.units` and `schema.report`, each with `doc`, the
`///` text. It shows what the principal may read and never a prefix, bucket or ID.

```
schema.types(name starts_with "A") {
  name  doc  abstract  supertype { name }
  attributes  { name  type  optional  doc }
  navigations { name  relation { name }  cardinality }
}
```

## 15. Classification and `explain`

Every name is resolved before a request runs, so monotonicity is decided from the query's syntax and combined with the
schema's mutability facts (`SCHEMA.md` section 9.6):

| Construct | Monotonic |
|---|---|
| Selectors, comparisons, navigation, `some`, `all`, `exists`, `nav+`, `nav{m,n}`, `paths(…)`, `or` in patterns | Yes |
| `no`, `not`, `is none`, `try` in patterns | No |
| Aggregates | No |
| `order by` with `limit`, retrieval operators | No |
| `shortest(…)` | No |
| The live slice of a `@bitemporal` type | No: a retraction removes a result |
| `believed at` a past instant | Yes: that slice never changes |

A function call is monotonic exactly when its body is; a navigation through a relation supertype reads every subtype,
so its mutability is the most mutable subtype's.

| Query | Read level | Guarantee |
|---|---|---|
| Touches one group | That group's latest state | Exact |
| Monotonic, reading only `@append_only` and `@immutable` data | Monotonic-fast available | Every result true and lasting; recent facts may be missing |
| Monotonic, reading mutable data | Monotonic-fast available, flagged | Every result true at a recent closed epoch |
| Otherwise | A snapshot | Exact as of the epoch |

`explain` returns, without running the request, a record of its read level, the partitions or groups it reaches,
whether it is monotonic and what mutability it reads, and for every write its commit path: coordination-free,
segmented with the constraint that segments it, or coordinated (`SCHEMA.md` section 9.2).

## 16. Subscriptions

**[stage after the protocol carries a client feed; section 21]**

```
subscribe Organization(id = $org) { all_units { id } }
resume after $cursor
```

- `subscribe q` returns `q`'s answer at an epoch with a cursor, then every commit record after that epoch that writes
  a type `q` reads. Epochs give an exact cut, with no gap and no overlap between the answer and the records.
- Records are commit records filtered by type, not re-evaluated answers; incremental maintenance of a query is a
  different feature.
- `resume after $cursor` continues from a cursor; one older than the log's retention is refused as unknown.
- `with record $r request $id` on a write attaches a client record and deduplicates a retry with the same request ID
  through the protocol's client sessions.

## 17. The protocol

- **Requests.** A UQL request is a Query (`PROTOCOL.md` section 7.4) or a Prepare and Execute (section 8.1) with
  `language` 3, its text and its parameters by name; a prepared request's reply names each parameter's inferred type
  and the result type.
- **Results.** A root query's result is a stream of Result batches (`PROTOCOL.md` section 9.1), one row per root
  element and one column per root field. Until the protocol carries nested columns, a field holding a record, a list
  or a tagged union travels as an `rdf:JSON` literal whose leaves are typed terms **[choice]**; nested columns, records
  as struct columns, lists as list columns and unions as union columns, compatible with Arrow's layout, are asked of
  the protocol (section 21). A `query { … }` request returns one stream per member, in order.
- **Refusals.** A request outside the language is refused with code 29 and the diagnostic's number, the language in the
  detail's bits 16 to 23 (`PROTOCOL.md` section 9.4); a schema rule at commit with code 7 and its `UGS1xxx` number.
- **Typed clients.** A client library checks requests against the schema at build time and generates their result
  types, a tagged union as an enum; the Rust client's form is a macro over the schema source.

## 18. Diagnostics

A refusal names a code, a location, a reason and, where one exists, a fix, in the compiler style of `SCHEMA.md` section
10.4:

```
error[UQL0103]: field `firmware` is not defined on `Asset`
  --> query:4:5
   |
 4 |     firmware
   |     ^^^^^^^^ defined only on subtype `Avionics`
   |
help: narrow the type
   |
 4 |     on Avionics { firmware }
```

| Code | Condition |
|---|---|
| `UQL0001` | The request past its nesting bound |
| `UQL0002` | A token the lexer does not know |
| `UQL0003` | A malformed literal |
| `UQL0004` | A token the grammar does not expect; the message names what was expected |
| `UQL0101` | A name the catalog does not declare; the message lists the nearest declared ones |
| `UQL0103` | A field declared only on a subtype, asked without `on`; the fix names the `on` block |
| `UQL0104` | No working graph: no `use` and no `@default` graph, or an unknown graph |
| `UQL0105` | A query-local function shadowing a schema name |
| `UQL0106` | A pattern variable unbound where used, or bound to two types |
| `UQL0107` | `on T` where `T` is no subtype of the shape's type |
| `UQL0108` | A computed field without a name |
| `UQL0110` | Warning: a retired name, resolved during its migration's deprecation window |
| `UQL0201` | An operator, function or comparison applied to types it does not take |
| `UQL0202` | A parameter whose uses disagree on its type, or whose type no use fixes |
| `UQL0203` | An aggregate over a type it does not take |
| `UQL0301` | Path enumeration over a relation that is not `@acyclic` without a mode |
| `UQL0302` | `shortest` without `to` |
| `UQL0303` | A bound `{m,n}` with `m > n` |
| `UQL0402` | A variable used after `or` that some branch does not bind |
| `UQL0410` | `at monotonic` on a query that is not monotonic; the message names the construct |
| `UQL0501` | `insert` of an abstract type |
| `UQL0502` | A write to a derived name |
| `UQL0503` | `update` of an `@append_only` type |
| `UQL0504` | `delete` of a `@bitemporal` type; the fix is `retract` |
| `UQL0505` | `retract` of a type that is not `@bitemporal` |
| `UQL0506` | `upsert` whose filter does not fix the type's whole key |
| `UQL0507` | A role selector that may resolve to several instances |
| `UQL0601` | A pipeline form outside the language |
| `UQL0602` | A pipeline past the transaction's write set |
| `UQL0603` | A write in a read-only transaction |
| `UQL0701` | Retrieval over an attribute without the index its operator needs |
| `UQL0702` | A retrieval value of another type or dimension than the attribute's |
| `UQL0703` | `weights` whose count is not the number of rankings |
| `UQL0901` | A construct before the stage that executes it; the message names the stage |
| `UQL1001` | Integer overflow at run time |
| `UQL1002` | Division by zero at run time |
| `UQL1003` | A result past the transaction's bounds (`PROTOCOL.md` code 6) |

## 19. Conformance

1. **Differential, against the query cores.** A corpus of UQL queries over the DeepBrain schema and the GQL core's
   MGQL graphs is translated into the GQL core and the SPARQL core and run over one dataset. A UQL result is flattened
   into rows by unnesting each list level and taking each record's fields as columns; the corpus keeps to queries whose
   every navigation is single-valued or under `some`, so flattening is an inner join, which both cores express. UQL's
   `not p` corresponds to GQL's `p IS NOT TRUE` and to SPARQL's `!(p)` over a bound value; a UQL `iri` is the IRI both
   cores return.
2. **Type soundness.** Every result conforms to the result type the compiler inferred, checked on every corpus query
   and on generated queries; a mismatch is a defect of the compiler or the engine.
3. **Classification.** Every corpus query's monotonicity and read level are compared with section 15's tables, and
   every monotonic query is checked to return a subset of its answer after further inserts.
4. **The formal model.** The result-type rules of section 5 and the shape semantics of section 2 are stated over
   `SCHEMA-APG.md`'s Lean model, once its named layer exists, so the compiler's inference is checked against them.

## 20. DeepBrain in UQL

The Gen5 schema (`scrith/openspec/notes/schema/gen5/DeepBrainSchema.ugs`) installed as namespace `deep_brain`, graph
`ontology`, the default graph.

**Read.** A node with who asserted it; a unit's open members with their roles; every unit below a unit; a statement's
topics with the confidence and provenance of each `IsAbout`:

```
Node(id = $id) {
  id  label  rank  asserted
  asserted_by { on Source { id  label }  on Person { id  personal_name } }
}

Organization(id = $unit) {
  label
  members(open) order by valid.start desc {
    valid  rank
    member { on Person { id  personal_name }  on Organization { id  label } }
    role { id  label }
  }
}

Organization(id = $unit) { all_units order by label { id  label } }

Information(id = $id) {
  about { type  iri  on Node { id }  edge { confidence  method  asserted_by { on Source { id } } } }
}
```

`asserted_by` is the navigation `AssertedBy` gives every statement, `?` in Gen5, so it reads as an `Option` of a tagged
union over `Source` and the concrete agent types; `about` reaches a `Statement`, which declares no `id`, so `on Node`
reads it where the topic is a node; `members` reads `MemberOf` instances, an n-ary relation; `all_units` is the
materialized closure of `SubOrganizationOf`; `about` is binary, so `edge` reads the `IsAbout` instance itself.

**What was believed.** A node as the graph held it at an instant, and its memberships, retracted ones included:

```
Node(id = $id, believed at $t) { label  rank  asserted }
Person(id = $p) { memberships(with retracted) { collection { id }  valid  asserted  live } }
```

**Retrieve.** The nearest concepts to an embedding with their schemes, and the claims a text query matches:

```
Concept ~ nearest(embedding, $q, k: 8) { id  label  score  scheme { label } }
Claim ~ search(statement, $text, k: 8) { id  statement  score }
```

**Write.** A person and a membership, each with its provenance, in one transaction; a re-ranking, which is a revision;
a retraction:

```
transaction {
  insert Person { id: "AGENT:ada", personal_name: "Ada", asserted_by: Source(id = "SRC:hr") };
  insert MemberOf {
    member: Person(id = "AGENT:ada"), collection: Company(id = "AGENT:acme"),
    valid: 2026-01-01T00:00:00Z .., method: import, confidence: 0.98,
    asserted_by: Source(id = "SRC:hr")
  } returning { iri }
}

update MemberOf(iri = $membership) { rank: preferred }
retract Person(id = "AGENT:ada")
```

`inferred` and `rank` take the schema's defaults. Retracting Ada closes her `asserted`, cascades to the bitemporal
relations naming her, and leaves her append-only provenance links as written, out of the live slice with her
(`SCHEMA.md` section 8.4, rule 6).

**Backfill.** Provenance for every claim that has none, as a pipeline:

```
for c in Claim(no asserted_by) limit 1000
insert AssertedBy { statement: c, asserter: Source(id = "SRC:backfill") }
```

## 21. What this specification asks of the others

- **`PROTOCOL.md`.** `language` 3 for UQL in Query and Prepare; the inferred parameter and result types in Prepare's
  reply; nested result columns, records, lists and tagged unions, compatible with Arrow's layout; an opaque cursor for
  `after` that carries no internal ID; a client-facing feed for `subscribe` and `resume`, with commit records filtered
  by type, of which the protocol today holds only the internal epoch feed between roles; `with record … request …` as a
  client session's deduplicated request; `explain` as a request that compiles and classifies without executing.
- **`SCHEMA.md`.** Section 4's expression grammar names this file as its extension for queries, as it does; section
  5.11's `edge` as a field with a shape inside a binary navigation's shape.
- **`SCHEMA-APG.md`.** Part 2's list collection, which `List` result types and set-valued roles read; part 3's views,
  which the differential correspondence of section 19 relies on; the Lean model's named layer, for section 19's
  fourth check.
- **`QUERY-GQL-CORE.md`, `QUERY-SPARQL-CORE.md`.** The differential correspondence of section 19, from UQL's side.

## 22. Decisions

| Question | Decision |
|---|---|
| Formal ground | A query denotes a function from the working graph's algebraic property graph to a label-free value of its result type (section 2); results carry IRIs, never internal IDs |
| Logic | Two-valued, as the design has it; `not p` corresponds to GQL's `p IS NOT TRUE` (section 5.2) |
| `update` on a `@bitemporal` type | A revision, as `SCHEMA.md` section 8.4 decided on 2026-11-05; the design's refusal of `update` is superseded |
| `delete` on a `@bitemporal` type | Refused with `retract` as the fix **[choice]**: UQL has `retract`, so one operation has one spelling. The GQL core reads `DELETE` as a retraction because GQL has no `retract` |
| Keyless instances | Named by their handle IRIs (`SCHEMA.md` section 7.3), which the design had postponed; `let` binds one inside a transaction |
| Provenance and relations over binary relations | Written through navigation names in nested inserts, read through `edge` with a shape (sections 7.1 and 12.2) |
| Existence by `contains` | Written `some nav(…)`, since `contains` is Allen's relation in UGS's grammar; the design's `parts contains Engine(…)` is not adopted |
| Text predicates | `starts_with` and `matches` in the first version **[choice]**; the rest of the string, numeric and vector library in the next, as the design decides for its library |
| Explicit patterns | One result per distinct binding of the returned variables **[choice]**; `where` across chains **[choice]** |
| Graph selection | `use g` before the request, else the tenant's `@default` graph |
| Results on the wire | `rdf:JSON` for nested values until the protocol carries nested columns **[choice]** |
| Grouping | By nesting only; no `group by` in the first version |
| Pipelines | One read stage into one write stage; multi-stage pipelines not adopted |

## 23. Open questions

- What does `subscribe` read now that P24 gives each fact one owner and no change streams: a feed the frontend
  assembles from the partitions' commits at each epoch close, or one per partition merged by the client?
- Is the reciprocal rank fusion constant fixed at 60, a parameter of `hybrid`, or the tenant's setting?
- Does `matches` take a named dialect, such as RE2's syntax, so that every client library validates a pattern alike?
- Does a later version add `group by` beside grouping by nesting, and over which keys?
- Does `try` in patterns, non-monotonic today, gain a monotonic reading when its variables are not returned?
- When the protocol carries nested columns, does the `rdf:JSON` encoding of section 17 remain for clients that do not
  read them?

## 24. Sources

- `openspec/notes/UltraGraphDB Schema and Query Language.md`, part 2: the design this file specifies.
- `specification/SCHEMA.md`: the expression grammar (section 4), types (section 5), built-in fields (section 5.11),
  constraints (section 8), bitemporal statements (section 8.4), classification (section 9), introspection (section
  10.3), views (section 11).
- `specification/SCHEMA-APG.md`: the model a query is evaluated over, its types and values.
- `specification/SCHEMA-EVOLUTION.md`: retired names and read points across a cutover.
- `specification/QUERY-GQL-CORE.md`, `specification/QUERY-SPARQL-CORE.md`: the standard languages' cores, the
  quantified-edge evaluation UQL's bounded steps share, and the differential oracles.
- `specification/PROTOCOL.md`: Query, Prepare and Execute, Result, read versions and levels, refusals.
- `openspec/notes/UltraGraphDB Agentic Context Projection.md`: scoped retrieval and context shaping.
- J. F. Allen, *Maintaining Knowledge about Temporal Intervals*, Communications of the ACM 26(11), 1983.
- G. V. Cormack, C. L. A. Clarke, S. Büttcher, *Reciprocal Rank Fusion Outperforms Condorcet and Individual Rank
  Learning Methods*, SIGIR 2009.
- TypeQL's language guide and keyword glossary (TypeDB 3), for pipelines and their limits.
