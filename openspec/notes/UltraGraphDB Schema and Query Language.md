# UltraGraph Schema (UGS) and Ultra Query Language (UQL)

Nov 1, 2026 · @Marvin Hansen

This document specifies the two languages that are specific to UltraGraphDB. **UltraGraph Schema (UGS)** is the data definition language: it defines hyper-multigraphs once for GQL, SPARQL and UQL, and declares every invariant so writes and queries are classified for coordination avoidance automatically. **Ultra Query Language (UQL)** is the typed query language that follows from it: a query has the shape of its result, and the schema supplies types, cardinalities and roles, so queries stay short and are checked before they run. GQL and SPARQL remain first-class and fully standard; UQL is an addition, not a replacement. Companion documents: [UltraGraphDB Distribution Architecture](https://claude.ai/artifact/W7YBsh2yPKCLjehZiEsFZD) and [UltraGraphDB Single-Container Deployment](https://claude.ai/artifact/Lj3ztjXbdbyWW98ZGE49qG).

# Why UGS and UQL

The two languages serve different purposes, and it helps to separate them.

**UGS makes every language faster and safer to run.** Most of what the schema declares is used by the planner and the kernel, whichever language a query arrives in. Keys become point lookups, cardinalities give exact estimates, acyclicity simplifies paths, colocation avoids epoch waits, and writes are classified into the cheapest safe commit path. GQL and SPARQL get all of this while staying fully standard.

**UQL hands the schema's guarantees to whoever writes the query.** It is not more expressive than GQL or SPARQL; both can ask essentially any question UQL can. What UQL adds is that the schema's knowledge shapes the query and its result, not only the plan.

| What the schema knows | GQL and SPARQL | UQL |
| --- | --- | --- |
| Cardinalities and nesting | Used for planning; results are flat tables, so nested data repeats parent values on every row and clients regroup it | Results are typed trees of the query's shape, with no repeated data and no regrouping |
| Types of every field | Result shape known to the client only at runtime | Result types known at compile time; typed clients are generated and schema changes that break a query fail the build |
| Names, subtypes and roles | A misspelled predicate or a field asked of the wrong subtype usually returns an empty result | Rejected at compile time with a suggested fix |
| N-ary relations and roles | Written through their encoding: relation nodes with role edges, or role triples | Read directly through navigation names; an n-ary instance is inserted with all its roles in one statement |
| Consistency and path safety | Requested through session options outside the query grammar | Checked against the query: `at monotonic` on an aggregate and unbounded paths over cyclic relations are compile errors |
| Text, vector and graph retrieval | Extension functions | Built-in operators with defined semantics |

**Who gains most.** Application code gains typed, nested results and build-time breakage instead of runtime surprises. AI agents gain the most: their typical failure is a plausible query that silently returns nothing, and UQL turns that into a compile error with a fix they can act on, while tree-shaped results fit directly into context with no redundant rows.

**What the standard languages keep.** GQL and SPARQL remain the right choice for portability, existing tools and drivers, standards conformance and teams that already know them. That is why both stay first-class and UQL is an addition, not a replacement.

**Purpose-built, not general-purpose.** UltraGraphDB is built for DeepBrain: massive, multi-domain, deeply interconnected knowledge that must be actionable for counterfactual reasoning and agentic systems. That is why the schema is executable. One set of declarations decides the types queries are checked against, the commit path of every write, the guarantees reads can offer, the placement of data and the standard views, so correctness and cost are known before anything runs. Workloads that do not depend on a shared ontology are not design targets.

# Part 1: UltraGraph Schema (UGS)

## Design principles

1. **One source of truth.** UGS defines the graph. The GQL graph type, RDF vocabulary, SHACL shapes, catalog, write and query classification, and placement rules are all generated from it and versioned together.
2. **Declare invariants; never infer them.** Every constraint is explicit, so classification is exact rather than heuristic.
3. **Standard queries stay standard.** No UGS construct requires non-standard syntax in GQL or SPARQL. The schema only gives the engine facts it can use.
4. **Identity everywhere.** Every entity and every relation instance has identity, which is what makes multigraphs, hyperedges and provenance possible.
5. **Closed by default, open on request.** Data that does not match the schema is rejected. Types marked `@open` accept undeclared attributes, for ingestion staging and open-world RDF data, at the cost of losing classification benefits for those attributes.
6. **Errors at compile time.** The compiler rejects schemas whose constraints cannot be enforced, and warns when a constraint forces coordination that a placement change would avoid.

## Why a custom DDL

GQL graph types and SHACL each cover part of what the engine needs, but neither covers it all: n-ary relations with named roles, relations whose instances take part in other relations, control over multi-edges, acyclicity, delete semantics, mutability and data placement. The trade is deliberate: one custom DDL in exchange for standard-conformant queries that the engine can often run faster than a schema-less engine could.

## Data model: hyper-multigraphs

- **Entities** are typed nodes with single inheritance and abstract types.
- **Attributes** are typed values attached to entities and relations. Named attribute types are identified by value: two entities with the same `Serial` share one value, so "which other entities have this value" is a lookup, not a scan.
- **Relations** connect two or more participants through named **roles**. Every relation instance has its own identity (a statement ID), which gives three capabilities:
  - **Multigraphs:** a relation declared `@multi` may hold several instances between the same participants, each with its own attributes.
  - **Hyperedges:** an n-ary relation connects any number of roles in one instance.
  - **Relations over relations:** a relation instance can fill a role in another relation.
- **Derived relations** are declared closures or compositions, maintained by the engine and read-only to clients.
- **Graphs** are named instances of a schema, mapped to GQL graphs and RDF named graphs.

## Lexical structure

- **Source** is UTF-8. Comments are `//` and `/* */`; documentation comments `///` become `rdfs:comment` in RDF and descriptions in the catalog.
- **Identifiers:** types and relations in `UpperCamelCase`, attributes, roles and navigation names in `lower_snake_case`. The convention is enforced by the linter, not the grammar.
- **Reserved words:** `schema`, `module`, `version`, `extends`, `namespace`, `import`, `attribute`, `abstract`, `statement`, `entity`, `relation`, `derive`, `graph`, `enum`, `record`, `quantity`, `unit`, `fn`, `as`, `Optional`, `exists`, `order`, `by`, `true`, `false`.
- **Literals:** integers with `_` separators, decimals, strings in double quotes, dates (`2026-11-01`), date-times with offsets, ISO 8601 durations (`P30D`) and IRIs in angle brackets.

## Type system

| UGS type | Meaning | GQL view | RDF view |
| --- | --- | --- | --- |
| `Bool` | Boolean | `BOOLEAN` | `xsd:boolean` |
| `Int8` to `Int128`, `UInt8` to `UInt64` | Signed and unsigned integers | Matching exact numeric types | `xsd:byte` to `xsd:integer`, unsigned variants |
| `Float32`, `Float64` | Binary floating point | `FLOAT32`, `FLOAT64` | `xsd:float`, `xsd:double` |
| `Decimal(p, s)` | Exact decimal | `DECIMAL(p, s)` | `xsd:decimal` |
| `String` | UTF-8 text | `STRING` | `xsd:string` |
| `Text` | Natural-language text with an optional language tag; eligible for full-text indexing | `STRING` | `rdf:langString` or `xsd:string` |
| `Bytes` | Binary data | `BYTES` | `xsd:base64Binary` |
| `Date`, `Time`, `DateTime`, `LocalDateTime`, `Duration` | Temporal values; `DateTime` carries an offset | `DATE`, `ZONED TIME`, `ZONED DATETIME`, `LOCAL DATETIME`, `DURATION` | `xsd:date`, `xsd:time`, `xsd:dateTime`, `xsd:dateTime`, `xsd:duration` |
| `Iri` | An IRI value | `STRING` | IRI term |
| `enum Name { a, b }` | Closed set of named values | `STRING` with a value check | One IRI per member |
| `List<T>`, `Set<T>` | Ordered list; unordered set without duplicates | `LIST<T>` | `List`: an `rdf:JSON` literal; `Set`: one triple per member |
| `Interval<T>` | Closed-open interval over an ordered type; the end may be open (see Time) | `RECORD` with `start` and `end` | Two literals, `{attribute}_start` and `{attribute}_end`, so SPARQL can compare them |
| `record Name { ... }` | Named structured value | `RECORD` | `rdf:JSON` literal |
| `Vector<BF16, n>`, `Vector<F32, n>` | Embedding of fixed dimension | `LIST<FLOAT32>` | Custom datatype literal |

**Optionality.** Optionality is written out as a keyword after the type: `T Optional` may be absent, and anything without `Optional` is required.

```
notes:    Text Optional
firmware: String Optional
```

The keyword is spelled out, rather than a `?` suffix, so that optionality (may this value be missing?) can never be confused with the cardinality markers on navigation names (how many relation instances may a participant take part in?). There are no implicit nulls. In the GQL view an absent value reads as null, as GQL requires; in UQL it is `None`.

**Named attribute types and `@independent`.** Values of a named attribute type are shared: entities holding the same value refer to one value. By default a value disappears when its last holder releases it. `@independent` keeps it, for vocabularies that should exist before and after anything uses them, such as a controlled list of tags: `attribute Tag = String @independent`. Removing orphaned values is derived-data maintenance, so it is coordination-free.

**`@values`** restricts an attribute to listed values without declaring an enum: `grade: String @values("A", "B", "C")`. It is shorthand for a single-use `enum` and maps the same way.

## Entities and identity

```
abstract entity Asset @key(serial) {
  serial:       Serial
  commissioned: Date
  notes:        Text Optional @index(fulltext)
}
entity Aircraft : Asset { tail_number: String @unique }
```

- **Inheritance is single.** A subtype inherits all attributes and roles of its supertype and may add constraints to inherited attributes (for example make an optional attribute required), but never remove them. `abstract` types have no direct instances. Multiple inheritance is not supported, not even for abstract types used only as interfaces, because a type test would then no longer be a check of its type's prefix ranges.
- **System identity.** Every entity and relation instance receives a 128-bit system ID: a 16-bit type prefix, a 24-bit bucket, a 64-bit sequence number and 8 reserved bits (decided 2026-11-01). The bucket takes 8 bits from a hash of the namespace and 16 from a hash of the declaring type's prefix and the key, so creation and the key's uniqueness check meet in one group; data without a key hashes its IRI. A bucket is a virtual partition that the placement map assigns to a replica group, so rebalancing moves buckets and never changes an ID. Sequences are unique per type, leased to groups in blocks without a round trip per ID, and never reused. IDs never leave the database: no query, result, IRI or cursor reveals them. How entities without a key, and relation instances, are named from outside (an **opaque handle**) is postponed beyond the first version, as personal data is.
- **Type prefixes.** A type's prefix is allocated when the type is first declared, in declaration order, and never changes. A supertype covers the prefixes of its descendants: one range when they are declared together, a few once later versions or modules add subtypes. A type test checks those ranges (decided 2026-11-01).
- **Business identity.** `@key` (on one attribute, or at entity level for a composite key such as `@key(country, registration)`) defines the external identity. Keys are required and immutable, are unique across the type and all its subtypes, and are used to mint IRIs.
- **Types are fixed for life.** Because the type is part of the ID, an entity cannot change type. A change of type is a delete and a create, which the schema makes explicit rather than silent.

## Relations

```
relation Supply @multi {
  supplier: Company -> sales*
  buyer:    Company -> purchases*
  item:     Asset   -> contracts*
  signed:   Date
  value:    Decimal(18, 2)
}
```

**Roles.** A relation has two or more roles. Each role declares a name, a participant type and a **navigation name** with a cardinality. The navigation name is the field the participant type gains (`Company.sales`); its cardinality limits how many instances of this relation one participant may take part in through that role.

| Role form | Meaning |
| --- | --- |
| `role: Type -> nav card` | Exactly one participant per instance |
| `role: Type Optional -> nav card` | Optional participant: an instance may leave this role empty |
| `role: [Type] -> nav card` | Set-valued role: one or more participants per instance, for example the members of a team |
| `role: RelationType -> nav card` | The participant is a relation instance (a relation over a relation) |

**Set and multi semantics.**

- `@set` (the default): at most one instance per combination of participants. Inserting an existing combination returns the existing instance, so inserts are idempotent; attributes of the instance can be updated.
- `@multi`: every insert creates a new instance with its own identity, so repeated contracts between the same companies coexist, each with its own attributes.

**Symmetric relations.** `@symmetric` applies to binary relations whose two roles have the same participant type and share one navigation name, so (a, b) and (b, a) are the same instance:

```
relation PeerOf @symmetric { a: Company -> peers*  b: Company -> peers* }
```

**Relation attributes** are declared like entity attributes and may carry `@key`, which gives instances a business identity (for example a contract number).

**Navigation semantics.** What a navigation name yields depends on arity:

| Relation | Navigation yields | Relation attributes reached through |
| --- | --- | --- |
| Binary | The participant at the other role (`Asset.owner` yields an `Organization`) | `edge`, the instance traversed to reach it (`edge.since`) |
| N-ary | The relation instances (`Company.sales` yields `Supply` instances) | The instance directly; its other roles are fields of the instance (`buyer`, `item`) |

This rule keeps binary relations as convenient as property-graph edges while giving hyperedges first-class instances.

## Relation inheritance

Relations inherit from relations, as entities inherit from entities. This lets one query cover a whole family of relation kinds, lets one constraint govern the family, and lets new kinds be added later without changing queries written against the supertype.

```
abstract relation Agreement @multi {
  party:        Organization -> agreements*
  counterparty: Organization -> counterparty_agreements*
  signed:       Date
}

relation Supply : Agreement @append_only {
  supplier: Company as party        -> sales*
  buyer:    Company as counterparty -> purchases*
  item:     Asset                   -> contracts*
  value:    Decimal(18, 2) @check(value > 0)
}

relation Lease : Agreement {
  lessor: Organization as party        -> leases_out*
  lessee: Organization as counterparty -> leases_in*
  asset:  Aircraft                     -> leases*
  term:   Interval<Date>
}
```

`Organization.agreements` now returns supplies and leases alike, while `Company.sales` returns only supplies. A supplier is also a `party`, so a query written against `Agreement` keeps working when a third kind of agreement is added.

| Aspect | Rule |
| --- | --- |
| Inheritance | Single; `abstract` relations have no direct instances |
| Inherited roles | A subrelation has every role of its supertype; it keeps each one as declared or specializes it once with `as` |
| Role specialization | `name: SubType as superrole -> nav card` renames the role, narrows its participant type to the same type or a subtype, and adds a navigation name. A participant in the specialized role is also a participant in the super role |
| Added roles | A subrelation may add roles, so a binary supertype can have n-ary subtypes; queries over the supertype ignore the added roles |
| Attributes | Inherited; a subtype may add constraints, never remove them |
| Navigation names | A supertype's names return instances of every subtype; a subtype's names return only its own |
| Cardinality | A bound on a supertype's navigation name counts instances of all subtypes; a subtype may add a tighter bound on its own |
| `@set` or `@multi` | Fixed at the root of the hierarchy; subtypes cannot change it |
| Mutability | A subtype may add `@append_only`, never remove it |
| Deletes | A subtype may tighten `cascade` to `restrict`, never the reverse |
| `@acyclic` on a supertype | Applies across all subtypes together |
| Placement | Inherited, so constraints over the family stay checkable in one group |
| Identity | Relation types get type-prefixed statement IDs; a supertype covers its descendants' prefix ranges (see Type prefixes) |

**Acyclicity across relation kinds.** Declaring `@acyclic` on an abstract supertype makes a hierarchy built from several relation kinds acyclic as a whole, which no single-relation constraint can express:

```
abstract relation Contains @acyclic @colocate(root) {
  inner: Asset -> container?
  outer: Asset -> contents*
}
relation PartOf      : Contains { part:   Asset    as inner -> assembly?
                                  whole:  Asset    as outer -> parts* }
relation InstalledIn : Contains { device: Avionics as inner -> bay?
                                  host:   Aircraft as outer -> installed* }
```

Here `container?` also means an asset sits in at most one container, whether as a part or as installed equipment.

**Classification.** A constraint declared on a supertype applies to the union of its subtypes, so the conflict graph includes writes to every subtype. The mechanism is unchanged; the compiler simply sees more operations per constraint. Inherited placement keeps those operations in one group.

**Validation.** The compiler checks that each specialized participant type is the super role's type or a subtype of it, that no super role is specialized twice in one subtype, that navigation names stay unique per participant type across the hierarchy, and that subtypes only tighten inherited constraints.

## Derived relations

```
derive Asset.all_parts     = parts+                       // transitive closure
derive Asset.near_parts    = parts{1,3}                   // bounded depth
derive Company.suppliers   = purchases.supplier           // composition through an n-ary relation
derive Supply.certified    = certification exists         // filtered boolean
```

- **Read-only.** Clients query derived relations like any other; only the engine writes them.
- **Maintained incrementally** through change streams, which makes their maintenance coordination-free. Derived updates are stamped with the epoch of the write that caused them, so a snapshot sees base data and derived data at the same point.
- **Materialization.** `@materialized` (the default for closures over `@acyclic` relations) stores the result; `@virtual` (the default otherwise) expands it at query time. Closures over relations that may contain cycles use set semantics, so they always terminate.

**Folds.** A derive may also fold over a navigation to compute state from history: `last`, `first`, `count`, `sum`, `min` and `max`, with optional filters and an ordering, for example `derive Parameter.current: Float64 = last(changes order by day.date).value`. Folds are materialized and maintained incrementally, like closures; reading them is non-monotonic, because the folded value can change as occasions arrive.

## Functions

Functions are named, parameterized queries stored in the schema.

```
fn Aircraft.engines() -> Engine* = all_parts(is Engine)
fn Company.spend(since: Date) -> Decimal(18, 2) = sum(purchases(signed >= since).value)
fn certified_suppliers() -> Company* = Company(some sales(exists certification))
```

- **Two forms.** Method functions (`Type.name`) are used in UQL like navigation names: `engines { serial }`, `spend(2026-01-01)`. Free functions are used as root selectors: `certified_suppliers() { name }`.
- **Typed.** Parameters and the return type, including its cardinality, are declared, and the body is checked against them.
- **Pure and non-recursive.** Bodies are deterministic, and the call graph must be acyclic in the first version; `nav+` and `derive` cover most recursive needs.
- **Inlined, not materialized.** Calls are expanded into the calling query's plan, so the planner optimizes across them. A result that should be stored and maintained is a `derive` instead.
- **Classified like their bodies.** A call is monotonic exactly when its body is.
- **Versioned with the schema.** UQL can also define query-local functions with a `with` preamble (Part 2).

User-defined functions are separate from the built-in standard library (string, numeric and vector functions), which is planned for the next version. Temporal types and operations are part of the core language in version 1 (see Time).

## Constraints

| Constraint | Syntax | Meaning | Write path |
| --- | --- | --- | --- |
| Key | `@key`, `@key(a, b)` | Required, unique across the type tree, immutable | Segmented: unique index partitioned by key, owning group checks |
| Unique | `@unique` | Unique when present | Segmented, as for keys |
| Required value | no `Optional` | Must be present at commit | Coordination-free: checked on the written instance |
| Value check | `@pattern(re)`, `@range(min, max)`, `@values(...)`, `@check(expr)` | Condition on one instance's own values; expressions must be deterministic | Coordination-free |
| Participation cardinality | `-> nav ?`, `+`, `{m, n}` | How many instances one participant may take part in | Segmented: instances colocated with the constrained participant; coordinated if two constrained participants live in different groups |
| Referential integrity | implicit | Participants must exist | Coordination-free with `cascade`; coordinated with `restrict` |
| Acyclicity | `@acyclic` | No cycle through the relation's two roles | Segmented with `@colocate(root)`: rank-based local check; coordinated across hierarchies |
| Bounded aggregate | `@limit(sum(value) <= n per buyer)` | Aggregate over a participant's instances stays within a bound; `count` and `sum` in the first version | Segmented via escrow shares |

## Mutability

| Annotation | Applies to | Meaning | What it enables |
| --- | --- | --- | --- |
| (default) | Attributes, entities, relations | Mutable | Nothing extra |
| `@immutable` | Attribute | Set once at creation, never changed | Coordination-free writes; values safely cached |
| `@append_only` | Entity or relation type | Instances are inserted, never updated or deleted | Monotonic-fast reads that never return a false fact; no deletion checks; relations to append-only entities never dangle |

**Legal erasure.** Append-only data can still be removed for legal reasons through an administrative purge. A purge is a coordinated, logged operation that opens a new purge epoch; monotonic-fast reads never combine data from both sides of a purge boundary.

## Delete semantics

`@on_delete(role: action)` states what happens to relation instances when the participant in that role is deleted.

| Action | Effect | Allowed when | Write path |
| --- | --- | --- | --- |
| `cascade` (default) | The relation instance is deleted, and so are instances of relations over it | Always | Coordination-free; cascades travel on change streams in deterministic order |
| `restrict` | The participant cannot be deleted while instances exist | Always | Coordinated, unless the instances are colocated with the participant |
| `orphan` | The instance stays, with the role empty | Only for optional roles (`Type Optional`) | Coordination-free |

The compiler rejects `orphan` on a required role, and warns when `restrict` will force coordination that `@colocate` on the same role would avoid.

## Placement

Placement decides which replica group stores what, and therefore which constraints can be checked in one group.

- **Default.** In the current KV stage, an entity's attributes live in its bucket's group, and a relation instance with its first role's participant; a reverse entry follows by change stream to the other participant's group, and a heavily referenced node's reverse list is split across a few groups (Shard-per-Core note, P3, P17). In the later tile stage, placement follows the GSL grid.
- **`@colocate(role)`** partitions a relation by the group of the participant in that role instead of its first role's participant, so constraints on that participant are checked locally. The relation's other navigation names then read across the groups of their participants; the coordination report lists those scatter reads. Example: `asset: Asset -> owner?` with `@colocate(asset)` puts each ownership instance in its asset's group.
- **`@colocate(root)`** applies to `@acyclic` hierarchies: each tree lives in its root's group. Moving a subtree to a different root is a coordinated move.
- **Automatic placement.** For every segmented constraint without an explicit annotation, the compiler chooses the colocation that makes it local and reports the choice. When two constraints demand conflicting placements, it keeps one local, routes the other through coordination and warns.
- **Unique indexes** are always partitioned by key hash, so each key has exactly one owning group.

## Indexes

| Annotation | Index | Notes |
| --- | --- | --- |
| `@index(range)` | Ordered secondary index | Range filters and ordering |
| `@index(fulltext)` | BM25 inverted index | Analyzer chosen by language tag on `Text` values |
| `@index(ann, metric: cosine, quantization: int8)` | Approximate nearest-neighbour vector index | Metrics `cosine`, `dot`, `l2`; quantized search with BF16 rescoring of top candidates |
| implicit | Key and unique indexes | Created for every `@key` and `@unique` |

Indexes are derived data: maintained through change streams, coordination-free, and consistent with base data at every snapshot epoch.

## Namespaces and IRIs

- **Vocabulary terms.** `namespace av = "https://example.org/aviation#"` sets the base for schema terms: types (`av:Aircraft`), attributes (`av:serial`), navigation names (`av:assets`, with `owl:inverseOf` pairing both directions) and n-ary roles (`av:Supply_buyer`).
- **Instance IRIs.** Entities with a key get `{namespace}{Type}/{key}`, `Type` being the type that declares the key, so a subtype's instances share their supertype's IRI space and the key can be read back from the IRI; entities without a key get `{namespace}{Type}/{handle}` and relation instances a reifier IRI `{namespace}rel/{handle}`, the handle's scheme postponed (System identity). `@iri("template")` overrides the pattern per type; the compiler refuses a template from which the key cannot be read back. System IDs never appear in IRIs.
- **External vocabularies.** `import "https://schema.org/" as schema` and `@maps_to(schema:name)` align a UGS term with an external one (exported as `owl:equivalentProperty` or `owl:equivalentClass`), so existing ontologies and SPARQL clients can use familiar terms.

## Graphs and shared graphs

Multi-domain knowledge is organized as several **domain graphs** whose nodes are disjoint by design, connected only through a few **shared graphs** such as an ontology, space and time. Connections between domains are therefore always explicit and always typed: a domain never links directly to another domain, only to a shared graph that both reference.

```
graph ontology  : ontology @shared @replicated
graph time      : temporal @shared @anchor(leaf: Day)
graph space     : spatial  @shared @anchor(leaf: Cell)

graph cfd       : cfd
graph causality : deepcausality
```

| Rule | Meaning |
| --- | --- |
| Disjoint domain graphs | Every entity belongs to exactly one graph. A relation between two domain graphs is a compile error |
| Bridge relations | A relation in a domain graph may have roles whose participant types belong to a shared graph's schema (imported with `import schema`). Such relations are stored in the domain graph |
| `@shared` | Marks a connection graph that domain graphs may reference |
| `@anchor(leaf: Type)` | Bridges into this shared graph may target only the named leaf type; coarser levels (week, month, region) are reached through the shared graph's own hierarchy |
| `@replicated` | A small, read-mostly shared graph (typically the ontology) is copied to every group, so traversals and checks against it are always local; its rare changes are coordinated |
| Partitioned shared graphs | Large shared graphs (time, space) are partitioned by leaf ID range like any other data |

**Avoiding supernodes.** Shared graphs are where supernodes form: every occasion on a given day points at that day. Three rules keep them bounded:

1. **Leaf anchoring.** Bridges attach to the finest level only (a day, not a year; a cell, not a continent), which spreads the references over many nodes. Aggregate levels are reached through the shared graph's hierarchy, typically a materialized `derive` closure, not through stored edges.
2. **Reverse adjacency partitioned by domain.** The inverse of a bridge (`Day.parameter_changes`) is stored with the referencing domain's data, not on the shared node. A busy day has one bounded slice per domain group, never one global list.
3. **Tiles later.** In the tile stage, remaining hot shared nodes are split across the 2D tile grid like any other high-degree node.

**Coordination.** Shared graphs for time and space are append-only: days and cells are created, never deleted. The existence of an append-only node is a monotonic fact, so once seen it can never be invalidated. Inserting a bridge to an existing shared node is therefore coordination-free, even when the shared node lives in another group. Only changes to a replicated ontology are coordinated, and they are rare by nature.

**Standard views.** In RDF each graph is a named graph; bridge triples live in the domain's named graph and reference IRIs in the shared graphs, so SPARQL queries span them naturally. In GQL, where an edge belongs to one graph, each domain graph is exposed together with the shared graphs it references as one GQL graph.

## Modelling occasion-primary knowledge

In an occasion-primary model, what happened is primary and state is derived from it. UGS expresses this directly: occasions are append-only relations anchored in shared time, and state is a fold over occasions that the engine maintains.

```
schema cfd version 1
import schema temporal
import schema ontology

entity Simulation @key(run_id) { run_id: String }
entity Parameter  @key(name)   { name: String }

/// An occasion: a parameter changed in a simulation, on a day, under a concept.
relation ParameterChange @multi @append_only @colocate(parameter) {
  parameter:  Parameter        -> changes*
  simulation: Simulation       -> parameter_changes*
  day:        temporal.Day     -> parameter_changes*
  concept:    ontology.Concept -> parameter_changes*
  value:      Float64
  reason:     Text Optional
}

/// State is a fold over occasions, maintained by the engine.
derive Parameter.current: Float64 = last(changes order by day.date).value

/// State at any valid time.
fn Parameter.value_at(d: Date) -> Float64 Optional =
  last(changes(day.date <= d) order by day.date).value
```

| Property | Consequence |
| --- | --- |
| Occasions are `@append_only` | Writes are coordination-free; history read monotonic-fast never contains a false fact; history is never rewritten |
| State is a `derive` fold | Maintained incrementally through change streams; always consistent with the occasions at the same snapshot epoch |
| Two clocks | Valid time comes from the occasion's anchor in shared time (`day`); transaction time is the epoch. `value_at(d)` asks what was true on day `d`; `as of epoch e` asks what the database knew at epoch `e`. Together they give bitemporal queries |
| History is monotonic, state is not | Queries over occasions can run monotonic-fast; queries over current state run at a snapshot epoch |

Shared time also joins domains without any direct link between them:

```
temporal.Day(date between $from and $to) {
  date
  parameter_changes { parameter { name }  value  reason }   // cfd graph
  releases { version  summary }                             // causality graph
}
```

## Time

Time is central to occasion-primary knowledge, so temporal types, constraints and operators are part of the core language in version 1, not the deferred standard library.

### Temporal types

| Type | Values | Notes |
| --- | --- | --- |
| `Date` | A calendar day |  |
| `DateTime` | An instant with a UTC offset | Compared in UTC |
| `LocalDateTime` | Wall-clock time without an offset | Not comparable with `DateTime`; mixing them is a compile error |
| `Time` | A time of day |  |
| `Duration` | ISO 8601, with a calendar part (`P1M`) and an exact part (`PT36H`) | Months and years use calendar arithmetic |
| `Interval<T>` | Closed-open `[start, end)` over `Date`, `DateTime` or a numeric type | The end may be open, meaning "until further notice" |

Mixing granularities, such as comparing a `Date` with a `DateTime`, requires an explicit conversion (`date(t)`, `start_of_day(d, zone)`), so granularity mistakes are compile errors rather than silent truncations.

### Declaring valid time

```
/// Which value a simulation used for a parameter, and when.
relation ValueUse @valid(usage) @no_overlap(per: [simulation, parameter])
                  @colocate(simulation) {
  simulation: Simulation     -> value_uses*
  parameter:  Parameter      -> uses_of*
  value:      ParameterValue -> uses*
  usage:      Interval<Date> @index(interval)
}
```

| Annotation | Meaning | Write path |
| --- | --- | --- |
| `@valid(attr)` | Names the interval attribute holding the valid time of an entity or relation, which enables `valid at` and `valid during` in UQL | No effect |
| `@no_overlap(per: [roles])` | For each combination of participants in the named roles, valid intervals never overlap (here: a simulation uses one value per parameter at any time) | Segmented, like a cardinality constraint: instances colocated with those participants |
| `@index(interval)` | Interval index for overlap and containment queries | Derived data; coordination-free |

`today()` and `now()` are not allowed in `@check` or `derive`, because they are not deterministic. In queries they return the time of the snapshot epoch, which keeps every query replayable under `as of`.

### Three time axes

Value provenance in multi-physics simulation needs three separate time axes: when a value was **used**, when a status change was **recorded**, and the interval over which that change is **effective**, which may begin before it was recorded (a retroactive invalidation). All three are ordinary declarations:

```
enum StatusKind { valid, invalidated, superseded, convention_change }

/// A change in a value's status, recorded on a day, effective over an interval.
relation StatusChange @multi @append_only @colocate(value) {
  value:     ParameterValue          -> status_changes*
  recorded:  temporal.Day            -> status_changes*
  successor: ParameterValue Optional -> predecessor?
  effective: Interval<Date>
  kind:      StatusKind
}

/// Status of a value on valid date t, as known on date k.
fn ParameterValue.status_at(t: Date, k: Date) -> StatusKind Optional =
  last(status_changes(t in effective, recorded.date <= k) order by recorded.date).kind
```

The domain's own knowledge time (`recorded`) is separate from the database's transaction time (the epoch), so `as of epoch e` can still reconstruct what the database held, independently of what the domain had recorded.

### Standard views

`Interval<T>` maps to a `RECORD` with `start` and `end` in GQL, and to two literals, `{attribute}_start` and `{attribute}_end`, in RDF. Interval operators in UQL translate to comparisons on those bounds, so the same temporal questions can be asked in GQL and SPARQL.

### Precision and certainty

Temporal knowledge is often stated to a precision: a membership may begin on a known day and end in a known year. `Interval<T>` over a temporal type therefore carries a precision per bound (millennium, century, decade, year, month, day, hour, minute, second, millisecond, microsecond, nanosecond). A bound stated to a precision stands for the whole unit it names: `2026-09` is the month of September 2026. At the end of a closed-open interval, a bound stated to a precision ends within its unit: in
`2026-09-14 .. 2027` the outer extent ends at the end of 2027 and the inner extent at its start.

```
2026-09-14 .. 2027              // starts on a known day, ends in a known year
2026-09                         // an instant known to the month: the whole month
2026-09 ..                      // open end
```

An imprecise interval has an outer extent (earliest possible start to latest possible end) and an inner extent (latest possible start to earliest possible end). Every interval operator therefore comes in two forms:

| Form | Holds when |
| --- | --- |
| `a possibly R b` | `R` holds for at least one completion of the imprecise bounds |
| `a certainly R b` | `R` holds for every completion |
| `a R b` | Same as `possibly` |

```
Asserted(valid certainly contains 2026-09-28T14:00:00Z)
Event(valid possibly overlaps 2026-07-01 .. 2026-10-01)
```

`@index(interval)` stores both extents, so both forms are range scans. Offsets are kept on the value and compared in UTC. `@no_overlap` refuses intervals that certainly overlap; `@no_overlap(strict)` also refuses intervals that possibly overlap.

## Statements: one root for entities and relations

Many relations accept "any node or relation" at one end (assumptions, verification, supersession, citation, derivation), and every node and relation carries the same envelope. A third kind of declaration, `statement`, provides one root for both:

```
abstract statement Statement @key(id) {
  id:          String
  uncertainty: Uncertainty Optional
}
abstract entity   Node     : Statement { }
abstract relation Relation : Statement @multi {
  subject: Statement -> out*
  object:  Statement -> in*
}
```

- A statement type is always abstract, has attributes and constraints, and has no roles. It may extend another statement type; an entity type or a relation type may extend a statement type. Inheritance remains single throughout.
- A role may name a statement type, so it accepts both entities and relation instances below it. A navigation name declared on a statement type is gained by every entity and relation type below it.
- Entity and relation types share one type-prefix space; a statement type covers its descendants' prefix ranges.
- **GQL view:** a statement type is a label on nodes and relation nodes alike. A relation type that can fill a role (directly or through a statement supertype) is exposed as a relation node, never as an edge, because GQL edges cannot be edge endpoints.
- **RDF view:** an `rdfs:Class` whose instances are entities and reifiers.
- **Classification:** unchanged; a constraint on a statement type applies to all its entity and relation subtypes.

## Union-typed roles

Some roles accept several types that no single-inheritance tree can cover, because the sets overlap:

```
relation Governs : Relation {
  governor: Rule | Teloid | Mandate as subject -> governs*
  governed: Statement               as object  -> governed_by*
}
```

- Each member type gains the navigation name; navigating to the role yields a tagged union, as `on` blocks do.
- A type test against a union is a check against a few ID ranges.
- In a specialized role, each union member must be the super role's type or a subtype of it.
- **Views:** a GQL label expression (`:Rule|Teloid|Mandate`) and SHACL `sh:or`.
- **Classification:** unchanged; the participant check is per instance.

Union roles keep inheritance single while covering overlapping participant sets, which is why UGS has them instead of multiple inheritance.

## Bitemporal statements

Knowledge needs two clocks: **valid time**, when something held in the world, and **transaction time**, when the graph began and stopped asserting it. A correction never deletes; the old statement is retracted and stays readable for "what did the graph believe on date D" and for lineage through it.

The engine already has a transaction-time clock, the epoch, so bitemporality is an engine feature, not data that every writer maintains:

```
abstract statement Asserted : Statement @bitemporal(valid: valid) {
  valid: Interval<DateTime> Optional @index(interval)
}
```

1. **Transaction time is maintained by the engine.** Each instance gains a read-only `asserted: Interval<DateTime>`, opened at the commit that inserts it and closed by `retract`.
2. **Content is immutable.** `update` is refused on bitemporal types; a correction is a retraction plus a new statement.
3. **Retraction, not deletion.** `retract X(...)` closes `asserted` at the commit; the instance is never physically deleted, except by a legal purge.
4. **Reads see live instances by default.** Three filter items widen or narrow that: `believed at $t` reads the transaction-time slice at an instant, retracted instances included; `with retracted` reads every instance; `open` keeps live instances whose valid interval has no end.
5. **Constraints count live instances:** keys, `@unique`, `@set`, cardinality, `@limit`, `@no_overlap` and `@acyclic`. `@no_overlap` on a bitemporal type compares the declared valid time.
6. **`@on_retract(role: restrict | restrict open | cascade)`** replaces `@on_delete` for bitemporal types: `restrict` refuses a retraction while live instances name the participant, `restrict open` while open ones do. An insert that names a retracted participant is refused.
7. **`@retracts(role)`** on a relation type: inserting an instance retracts its participant in that role in the same commit, so a supersession is one statement.

```
relation Holds : Relation @no_overlap(per: post) {
  holder: Person as subject -> holds*
  post:   Post   as object  -> holders*
}
relation Supersedes : Relation @retracts(old) {
  new: Statement as subject -> supersedes*
  old: Statement as object  -> superseded_by?
}
```

**A third mutability class.** Bitemporal types sit between mutable and append-only types:

| Class | Content | Removal | Monotonic reads |
| --- | --- | --- | --- |
| Mutable (default) | Updatable | Delete | None guaranteed |
| `@bitemporal` | Immutable | Retraction closes `asserted` | `believed at` a past instant is immutable; `with retracted` reads that do not read `asserted` only grow |
| `@append_only` | Immutable | Never | All reads of the type |

Reading the live slice of a bitemporal type is non-monotonic, because a retraction removes a result.

**Classification.** A retraction updates one instance's `asserted` period: coordination-free unless a constraint or `@on_retract(restrict)` reads it, then segmented where the participants are colocated, as a `restrict` delete is. `@retracts(old)` across groups is an atomic write to two groups and takes the coordinated path, unless the relation is colocated with `old`.

**History retention.** Instances of `@bitemporal` and `@append_only` types are never physically discarded (except by a legal purge), so `believed at`, `as of epoch` and `as of time` replay are guaranteed for them at any point in the past. Other types keep their MVCC history for a configurable retention window; `as of` beyond that window is refused with an error that names the window.

## Filtered and per-type constraints

`@limit` gains grouping by concrete type, a filter condition and lower bounds:

```
@limit(aggregate op n per (roles... [, type]) [where condition])
```

```
abstract relation Relation : Asserted @multi { ... }
  @limit(count() <= 1 per (subject, type) where rank = preferred)

relation ActedOnBehalfOf : Relation { ... }
  @limit(count() >= 1 per subject where subject is Software, open)
```

- `op` is `<=`, `>=` or `=`; aggregates remain `count` and `sum` in the first version.
- The condition may read the instance's own values, its participants' types and `open`; it may not read other instances.
- `type` groups per concrete subtype, so a bound declared on a supertype applies separately to each relation kind below it.
- **Lower bounds are checked at commit.** Creating a Software agent and its first delegation must happen in one transaction or one nested insert; a retraction or window close that would break the bound is refused.
- Cardinality markers remain shorthand for unfiltered bounds.
- **Classification:** segmented on the grouped participant, as `@limit` is.

## Depth-bounded hierarchies

`@acyclic(open, max_depth: 32)` refuses cycles and any path longer than 32 through live instances whose valid interval is open, so a tree's closed history never creates a false cycle. Without `open`, the bound covers every live instance. A materialized subtree height in the tree's group lets the existing rank check also verify the new parent's depth plus the moved subtree's height. Segmented with `@colocate(root)`; a move between trees is coordinated, as for `@acyclic`.

## Catalog types as participants

Some statements are about a kind of thing, not a thing: an absence ("this subject has no relation of kind K"), a rule that prescribes a kind of event, a role that applies to things of a type. A role may therefore name catalog types:

```
relation Absence : Asserted @set {
  subject: Statement                     -> absences*
  kind:    schema.RelationType<Relation> -> absences_of*
}
```

- `schema.EntityType<T>` accepts entity types at or below `T`; `schema.RelationType<T>` relation types at or below `T`.
- A reference holds the type across schema versions. Dropping a referenced type follows the deprecate-then-drop path and is refused while live statements name it.
- Catalog types are also values for parameters, so `is $kind` works in queries and functions.
- **Views:** a type-name value in GQL; the class IRI used as an individual in RDF, as OWL 2 punning allows.
- **Classification:** coordination-free; catalog entries change only through schema versions, at epoch boundaries.

## Geometry

```
where:             Geo Optional @index(geo) @uncertainty(where_uncertainty)
where_uncertainty: Measure<Length, m> Optional
```

- `Geo` is a GeoJSON geometry (point, line or polygon) in WGS 84. `@index(geo)` is a spatial index built on S2 cell coverings.
- `@uncertainty(attr)` names a length attribute that widens a point to a cap.
- Operators `within`, `intersects` and `near`, each in a `certainly` and a `possibly` form; `near` orders by distance. `s2_cell(id)` gives a cell's polygon, so a read by cell is a read by area.
- **Views:** GeoSPARQL 1.1 GeoJSON literals and `geo:hasMetricSpatialAccuracy` in RDF; a GeoJSON `STRING` in GQL.
- **Staging:** the type, index and operators are part of the version 1 language; the implementation follows the engine plan's geo stage.
- **Classification:** indexes are derived data; coordination-free.

Whether a schema anchors places in a shared space graph, derives cells from geometry, or both, is a modelling choice the language leaves open.

## Quantities and units

A number never appears without its unit. Quantity kinds and units are schema declarations, so the compiler and the write path enforce unit discipline:

```
quantity Length { dimension: "L", scale: ratio, canonical: m } @maps_to(qudt:Length)
unit ft_i       { quantities: [Length, Altitude], ucum: "[ft_i]", symbol: "ft", multiplier: 0.3048 }
unit dBm        { quantities: [PowerRatio], ucum: "dB[mW]", affine: false }
```

| Type | Values |
| --- | --- |
| `Measure<K, u>` | A number of quantity kind `K` in unit `u`, fixed by the schema |
| `Measure<K>` | A number of kind `K` in any declared unit of `K`; value and unit stored together |
| `Measure` | A number in any declared unit; its kind is its unit's |
| `Unit`, `QuantityKind` | A declared unit or quantity kind, as a value |

- The compiler and the write path refuse undeclared units and units of the wrong kind.
- Values of one kind compare, order and index by their value in the kind's canonical unit, so no canonical twin is stored. A non-affine unit (such as dBm) compares only within itself.
- `convert(q, unit)` is a core function; it refuses a non-affine conversion unless the engine supplies the function for it.
- **Arithmetic in version 1:** addition and subtraction within one kind (converting as needed) and scaling by a number. Dimension algebra (multiplying lengths into areas) comes later.
- `schema.quantities` and `schema.units` are introspectable, with their conversions. Nominal and ordinal kinds have no unit.
- **Views:** QUDT in RDF; a `RECORD` of value and unit in GQL.
- **Classification:** unit checks read only the catalog; coordination-free.

## Filtered derived relations

Derive steps take filters, including the bitemporal read clauses, so maintenance queues can be stated declaratively:

```
derive Asserted.stale =
  grounded_in(with retracted, not live or valid has_end or rank = deprecated) exists
  @materialized @index(range)
```

The result is derived data, maintained through change streams; coordination-free.

## Schema modules

A knowledge domain's schema is a module that extends the core schema:

```
module sr version 3 extends deep_brain >= 5
namespace sr = "https://example.org/sr#"
```

- A module may declare types beneath any type it extends, relation types, `quantity` and `unit` declarations, and members of enums the core marks `@open`.
- The core's constraints apply to the module's subtypes; a module may tighten them, never loosen them.
- Two modules may not declare one name. A deployment's own module may reserve names that no extension can take.
- A namespace's catalog records each installed module and its version. Installing is an additive schema change; an upgrade is a schema version like any other, committed through the metadata group at an epoch boundary.
- `@open` enums are still closed within each catalog version, so misspelt members remain compile errors.

## Language clarifications

| Question | Rule |
| --- | --- |
| Attribute identity | An attribute is one predicate per declaring type (`Person.name`). Only a named attribute type (`attribute Tag = ...`) is shared across types |
| Parameters in path bounds and type tests | Allowed: `nav{1, $depth}`, `is $kind`. Plans are cached per parameter shape, not per value |
| Participant types in `@check` | Allowed. A relation instance's participants never change, so reading them, and their own immutable roles, is a monotonic read that needs no coordination |
| A role without an inverse navigation | `-> _` declares no inverse and builds no reverse index, for hub participants such as an activity with millions of readings. `restrict` and cardinality on that side are compile errors, since nothing could check them |
| Defaults | `rank: Rank = normal`: constant defaults only, so writes stay deterministic |
| A role's cardinality | Always written (`?`, `*`, `+`, `{m, n}`); a role without one is a compile error, since invariants are declared, never inferred |
| `@valid` and `@bitemporal` | `@bitemporal(valid: attr)` implies `@valid(attr)`; declaring both on one type is refused |
| Unsigned integers | Stop at `UInt64`; wider integers are signed (`Int128`) |
| Writes through GQL and SPARQL | Edges and triples written in one transaction assemble into instances, and every instance is validated whole at commit: an n-ary instance missing a required role, or a reifier without its triple, aborts the transaction, naming the instance and the rule |
| `min` and `max` folds over mutable data | Recomputed from the remaining instances when one is updated or deleted; over append-only data they are maintained incrementally, as `last`, `count` and `sum` are |

## Complete example

The aviation schema used throughout this document and in the UQL examples:

```
schema aviation version 1

namespace av = "https://example.org/aviation#"
import "https://schema.org/" as schema

attribute Serial    = String @pattern("[A-Z0-9-]{6,20}")
attribute Tag       = String @independent
attribute Embedding = Vector<BF16, 768> @index(ann, metric: cosine, quantization: int8)

enum Status { active, stored, retired }

/// Anything that can be owned, assembled and supplied.
abstract entity Asset @key(serial) {
  serial:       Serial
  commissioned: Date
  status:       Status
  tags:         Set<Tag>
  notes:        Text Optional @index(fulltext)
  embedding:    Embedding Optional
}
entity Aircraft : Asset { tail_number: String @unique }
entity Engine   : Asset { thrust_kn: Float64 @range(0, 2000) }
entity Avionics : Asset { firmware: String Optional }

abstract entity Organization @key(name) {
  name: String @maps_to(schema:legalName)
}
entity Company   : Organization
entity Authority : Organization { jurisdiction: String @values("EU", "US", "UK") }

relation Owns @colocate(asset) {
  owner: Organization -> assets*
  asset: Asset        -> owner?
  since: Date
}

relation PartOf @acyclic @colocate(root) {
  part:  Asset -> assembly?
  whole: Asset -> parts*
} @on_delete(whole: restrict)

abstract relation Agreement @multi {
  party:        Organization -> agreements*
  counterparty: Organization -> counterparty_agreements*
  signed:       Date
}

relation Supply : Agreement @append_only {
  supplier: Company as party        -> sales*
  buyer:    Company as counterparty -> purchases*
  item:     Asset                   -> contracts*
  value:    Decimal(18, 2) @check(value > 0)
} @limit(sum(value) <= 50_000_000 per buyer)
  @on_delete(supplier: restrict, buyer: restrict, item: restrict)

relation Lease : Agreement {
  lessor: Organization as party        -> leases_out*
  lessee: Organization as counterparty -> leases_in*
  asset:  Aircraft                     -> leases*
  term:   Interval<Date>
}

relation Certifies @colocate(contract) {
  authority: Authority -> certifications*
  contract:  Supply    -> certification?
  valid:     Interval<Date>
}

derive Asset.all_parts   = parts+
derive Company.suppliers = purchases.supplier

fn Aircraft.engines() -> Engine* = all_parts(is Engine)
fn Company.spend(since: Date) -> Decimal(18, 2) = sum(purchases(signed >= since).value)

graph production : aviation
```

## Compilation

&#91;embedded content: Schema compilation · one source, five outputs\]

The outputs are versioned together, so the catalog, both standard views, the classification and the placement rules can never disagree.

**Validation checks**

- **Resolution:** names, imports, types, and no cycles in inheritance.
- **Relations:** at least two roles; navigation names unique on each participant type, including inherited ones; `@acyclic` only between roles of compatible types; `@symmetric` only on binary relations with one participant type.
- **Satisfiability:** cardinality bounds consistent, `orphan` only on optional roles, `@check` expressions deterministic, keys immutable.
- **Mutability consistency:** an `@append_only` relation may not lose instances through a participant's deletion, so its participants must be append-only or restrict deletes.
- **Placement:** conflicting colocation demands reported, with the resulting coordinated writes listed.
- **Standard views:** names that would collide in the GQL or RDF view are rejected.

**Diagnostics** follow the style of Rust's compiler: a precise location, the reason, and a concrete fix.

```
error[UGS0204]: append-only relation `Supply` would lose instances on delete
  --> aviation.ugs:44:3
   |
44 |   item:     Asset   -> contracts*
   |             ^^^^^ `Asset` is deletable and `item` defaults to `cascade`
   |
help: keep `Supply` append-only by restricting deletes of its participants
   |
49 |   @on_delete(supplier: restrict, buyer: restrict, item: restrict)

note[UGS0901]: 3 of 17 write operations need cross-group coordination
  - delete Asset while PartOf.whole instances exist   (restrict)
  - delete Asset while Supply.item instances exist    (restrict)
  - delete Company while Supply instances exist       (restrict)
```

The coordination report is part of every successful compilation, so the cost of each constraint is visible when it is written, not discovered in production.

## Write classification

The compiler derives one commit path per write operation, following the coordination model of the distribution architecture.

**How the classification is built**

1. Enumerate every write the schema permits: create, update and delete per entity type and attribute; insert, update and delete per relation.
2. For each constraint, find the operations that could violate it and decide whether concurrent executions of them always merge safely (invariant confluence).
3. Build a conflict graph linking operations that can jointly violate an invariant.
4. Choose placement so that conflicting operations meet in one group wherever possible (segmentation).
5. Assign each operation a path, emit the coordination report, and store the table in the catalog under the schema version.

**Result for the example schema**

| Write | Constraint involved | Path |
| --- | --- | --- |
| Create `Asset` or `Company` | Key uniqueness | Segmented: the key's owning group checks |
| Update `status`, `notes`, `commissioned` | None | Coordination-free |
| Insert `Owns` | `owner?` on the asset | Segmented: local to the asset's group (`@colocate(asset)`) |
| Insert `PartOf` within a tree | `@acyclic`, `assembly?` | Segmented: rank check in the tree's group |
| Insert `PartOf` joining two trees | `@acyclic` across hierarchies | Coordinated: Calvin-style ordering |
| Insert `Supply` | `@limit` per buyer | Segmented via the buyer's escrow share |
| Insert `Certifies` | `certification?` on the contract | Segmented: local to the contract's group |
| Delete `Asset` | `restrict` on `PartOf.whole` and `Supply.item` | Coordinated when instances exist in other groups; otherwise local |
| Derived relations and indexes | None (derived data) | Coordination-free, via change streams |

## Query classification

Queries in GQL, SPARQL and UQL are classified along three dimensions, from the query and the schema together.

- **Locality.** If placement puts everything the query touches in one group (for example a pattern inside one colocated tree), it reads that group's latest state with no epoch wait.
- **Syntactic monotonicity.** Queries without negation, aggregation or absence-sensitive optional matches only grow as data is added.
- **Data mutability.** If every relation and attribute the query reads is `@append_only` or `@immutable`, facts can only be added, never retracted.

| Query shape | Read level | Guarantee |
| --- | --- | --- |
| Touches one group | Local latest state | Exact |
| Monotonic, reads only append-only or immutable data | Monotonic-fast available | Every result is true and stays true; recent facts may be missing |
| Monotonic, reads mutable data | Monotonic-fast available, flagged | Every result was true at a recent closed epoch; may include facts retracted since |
| Non-monotonic | Snapshot epoch | Exact as of the epoch |

In the example, "assets this company bought" reads only `Supply`, which is append-only, so a monotonic-fast answer never contains a false fact. "How many contracts did this company sign" is an aggregate and always runs at a snapshot epoch.

## Why the schema makes queries faster

| Schema fact | What the planner does with it |
| --- | --- |
| `@key`, `@unique` | Point lookups instead of scans; exact join cardinality |
| Participation cardinality of at most one | Functional dependencies: no duplicate elimination, join elimination, exact estimates |
| Closed type hierarchy with type-prefixed IDs | Label checks become ID range checks; supertype queries become range scans |
| `@acyclic` | Every walk is acyclic, so GQL path modes collapse and need no cycle bookkeeping; traversal always terminates |
| `@set` | No duplicate elimination on edge results |
| `@append_only`, `@immutable` | No deletion checks; caches never need invalidating; strong monotonic-fast guarantee |
| `@colocate` | Single-group plans with no epoch wait |
| Navigation and inverse names | Both directions indexed; the planner chooses direction freely |
| `derive` with `@materialized` | Closures answer path queries by lookup |
| `@index` | Hybrid text, vector and graph retrieval in one plan |

## Mapping to GQL and SPARQL

| UGS construct | GQL view | RDF and SPARQL view |
| --- | --- | --- |
| Entity type with supertypes | Node type; each node carries the labels of its type and all supertypes | `rdfs:Class` with `rdfs:subClassOf`; supertype `rdf:type` triples materialized |
| Attribute | Property with a value type | Datatype property; literal with an XSD datatype |
| Enum or `@values` | `STRING` property with a value check | One IRI per member; `sh:in` |
| Binary `@set` relation without attributes | Directed edge type | Object property, with `owl:inverseOf` for the inverse navigation name |
| `@symmetric` relation | Undirected edge type | Object property declared `owl:SymmetricProperty` |
| Binary relation with attributes | Edge type with properties | Asserted triple plus an RDF 1.2 reifier carrying the attributes |
| Binary `@multi` relation | Parallel edges, each with its own identity | One asserted triple; one reifier per instance |
| N-ary relation | Node type for the relation plus one edge type per role | Class for the relation plus one property per role |
| Relation subtype | Relation node or edge carries the labels of its type and all supertypes | `rdfs:subClassOf` between relation classes; supertype `rdf:type` triples materialized |
| Specialized role | Role edge carries both role labels (`supplier` and `party`) | Role property declared `rdfs:subPropertyOf` the super role; super-role triples materialized |
| Optional or set-valued role | Role edge absent, or several role edges | Role triple absent, or several role triples |
| Relation over a relation | Edge to the relation's node | Property pointing to the reifier or relation node |
| Cardinality | Enforced by the engine | `sh:minCount`, `sh:maxCount` |
| `@key`, `@unique` | Enforced by the engine | Enforced by the engine; exported as a SHACL-SPARQL constraint |
| `derive` (closure or fold) | Read-only edge type or property | Read-only, materialized property |
| `fn` | Not exposed in the first version | Not exposed in the first version |
| `graph` | GQL graph of the generated graph type | Named graph |
| Shared graph and bridge relations | Each domain graph exposed together with the shared graphs it references as one GQL graph | Bridge triples in the domain's named graph, referencing IRIs in the shared named graphs |
| `///` documentation | Catalog description | `rdfs:comment` |

| Extension | GQL view | RDF and SPARQL view |
| --- | --- | --- |
| `statement` type | Label on nodes and relation nodes; relation types that can fill a role are relation nodes, not edges | `rdfs:Class` whose instances are entities and reifiers |
| Union role | Label expression | `sh:or` |
| `@bitemporal` | `asserted` and valid time as `RECORD` properties | `asserted` and valid bounds as literals on the instance or its reifier |
| Catalog type reference | Type-name value | Class IRI used as an individual (OWL 2 punning) |
| `Geo` | GeoJSON `STRING` | GeoSPARQL 1.1 GeoJSON literal |
| `Measure` | `RECORD` of value and unit | QUDT quantity value |

The mapping follows the RDF/LPG mapping contract, so a fact written through any of GQL, SPARQL or UQL is visible through the other two.

## Schema evolution

| Change | Kind | How it takes effect |
| --- | --- | --- |
| Add type, optional attribute, relation, index or graph | Additive | Online; no validation |
| Add required attribute (with default), key, cardinality bound, `@acyclic`, `@append_only` | Tightening | Validation scan; rejected if existing data violates it |
| Remove a constraint or widen a bound | Loosening | Online |
| Change placement | Relocation | Background data movement; the old placement serves reads until the move completes |
| Remove a type, attribute or relation | Destructive | Deprecate first; drop after finalize |

Every schema version is committed through the metadata group and takes effect at an epoch boundary. Queries compile against the version in force at their snapshot epoch, so no query sees a mix of versions. Each change also shows the difference in the coordination report, so a schema review shows exactly which writes become cheaper or more expensive.

## UGS grammar

```
source      = schema | module ;
schema      = "schema" ident "version" int { import | namespace } { decl } ;
module      = "module" ident "version" int "extends" ident ">=" int
              { import | namespace } { decl } ;
import      = "import" ( string "as" ident | "schema" ident ) ;
namespace   = "namespace" ident "=" string ;
decl        = attribute | enum | record | statement | entity | relation | derive
            | function | quantity | unit | graph ;
attribute   = "attribute" ident "=" type { annot } ;
enum        = "enum" ident { annot } "{" ident { "," ident } "}" ;   (* @open *)
record      = "record" ident "{" { field } "}" ;
statement   = "abstract" "statement" ident [ ":" ident ] { annot } "{" { field } "}" ;
entity      = [ "abstract" ] "entity" ident [ ":" ident ] { annot }
              "{" { field } "}" ;
relation    = [ "abstract" ] "relation" ident [ ":" ident ] { annot }
              "{" { role | field } "}" { annot } ;      (* two or more roles, own or inherited *)
role        = ident ":" participant [ "as" ident ] "->" ( ident card | "_" ) ;
participant = union [ "Optional" ] | "[" union "]" ;
union       = ptype { "|" ptype } ;
ptype       = qident | "schema" "." ( "EntityType" | "RelationType" ) "<" qident ">" ;
qident      = [ ident "." ] ident ;                  (* schema-qualified, e.g. temporal.Day *)
field       = ident ":" type [ "Optional" ] [ "=" literal ] { annot } ;
derive      = "derive" ident "." ident [ ":" type ] "=" ( pathexpr | fold ) { annot } ;
fold        = ( "last" | "first" | "count" | "sum" | "min" | "max" )
              "(" path [ filters ] [ "order" "by" path ] ")" [ "." ident ] ;
pathexpr    = step { "." step } ;
step        = ident [ filters ] [ "+" | "{" bound "," bound "}" ] [ "exists" ] ;
function    = "fn" [ ident "." ] ident "(" [ param { "," param } ] ")"
              "->" type card "=" uql_expr ;
param       = ident ":" type ;
quantity    = "quantity" ident "{" qfield { "," qfield } "}" { annot } ;
unit        = "unit" ident "{" qfield { "," qfield } "}" { annot } ;
qfield      = ident ":" ( literal | ident | "[" ident { "," ident } "]" ) ;
graph       = "graph" ident ":" ident { annot } ;    (* @shared, @replicated, @anchor *)
type        = qident [ "<" typearg { "," typearg } ">" ]
              [ "(" int { "," int } ")" ] ;
card        = "?" | "*" | "+" | "{" int "," int "}" ;    (* cardinality, not optionality; always written *)
annot       = "@" ident [ "(" arg { "," arg } ")" ] ;
```

# Part 2: Ultra Query Language (UQL)

UQL follows from UGS: every name a query uses (types, attributes, navigation names, roles) comes from the schema, and the schema supplies the types, cardinalities and roles the compiler needs to check and plan the query. All examples use the aviation schema above.

## Design principles

1. **The query is the shape of the result.** A query is a tree of selections, and the result is a typed tree of the same shape.
2. **The schema carries the semantics.** Queries reference names; the compiler infers types, roles and cardinalities and rejects anything the schema does not define.
3. **Inference before annotation.** Explicit forms exist only for cases that are genuinely ambiguous.
4. **No implicit nulls.** A field that does not apply to a type is absent; a value that is unknown is `None`. The two are never confused.
5. **Same engine, same guarantees.** UQL compiles to the same kernel as GQL and SPARQL and follows the same write classification, query classification and consistency levels.
6. **Checked before it runs.** Name, type, role and classification errors are reported at compile time, with fixes.

## Query structure

```
Aircraft(status = active) {
  tail_number
  serial
  owner { name  edge.since }
}
```

A query has a **root selector** (a type with an optional filter list) and a **projection block**. Nested blocks follow navigation names. Parameters are written `$name` and typed by inference from where they are used; an unknown or ambiguously typed parameter is a compile error. Several roots can be combined in one request, `query { fleet: Aircraft { ... }  vendors: Company { ... } }`, returning one record.

## Projection

| Element | Example | Result |
| --- | --- | --- |
| Attribute | `serial` | The value; `Option` if the attribute is optional |
| Navigation | `owner { name }` | Nested record, `Option`, or list, according to the cardinality |
| Function call | `engines { serial }`, `spend(2026-01-01)` | As declared by the function |
| Alias | `registration: tail_number` | Renamed field |
| All attributes | `Asset { .. }` | Every attribute of the type |
| Meta fields | `type`, `iri` | Concrete type; external IRI |
| Relation attributes (binary) | `owner { name  edge.since }` | Attributes of the relation instance traversed |
| Computed field | `age: years(today() - commissioned)` | Expression over fields |

There is no field for the internal system ID. Keys and IRIs are the only external identities; how keyless entities appear inside `iri` follows the handle scheme, postponed beyond the first version.

## Navigation

Navigation names from the schema are used as fields, in either direction; the compiler knows which role each name belongs to.

```
Company(name = $vendor) {
  sales {                    // Supply instances (n-ary relation)
    signed
    value
    buyer { name }
    item { serial  type }
    certification {          // a relation over a relation
      authority { name }
      valid
    }
  }
}
```

For binary relations a navigation name yields the participant at the other end; for n-ary relations it yields the relation instances, whose other roles are fields. The explicit form `Supply(buyer: $c)` selects the same instances as `purchases` on company `$c` and is available as a root selector.

## Filters

Filters are written in parentheses on a root selector or on any navigation: `contracts(signed >= 2025-01-01, value > 1_000_000)`. Commas mean `and`.

| Operator | Example |
| --- | --- |
| Comparison | `=`, `<>`, `<`, `<=`, `>`, `>=` |
| Membership and range | `status in [active, stored]`, `commissioned between 2010-01-01 and 2015-12-31` |
| Text | `starts_with(serial, "EN-")`, `matches(serial, $re)`, `search(notes, "turbine blade")` |
| Type test | `item is Engine` |
| Existence | `exists certification`, `parts contains Engine(thrust_kn > 300)` |
| Presence | `firmware is some`, `firmware is none` |
| Boolean | `and`, `or`, `not` |

Comparisons on an absent value are false rather than null, so there is no three-valued logic. `not`, `is none` and the absence form of existence are non-monotonic; the classifier accounts for them.

## Polymorphism

```
Asset(commissioned < 2010-01-01) {
  serial
  type
  on Engine   { thrust_kn }
  on Avionics { firmware }
}
```

Selecting a supertype returns instances of every subtype; with type-prefixed IDs this is a range scan. `on Type { ... }` adds subtype-specific fields, and the result is a tagged union. `is Type` narrows inside filters. Asking for `firmware` on `Asset` without `on Avionics` is a compile error, with that fix suggested.

Relation subtypes work the same way. A supertype's navigation name returns every subtype, `on` adds subtype fields, and a specialized role answers to both its own name and the super role's name:

```
Organization(name = $org) {
  agreements order by signed desc {
    signed
    type
    counterparty { name }
    on Supply { value  item { serial } }
    on Lease  { term   asset { tail_number } }
  }
}
```

## Required and excluded branches

```
Company {
  name
  some sales(item is Engine, value > 1_000_000) { signed  value }
  no   purchases(signed >= 2026-01-01)
}
```

- `some nav(...)` keeps a parent only if at least one child matches. It is monotonic.
- `no nav(...)` keeps a parent only if no child matches. It is non-monotonic and always runs at a snapshot epoch.

Without either keyword, a navigation that matches nothing returns an empty list or `None` and the parent stays.

## Paths

```
Aircraft(tail_number = $tail) {
  all_parts { serial  type }                 // derived, materialized closure
  parts{1,2} { serial }                      // bounded expansion
  paths(parts+, to: Engine) { length  nodes { serial } }
}
```

- `nav+` and `nav{m,n}` expand a navigation transitively; by default they return distinct endpoints (reachability), which bounds every hop by the number of entities.
- `paths(...)` returns paths, each with `nodes`, `edges` and `length`. Over `@acyclic` relations every path is acyclic, so no path mode is needed. Over other relations a mode is required: `paths(peers+, mode: trail)`; omitting it is a compile error, because path enumeration can grow exponentially.
- `shortest(peers+, to: $other)` returns one shortest path; `shortest(..., all: true)` returns all of them.

## Aggregation and grouping

Nesting is grouping: an aggregate inside a block is computed per parent.

```
Company {
  name
  contracts_2025: count(purchases(signed >= 2025-01-01))
  spend:          sum(purchases.value)
  largest:        max(purchases.value)
}
```

Aggregates are `count`, `sum`, `avg`, `min`, `max` and `collect`, and are non-monotonic. Grouping is expressed by nesting only; the first version has no `group by`.

## Ordering and pagination

```
Company(name = $vendor) {
  sales order by signed desc limit 5 { signed  value }
}
```

`order by`, `limit` and `after $cursor` apply at any level. Cursors are tied to the query's snapshot epoch, so pages are stable while new data arrives.

## Explicit patterns

Shapes cover trees. When two branches must meet (a cycle or a join), an explicit pattern with variables is used; navigation names label the edges.

```
match (c: Company) -sales->     (s) -item-> (e: Engine),
      (c)          -purchases-> (p) -item-> (e)
return c { name }
```

This returns companies that both sold and bought the same engine. Reusing a variable expresses the join; the `return` block is an ordinary shape.

**Optional and alternative sub-patterns.** `try { ... }` matches a sub-pattern if it can; variables bound only inside it become `Option` in the result. `or { ... } { ... }` matches any of its branches; a variable used after it must be bound in every branch.

```
match (o: Organization) -assets-> (a: Asset),
      or  { (a: Engine) } { (a: Avionics) },
      try { (a) -assembly-> (host: Aircraft) }
return {
  owner: o { name }
  asset: a { serial  type }
  host:  host { tail_number }        // Option: the asset may not be installed
}
```

`or` is monotonic; `try` is not, because a later match can change an absent branch into a present one.

## Functions in queries

Schema functions are called like navigation names or root selectors. A `with` preamble defines functions for a single request, under the same rules as schema functions:

```
with fn Company.big_sales() -> Supply* = sales(value > 1_000_000)

Company {
  name
  spend(2026-01-01)
  big_sales order by value desc limit 3 { value  item { serial } }
}
```

Query-local functions are visible only in their request and cannot shadow names defined in the schema.

## Retrieval: graph, text and vectors in one query

Retrieval operators select by relevance, at the root of a query or on any navigation, so the graph can scope the search before anything is ranked:

```
Aircraft(tail_number = $tail) {
  engines {
    evidence: documents(readers contains Principal(name = $user))
              .chunks ~ hybrid(nearest(embedding, $q_vec), search(text, $q_text), k: 8) {
      text  score  document { title  iri }
    }
  }
}
```

| Operator | Selects by |
| --- | --- |
| `~ nearest(attr, $vec, k: n)` | Vector similarity, using the approximate index or exact BF16 scoring |
| `~ search(attr, $text, k: n)` | BM25 full-text relevance |
| `~ hybrid(..., ..., k: n)` | Both, fused by reciprocal rank fusion, or by weighted scores with `weights: [...]` |

`score` is the retrieval score. Filters, access-control relations and graph expansion apply in the same plan and at the same snapshot. The full design, covering scoped vector search, access control, context shaping, projection functions and agent write-back, is in [Agentic Context Projection](https://claude.ai/artifact/YZU8HTxzfYtKCcL4RMwLHk).

## Schema introspection

The catalog is a read-only graph that UQL queries like any other, so applications and AI agents can read the schema, including its documentation, before writing queries.

```
schema.types(name starts_with "A") {
  name  doc  abstract
  supertype { name }
  attributes  { name  type  optional  doc }
  navigations { name  relation { name }  cardinality }
}

schema.functions { name  parameters { name  type }  returns  doc }
```

`doc` returns the text of the `///` comments. Introspection describes the schema only; it never reveals instance internals such as system IDs. The same catalog is published to SPARQL as RDF vocabulary with `rdfs:comment`, and to GQL as the graph type.

## Consistency and time

```
at monotonic   Company(name = $c) { purchases { item { serial } } }
as of epoch 1842  Asset(serial = $s) { owner { name } }
as of time 2026-10-01T00:00:00Z  Company { spend: sum(purchases.value) }
```

| Clause | Meaning |
| --- | --- |
| (none) | Snapshot at the latest closed epoch; with a session token, at least the epoch of the session's last write |
| `at monotonic` | Monotonic-fast read; a compile error if the query is not monotonic |
| `as of epoch e` | Point-in-time read at epoch `e` |
| `as of time t` | Point-in-time read at the last epoch closed before `t` |

Every response carries the epoch it was read at and, for monotonic-fast reads over mutable data, the staleness flag.

## Temporal operations

**Literals and accessors.** `2026-01-01 .. 2026-07-01` is a closed-open interval; `2026-01-01 ..` has an open end. Durations use ISO 8601: `P30D`, `P1M`, `PT12H`. An interval has `start`, `end` (an `Option`, absent when open) and `duration` (absent when open).

**Arithmetic.** `date + P1M`, `t2 - t1` (a `Duration`), `period.start + P7D`.

**Points and intervals.** `t in i` holds when `i.start <= t < i.end`; points compare with `<`, `<=`, `=`, `>=` and `>`.

**Interval relations.** UQL provides Allen's thirteen interval relations as operators. For closed-open intervals `a` and `b`, with an open end treated as unbounded:

| Operator | Holds when | Inverse |
| --- | --- | --- |
| `a before b` | `a.end < b.start` | `after` |
| `a meets b` | `a.end = b.start` | `met_by` |
| `a overlaps b` | `a.start < b.start < a.end < b.end` | `overlapped_by` |
| `a starts b` | `a.start = b.start` and `a.end < b.end` | `started_by` |
| `a during b` | `b.start < a.start` and `a.end < b.end` | `contains` |
| `a finishes b` | `a.end = b.end` and `b.start < a.start` | `finished_by` |
| `a equals b` | Same start and same end | `equals` |

Three shorthands cover the most common questions:

| Operator | Holds when |
| --- | --- |
| `a intersects b` | The intervals share at least one point: `a.start < b.end` and `b.start < a.end` |
| `a disjoint b` | They share no point |
| `a within b` | `a` lies inside `b`: `during`, `starts`, `finishes` or `equals` |

**Core temporal functions.** `intersection(a, b)` (an optional interval), `span(a, b)`, `date(t)`, `start_of_day(d, zone)`, `today()` and `now()` (the time of the snapshot epoch). `distinct(a, b)` compares two optional values and treats absence as a value, so a status that appears where there was none counts as a change.

**Valid-time shorthand.** For types with `@valid`, `ValueUse(valid at $t)` means `$t in usage`, and `ValueUse(valid during $i)` means `usage intersects $i`.

**Example: a parameterized feasibility query.** Which simulations used, during a campaign, a value whose status on date `$t` changed between what was known last month and what is known today?

```
Simulation {
  run_id
  some value_uses(valid during $campaign) {
    usage
    some value(distinct(status_at($t, $last_month), status_at($t, today()))) {
      name
      previous: status_at($t, $last_month)
      current:  status_at($t, today())
    }
  }
}
```

**Execution.** Interval operators on attributes with `@index(interval)` become range scans on the indexed bounds; ranges over shared time (`temporal.Day(date in $i)`) become key-range scans. Temporal predicates are pure comparisons, so they do not affect monotonicity; `today()` and `now()` are pinned to the epoch being read, so a monotonic-fast read uses its replica's epoch time.

**Grammar additions.**

```
interval    = expr ".." [ expr ] ;
temporal    = expr "in" expr
            | expr allen expr ;
allen       = "before" | "after" | "meets" | "met_by" | "overlaps"
            | "overlapped_by" | "starts" | "started_by" | "during"
            | "contains" | "finishes" | "finished_by" | "equals"
            | "intersects" | "disjoint" | "within" ;
validity    = "valid" ( "at" expr | "during" expr ) ;   (* inside filters *)
```

## Mutations

**Entities**

```
insert Engine {
  serial: "EN-4471-B", commissioned: 2026-10-30, status: active, thrust_kn: 310.5
}
update Asset(serial = "EN-4471-B") { status: stored }
update Asset(status = stored, commissioned < 2000-01-01) { status: retired }
upsert Company(name = "Leonardo") { }
delete Asset(serial = "EN-4471-B")
```

`upsert` matches by key and inserts if absent. A delete applies the schema's `@on_delete` actions; a `restrict` that finds instances fails the statement with the blocking relation named.

**Relations.** Participants are referenced by key selectors, which the compiler checks resolve to at most one entity.

```
insert Owns { owner: Company(name = $c), asset: Asset(serial = $s), since: 2026-10-30 }

insert Supply {
  supplier: Company(name = "Leonardo"),
  buyer:    Company(name = $buyer),
  item:     Asset(serial = $s),
  signed:   2026-10-30,
  value:    1_250_000.00
} returning { iri  signed }

delete Owns(asset: Asset(serial = $s))
```

An n-ary instance is inserted atomically with all its roles. Inserting an existing combination into an `@set` relation returns the existing instance.

**Nested inserts** create an entity together with its relations in one statement:

```
insert Aircraft {
  serial: "AC-0098", commissioned: 2026-11-01, status: active, tail_number: "D-AXYZ",
  owner:  Company(name = $c) { edge.since: 2026-11-01 },
  parts:  [ Asset(serial = "EN-1"), Asset(serial = "EN-2") ]
}
```

`returning { shape }` returns any shape of the written data, at the commit's epoch.

## Transactions

- **Every statement is atomic.** `transaction { ... }` groups several statements into one atomic unit.
- **Multi-group transactions commit optimistically.** A transaction that touches more than one group reads at a closed epoch and buffers its writes; its commit carries its read set to the sequencer, every group it reaches validates its own reads, and all commit or all abort, with no two-phase commit and no deadlock aborts. A transaction of blind, coordination-free writes skips the validation, and one submitted whole avoids an abort and a retry.

  **Single-group transactions may be interactive.** Where every statement stays in one group, Calvin ordering is not needed, so a client may open a transaction, run statements with its own logic in between, and commit. Reads see the group's state at the start of the transaction; writes are validated at commit and the transaction is retried or aborted on conflict. If a statement reaches a second group, the transaction continues and commits optimistically, as above.
- **One path per transaction.** The classifier takes the most demanding path among its statements: all single-group in the same group commits in that group; anything else is sequenced.
- **Dependent reads.** When a write depends on data read in the same transaction, the engine first performs a reconnaissance read, then sequences the transaction with a check that the read set has not changed, as Calvin does; if it has, the transaction is retried automatically.
- **Cost before execution.** `explain` shows the path of every statement before it runs, for example `insert Supply: segmented (escrow share of buyer, group 2)`.
- **Same rules for every language.** GQL data modification and SPARQL Update go through the same classification and commit paths.

## Pipelines: reads into writes

A pipeline feeds the results of one query into one write, in a single atomic statement. Full TypeQL-style multi-stage pipelines are not adopted: chaining stages after a write would make classification and deterministic re-execution much harder for little extra value. The single read-then-write form works cleanly and covers the common cases.

```
for s in Supply(value > 10_000_000, no certification)
insert Certifies {
  authority: Authority(name = "EASA"),
  contract:  s,
  valid:     $validity
}
returning { contract { iri  value } }
```

- **Semantics.** The read stage runs at one snapshot. Each result binds the variable, and the write stage runs once per binding. The whole pipeline commits atomically or not at all.
- **Classification at compile time.** The number of writes is unknown until the read runs, but their types are known, so the commit path is decided from the write stage before execution, like any other statement.
- **Single-group pipelines** run like an interactive single-group transaction.
- **Multi-group pipelines** are dependent transactions. The engine performs a reconnaissance run of the read stage, sequences the pipeline, then re-executes the read stage at its sequenced position. Because execution is deterministic, every replica computes the same writes. If the re-executed read differs from the reconnaissance run (for example, another client certified one of the contracts in between), the pipeline retries automatically.
- **Bounded fan-out.** A pipeline whose write count exceeds the request limit (configurable) aborts before commit. Large backfills run as batches with `limit` and `after $cursor`.
- **Excluded:** a write stage feeding another stage, and a read stage that observes writes of the same pipeline.

## Bitemporal reads and retraction

```
retract Holds(holder: Person(id = $p), post: Post(id = $post))

Asserted(believed at 2026-06-01T00:00:00Z, valid certainly contains $t) { ... }
Person(id = $p) { holds(with retracted) { post { id }  valid  asserted } }
Post(id = $post) { holders(open) { holder { id } } }
```

`retract` closes the asserted period of matching instances of a `@bitemporal` type; `update` is refused on such types. The filter items `believed at`, `with retracted` and `open` follow the rules in Part 1 (Bitemporal statements).

## Subscriptions

Agents and downstream systems follow changes through the commit log:

```
subscribe Organization(id = $org) { all_units { id } }
resume after $cursor
insert InContext { ... } with record $r request $request_id
```

- `subscribe <query>` returns the answer at an epoch, with a cursor, then every commit record after that epoch. Epochs give an exact cut, so there is neither a gap nor an overlap between the answer and the records.
- Records are commit records filtered to the types the query reads, not re-evaluated answers; incremental maintenance of arbitrary queries is a different feature.
- `resume after $cursor` continues from a cursor. A cursor older than the log's retention is refused as unknown.
- `with record $r request $id` attaches a client record to a write; a retry with the same request ID is deduplicated through the replication layer's client sessions.

**Grammar additions.**

```
mutation    = ... | "retract" qident filters [ returning ] ;
fitem       = ... | "believed" "at" expr | "with" "retracted" | "open" ;
request     = ... | "subscribe" query | "resume" "after" param ;
mutation    = mutation [ "with" "record" param "request" param ] ;
allen       = [ "certainly" | "possibly" ] allen_op ;
```

## Typing and results

Every query has a result type derived from the schema before it runs.

| Schema fact | Result type |
| --- | --- |
| Required attribute | `T` |
| Optional attribute | `Option<T>` |
| Navigation with cardinality exactly one | Record |
| Navigation with `?` | `Option<Record>` |
| Navigation with `*` | `List<Record>` |
| Navigation with `+` | Non-empty list |
| Supertype with `on` blocks | Tagged union of the subtypes' records |
| `count`; `sum`, `min`, `max`; `avg` | `UInt64`; the attribute's type, widened for `sum`; `Float64` or `Decimal` |
| `paths(...)` | `List<Path>` |

Results travel as Arrow-compatible column buffers: records as struct columns, lists as list columns, unions as union columns. Clients that want Arrow wrap the buffers without copying.

**Typed clients.** A Rust macro checks queries against the schema file at build time and generates the result types, with subtype narrowing as an enum:

```
let fleet = uql!(db, "aviation.ugs", r#"
    Aircraft { tail_number  all_parts { serial  on Avionics { firmware } } }
"#).await?;

for ac in fleet {
    for part in ac.all_parts {
        if let PartKind::Avionics { firmware } = part.kind {
            // firmware: Option<String>
        }
    }
}
```

## Classification of UQL queries

Because every name is resolved at compile time, monotonicity is decided syntactically and combined with the schema's mutability facts as described in Part 1.

| Construct | Monotonic? |
| --- | --- |
| Selectors, comparisons, navigation, `some`, `exists`, `contains`, `nav+`, `paths(...)` | Yes |
| `no`, `not`, `is none` | No |
| Aggregates | No |
| `order by` with `limit`, `~ nearest`, `~ search`, `~ hybrid` (top-k) | No |
| `shortest(...)` | No |

Further rules: `or` in patterns is monotonic and `try` is not; a function call is monotonic exactly when its body is; a navigation through a relation supertype counts as reading every subtype, so a monotonic query over `agreements` is flagged when it can reach mutable `Lease` data even though `Supply` is append-only.

`explain` reports, for any query: the read level, the groups touched, whether it is monotonic, and, for writes, the commit path of each statement.

## Diagnostics

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

error[UQL0410]: `at monotonic` requires a monotonic query
  --> query:1:1
   |
 1 | at monotonic Company { spend: sum(purchases.value) }
   |                                ^^^ aggregates are not monotonic
   |
help: remove `at monotonic` to read at a snapshot epoch
```

## The same question in three languages

*Suppliers of engines worth over 1,000,000, with each contract's date, value, buyer and engine.*

**UQL**

```
Company {
  name
  some sales(item is Engine, value > 1_000_000) {
    signed  value  buyer { name }  item { serial }
  }
}
```

**GQL (approx.)**, using the generated graph type, where `Supply` is a node with one edge per role:

```
MATCH (c:Supply)-[:supplier]->(s:Company),
      (c)-[:buyer]->(b:Company),
      (c)-[:item]->(e:Engine)
WHERE c.value > 1000000
RETURN s.name, collect({signed: c.signed, value: c.value,
                        buyer: b.name, engine: e.serial}) AS contracts
```

**SPARQL**, using the generated vocabulary:

```
PREFIX av: <https://example.org/aviation#>
SELECT ?supplier ?signed ?value ?buyer ?serial WHERE {
  ?c a av:Supply ;
     av:Supply_supplier ?s ; av:Supply_buyer ?b ; av:Supply_item ?e ;
     av:signed ?signed ; av:value ?value .
  ?e a av:Engine ; av:serial ?serial .
  ?s av:name ?supplier . ?b av:name ?buyer .
  FILTER(?value > 1000000)
}
```

All three run on the same engine, read the same data and are classified the same way; UQL returns one typed tree per supplier, while GQL and SPARQL return rows.

## UQL grammar

```
request     = [ preamble ] [ consistency ]
              ( query | mutation | pipeline | transaction | explain ) ;
preamble    = "with" function { function } ;          (* UGS function syntax *)
consistency = "at" "monotonic" | "as" "of" ( "epoch" int | "time" datetime ) ;
query       = selector [ "~" call ] [ filters ] { modifier } shape
            | "match" pattern { "," pattern } "return" result
            | "query" "{" { ident ":" query } "}" ;
selector    = ident { "." ident } [ "(" [ args ] ")" ] ;   (* type, function, schema.* *)
filters     = "(" fitem { "," fitem } ")" ;
fitem       = expr | ident ":" expr ;                 (* filter or role binding *)
modifier    = "order" "by" ordering | "limit" int | "after" param ;
shape       = "{" { field } "}" ;
field       = [ ident ":" ] path [ "(" [ args ] ")" ] [ filters ] [ "~" call ]
              { modifier } [ shape ]
            | [ ident ":" ] expr
            | ( "some" | "no" ) path [ filters ] [ shape ]
            | "on" ident shape
            | ".." ;
path        = ident [ "+" | "{" int "," int "}" ] { "." ident } ;
pattern     = chain | "try" "{" chain { "," chain } "}"
            | "or" "{" chain { "," chain } "}" "{" chain { "," chain } "}"
              { "{" chain { "," chain } "}" } ;
chain       = node { "-" ident [ "+" ] "->" node } ;
node        = "(" ident [ ":" ident [ filters ] ] ")" ;
result      = ident shape | "{" { ident ":" ident shape } "}" ;
mutation    = ( "insert" | "upsert" ) ident [ filters ] values [ returning ]
            | "update" ident filters values [ returning ]
            | "delete" ident filters ;
values      = "{" [ assign { "," assign } ] "}" ;
assign      = path ":" ( expr | ident filters [ values ]
            | "[" ident filters { "," ident filters } "]" ) ;
returning   = "returning" shape ;
pipeline    = "for" ident "in" query mutation ;
transaction = "transaction" "{" { mutation | pipeline } "}" ;
explain     = "explain" ( query | mutation | pipeline | transaction ) ;
```

# Decisions

| Question | Decision |
| --- | --- |
| Multiple inheritance for interface-only abstract types | No; inheritance stays single. Overlapping participant sets use union roles |
| Relation inheritance | Yes: single inheritance with role specialization (`as`); `@acyclic` on a supertype spans all subtypes |
| Statement root | Yes: `abstract statement` types as one root for entities and relations, sharing one type-prefix space |
| Union-typed roles | Yes: `role: A \| B \| C` |
| Bitemporal statements | Yes: `@bitemporal`, with transaction time maintained by the engine from commit epochs; content immutable, retraction instead of deletion, `@on_retract`, `@retracts` |
| One spelling per constraint | `@no_overlap` and `@limit(... where ...)` express overlap and open-window bounds; no separate qualifiers on navigation cardinalities |
| History retention | Replay guaranteed for `@bitemporal` and `@append_only` types; other types within a configurable MVCC retention window |
| Filtered and per-type constraints | Yes: `@limit` with `type` grouping, `where` conditions and lower bounds; lower bounds checked at commit |
| Depth bound on hierarchies | Yes: `@acyclic(open, max_depth: n)` |
| Catalog types as participants | Yes: `schema.EntityType<T>` and `schema.RelationType<T>` |
| Multi-domain structure | Disjoint domain graphs connected only through `@shared` graphs, via bridge relations |
| Supernodes in shared graphs | Leaf anchoring (`@anchor`), reverse adjacency partitioned by domain, tiles later; small shared graphs `@replicated`; roles without inverse navigation (`-> _`) for hubs |
| Occasion-primary modelling | Occasions as `@append_only` relations anchored in shared time; state as materialized `derive` folds |
| Temporal types and operations | Core language in version 1: interval literals, Allen's relations, `@valid`, `@no_overlap`, interval indexes |
| Temporal precision | Merged into `Interval<T>`: precision per bound, `certainly` and `possibly` operator forms, plain form meaning `possibly` |
| Geometry | Yes: `Geo` with `@index(geo)` and `@uncertainty` in the version 1 language; implementation follows the engine's geo stage |
| Quantities and units | Yes: `quantity` and `unit` declarations and `Measure` types; version 1 arithmetic within one kind plus scaling |
| Filtered derived relations | Yes |
| Schema modules | Yes: `module ... extends ...`, tighten-only, names unique across modules |
| Commit-log subscriptions | Yes: `subscribe`, `resume`, records with request IDs; commit records, not re-evaluated answers |
| Attribute identity, parameters in bounds and type tests, participant types in `@check`, defaults | As in Language clarifications |
| Personal data and erasure (`@personal`, crypto-shredding) | Postponed beyond the first version |
| Counterfactual scenarios | Out of scope: counterfactual reasoning belongs to DeepCausality, not UltraGraphDB |
| Ontology refinement and system-captured provenance | Deferred until the DeepBrain schema is redesigned for UGS |
| Aggregates supported by `@limit` in the first version | `count` and `sum` |
| Standalone schema-compiler CLI | No; schemas are compiled by the server (in CI, the development container) |
| Naming of n-ary role predicates in the RDF view | `av:Supply_buyer` |
| User-defined functions | Yes: schema functions and query-local `with` functions; pure, inlined, non-recursive |
| `@independent` and `@values` | Yes |
| Schema introspection | Yes; documentation from `///` comments |
| Internal IDs | Never exposed: no ID field in queries or results, and none in IRIs or cursors. 128 bits: type prefix 16, bucket 16, sequence 64, reserved 32 (2026-11-01) |
| Type prefixes | Allocated in declaration order, never renumbered; a supertype covers its descendants' prefix ranges (2026-11-01) |
| Opaque handles for keyless entities and relation instances | Postponed beyond the first version, as personal data is; no key encryption (2026-11-01) |
| Edge-attribute accessor in UQL | `edge.since` |
| `group by` in the first UQL version | No; grouping by nesting only |
| `try` and `or` in explicit patterns | Yes |
| Pipelines from reads into writes | Yes, as one read stage feeding one write stage; multi-stage pipelines are not adopted |
| UQL standard library (string, numeric, vector functions) | Next version; temporal, geometry and unit operations are core in version 1 |
| Interactive transactions | Yes; multi-group transactions commit optimistically through the sequencer |

# Sources

- [Coordination Avoidance in Database Systems (Bailis et al., VLDB 2015)](https://arxiv.org/pdf/1402.2237)
- [Keep CALM and CRDT On (VLDB 2023)](https://www.vldb.org/pvldb/vol16/p856-power.pdf)
- [Interactive Checks for Coordination Avoidance (Whittaker and Hellerstein, VLDB 2019)](https://mwhittaker.github.io/publications/iconfluence_VLDB19.pdf)
- [Hamsaz: Replication Coordination Analysis and Synthesis (POPL 2019)](https://mohsenlesani.github.io/companion/popl19/POPL19.pdf)
- [UltraGraphDB Distribution Architecture](https://claude.ai/artifact/W7YBsh2yPKCLjehZiEsFZD)
- [UltraGraphDB Single-Container Deployment](https://claude.ai/artifact/Lj3ztjXbdbyWW98ZGE49qG)

* [TypeQL keyword glossary (TypeDB 3)](https://typedb.com/docs/typeql-reference/keywords/)
* [TypeQL language guide](https://development.typedb.com/docs/typeql)

- [Neo4j: Vector search with filters](https://neo4j.com/blog/developer/vector-search-with-filters-in-neo4j-v2026-01-preview/)
