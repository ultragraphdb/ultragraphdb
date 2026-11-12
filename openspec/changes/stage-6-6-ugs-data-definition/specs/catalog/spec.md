## MODIFIED Requirements

### Requirement: Predicate declarations
The catalog SHALL hold, per tenant, a declaration for each predicate IRI it knows: its kind (edge, property, or
either), its cardinality (single- or multi-valued), an optional datatype constraint, whether its values are unique, its
indexes (tokenizers, an exact count index, and a vector index with its parameters), and the entity types it is declared
on, which the data dictionary reports it under. A declaration whose datatype constraint is not among the datatypes one
of its tokenizers takes SHALL be refused. In a governed tenant the declarations SHALL be the Predicate entries of its
compiled catalog version (ugs-catalog, "A catalog version holds the compiled schema"). In a tenant with no schema, and
in a governed one where `@open` admits it, a predicate the catalog does not declare SHALL behave as declared with kind
either, multi-valued, no datatype constraint, values not unique, no index and no entity type; a governed tenant SHALL
refuse every other undeclared predicate (ugs-catalog, "A tenant with a schema is governed").

#### Scenario: An undeclared predicate accepts any object
- **WHEN** `<a> <p> <b>` and `<a> <p> "text"` are written in a tenant with no schema and `p` is not declared
- **THEN** both statements are held

#### Scenario: A tokenizer that cannot take the datatype constraint is refused
- **WHEN** `age` is declared with datatype `xsd:string` and an `int` index
- **THEN** the declaration fails with an error naming `age`, `int` and `xsd:string`

#### Scenario: A declared index is read by index maintenance
- **WHEN** `name` is declared a property with a `term` index and `<a> <name> "big apple"` is written
- **THEN** the term index of `name` lists `a` under `big` and `apple`

### Requirement: Entity types
The catalog SHALL hold entity types, each with at most one parent type and an abstract flag, and relations, each with
named roles. Declaring a second parent for a type or a parent cycle SHALL be refused. Declaring a type that has a parent
again without that parent SHALL be refused, and the type SHALL keep its parent; a parent is dropped by removing the
type and declaring it again. A type a schema declares SHALL hold the type prefix
the metadata group allocated it in declaration order and its prefix set as sorted ranges (ugs-identity, "Type prefixes
are allocated in declaration order and never reissued"); a type a tenant with no schema declares through the write path
SHALL hold no prefix, its instances being of the built-in `Resource` type.

#### Scenario: A second parent is refused
- **WHEN** `Employee` is declared a subtype of `Person` and then of `Agent`
- **THEN** the second declaration fails with an error naming `Employee`, `Person` and `Agent`

#### Scenario: A parent is not dropped by a redeclaration
- **WHEN** `Employee` is declared a subtype of `Person` and then declared again with no parent
- **THEN** the second declaration fails with an error naming `Employee` and `Person`, and `Employee` keeps `Person`

#### Scenario: Prefixes follow declaration order
- **WHEN** a schema declares `Agent`, then `Person` below it, then `Org` below `Agent`, then `Employee` below `Person`,
  as the first types of an empty tenant
- **THEN** they hold the prefixes 3, 4, 5 and 6, and `Person`'s prefix set is the two ranges holding 4 and 6

### Requirement: Unique values
A write SHALL be refused, writing nothing of its changeset, when it gives a predicate declared unique an object that
another subject holds for that predicate in the same graph. Writing an object the same subject already holds SHALL be
accepted. Every write of a unique predicate's value, a plain write as well as an insert-if-absent or an
upsert-and-link, SHALL claim the value at the partition owning the value's bucket (system-identity, "The bucket comes
from the tenant and the key"), which records the claim and decides it; a claim from another partition is
cross-partition and SHALL be refused with `CrossPartition` in this stage (partitions, "A read-write transaction stays
within one partition in this stage"), with one partition it is local. A `@key` attribute's value SHALL be claimed in the bucket of its instance, computed from the key space's
prefix and the canonical key, so the partition creating the instance decides the claim; the rule above holds of a
`@unique` attribute and of a unique predicate of a tenant with no schema.

#### Scenario: A second subject with a unique value is refused
- **WHEN** `id` is declared unique, `<a> <id> "x"` is held, and `<b> <id> "x"` is written, all in the partition owning
  the bucket of `"x"`
- **THEN** the write fails with an error naming `id`, `"x"` and `a`, and nothing of its changeset is held

#### Scenario: The same subject may write its unique value again
- **WHEN** `id` is declared unique, `<a> <id> "x"` is held, and `<a> <id> "x"` is written, in the partition owning the
  bucket of `"x"`
- **THEN** the write succeeds and `a` holds `id` `"x"`

#### Scenario: A unique value is unique per graph
- **WHEN** `id` is declared unique and `<a> <id> "x"` is held in graph `g1`, and `<b> <id> "x"` is written in graph
  `g2`, all in the partition owning the bucket of `"x"`
- **THEN** both statements are held

#### Scenario: A plain write and an insert-if-absent from two partitions cannot both claim
- **WHEN** `id` is declared unique, partition 1 owns the bucket of `"x"`, a transaction bound to partition 2 writes
  `<b> <id> "x"`, and a transaction bound to partition 1 insert-if-absents a node with `id` `"x"`
- **THEN** the write in partition 2 aborts with `CrossPartition` naming partitions 2 and 1, the insert-if-absent
  commits, and one node holds `id` `"x"`
