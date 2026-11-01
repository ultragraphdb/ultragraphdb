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

**Who gains most.** AI agents gain the most: their typical failure is a plausible query that silently returns nothing, and UQL turns that into a compile error with a fix they can act on, while tree-shaped results fit directly into context with no redundant rows.

**What the standard languages keep.** GQL and SPARQL remain the right choice for portability, existing tools and drivers, standards conformance and teams that already know them. That is why both stay first-class and UQL is an addition, not a replacement.

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
- **Reserved words:** `schema`, `version`, `namespace`, `import`, `attribute`, `abstract`, `entity`, `relation`, `derive`, `graph`, `enum`, `record`, `fn`, `as`, `true`, `false`.
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
| `List<T>`, `Set<T>` | Ordered list; unordered set without duplicates | `LIST<T>` | `rdf:JSON` literal; one triple per member |
| `Interval<T>` | Closed-open interval over an ordered type | `RECORD` | `rdf:JSON` literal |
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

- **Inheritance is single.** A subtype inherits all attributes and roles of its supertype and may add constraints to inherited attributes (for example make an optional attribute required), but never remove them. `abstract` types have no direct instances. Multiple inheritance is not supported, not even for abstract types used only as interfaces, because it would break contiguous type ID ranges.
- **System identity.** Every entity receives a system ID made of its type prefix, its replica group and a sequence number. IDs are dense, generated without coordination and never reused, and they never leave the database: no query, result, IRI or cursor reveals them. Entities without a key are addressed from outside by an **opaque handle**, a keyed encryption of the system ID that is stable for the entity's lifetime but reveals nothing about its type, group or insertion order.
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
| Identity | Relation types get type-prefixed statement IDs in hierarchy order, so a supertype is one contiguous ID range |

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

User-defined functions are separate from the built-in standard library (temporal, string, numeric and vector functions), which is planned for the next version.

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

- **Default.** In the current KV stage, each attribute and relation is placed by predicate, as in Dgraph. In the later tile stage, placement follows the GSL grid.
- **`@colocate(role)`** partitions a relation by the group of the participant in that role instead of by predicate, so constraints on that participant are checked locally. Example: `asset: Asset -> owner?` with `@colocate(asset)` puts each ownership instance in its asset's group.
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
- **Instance IRIs.** Entities with a key get `{namespace}{Type}/{key}`; entities without a key get `{namespace}{Type}/{opaque handle}`; relation instances get a reifier IRI `{namespace}rel/{opaque handle}`. `@iri("template")` overrides the pattern per type. System IDs never appear in IRIs.
- **External vocabularies.** `import "https://schema.org/" as schema` and `@maps_to(schema:name)` align a UGS term with an external one (exported as `owl:equivalentProperty` or `owl:equivalentClass`), so existing ontologies and SPARQL clients can use familiar terms.

## Graphs

- `graph production : aviation` creates a graph instance of a schema. One schema can have several graphs, for example per tenant or per security domain.
- Each graph is a GQL graph in the catalog and a named graph in RDF.
- Relations stay within one graph; references across graphs are by IRI.
- A graph is also the natural unit for access control at retrieval time and for placement policies such as pinning a graph to specific groups.

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
| `derive` | Read-only edge type | Read-only, materialized property |
| `fn` | Not exposed in the first version | Not exposed in the first version |
| `graph` | GQL graph of the generated graph type | Named graph |
| `///` documentation | Catalog description | `rdfs:comment` |

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
schema      = header { import | namespace } { decl } ;
header      = "schema" ident "version" int ;
import      = "import" string "as" ident ;
namespace   = "namespace" ident "=" string ;
decl        = attribute | enum | record | entity | relation | derive | function | graph ;
attribute   = "attribute" ident "=" type { annot } ;
enum        = "enum" ident "{" ident { "," ident } "}" ;
record      = "record" ident "{" { field } "}" ;
entity      = [ "abstract" ] "entity" ident [ ":" ident ] { annot }
              "{" { field } "}" ;
relation    = [ "abstract" ] "relation" ident [ ":" ident ] { annot }
              "{" { role | field } "}" { annot } ;      (* two or more roles, own or inherited *)
role        = ident ":" participant [ "as" ident ] "->" ident card ;
participant = ident [ "Optional" ] | "[" ident "]" ;
field       = ident ":" type [ "Optional" ] { annot } ;
derive      = "derive" ident "." ident "=" pathexpr { annot } ;
function    = "fn" [ ident "." ] ident "(" [ param { "," param } ] ")"
              "->" type card "=" uql_expr ;
param       = ident ":" type ;
pathexpr    = step { "." step } ;
step        = ident [ "+" | "{" int "," int "}" ] [ "exists" ] ;
graph       = "graph" ident ":" ident ;
type        = ident [ "<" typearg { "," typearg } ">" ]
              [ "(" int { "," int } ")" ] ;
card        = [ "?" | "*" | "+" | "{" int "," int "}" ] ;  (* cardinality, not optionality *)
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

There is no field for the internal system ID. Keys and IRIs are the only external identities, and keyless entities appear through their opaque handle inside `iri`.

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

## Retrieval: vectors, text and graph in one query

```
Asset ~ hybrid(nearest(embedding, $vec), search(notes, $text), k: 50)
      (status = active) {
  serial
  score
  owner { name }
  all_parts(is Avionics) { serial  firmware }
}
```

- `~ nearest(attr, $vec, k: n)` selects by vector similarity, using the approximate index and BF16 rescoring.
- `~ search(attr, $text, k: n)` selects by BM25 full-text relevance.
- `~ hybrid(..., ..., k: n)` combines both by reciprocal rank fusion.
- `score` is the retrieval score; filters and graph expansion apply in the same plan.

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
- **Multi-group transactions are submitted whole.** A transaction that touches more than one group is sent as one request, not opened interactively with client logic between statements. This is the price of Calvin-style ordering, and it is what removes two-phase commit and deadlock aborts.

  **Single-group transactions may be interactive.** Where every statement stays in one group, Calvin ordering is not needed, so a client may open a transaction, run statements with its own logic in between, and commit. Reads see the group's state at the start of the transaction; writes are validated at commit and the transaction is retried or aborted on conflict. If a statement would reach a second group, it is rejected with a hint to submit the transaction whole.
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
field       = [ ident ":" ] path [ "(" [ args ] ")" ] [ filters ] { modifier } [ shape ]
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
| Multiple inheritance for interface-only abstract types | No; inheritance stays single |
| Relation inheritance | Yes: single inheritance with role specialization (`as`); `@acyclic` on a supertype spans all subtypes |
| Aggregates supported by `@limit` in the first version | `count` and `sum` |
| Standalone schema-compiler CLI | No; schemas are compiled by the server (in CI, the development container) |
| Naming of n-ary role predicates in the RDF view | `av:Supply_buyer` |
| User-defined functions | Yes: schema functions and query-local `with` functions; pure, inlined, non-recursive |
| `@independent` and `@values` | Yes |
| Schema introspection | Yes; documentation from `///` comments |
| Internal IDs | Never exposed: no ID field in queries or results, and none in IRIs or cursors; keyless entities use opaque handles |
| Edge-attribute accessor in UQL | `edge.since` |
| `group by` in the first UQL version | No; grouping by nesting only |
| `try` and `or` in explicit patterns | Yes |
| Pipelines from reads into writes | Yes, as one read stage feeding one write stage; multi-stage pipelines are not adopted |
| UQL standard library (temporal, string, numeric, vector functions) | Next version |
| Interactive transactions | Yes, for single-group transactions; multi-group transactions are submitted whole |

# Sources

- [Coordination Avoidance in Database Systems (Bailis et al., VLDB 2015)](https://arxiv.org/pdf/1402.2237)
- [Keep CALM and CRDT On (VLDB 2023)](https://www.vldb.org/pvldb/vol16/p856-power.pdf)
- [Interactive Checks for Coordination Avoidance (Whittaker and Hellerstein, VLDB 2019)](https://mwhittaker.github.io/publications/iconfluence_VLDB19.pdf)
- [Hamsaz: Replication Coordination Analysis and Synthesis (POPL 2019)](https://mohsenlesani.github.io/companion/popl19/POPL19.pdf)
- [UltraGraphDB Distribution Architecture](https://claude.ai/artifact/W7YBsh2yPKCLjehZiEsFZD)
- [UltraGraphDB Single-Container Deployment](https://claude.ai/artifact/Lj3ztjXbdbyWW98ZGE49qG)

* [TypeQL keyword glossary (TypeDB 3)](https://typedb.com/docs/typeql-reference/keywords/)
* [TypeQL language guide](https://development.typedb.com/docs/typeql)
