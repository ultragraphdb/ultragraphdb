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
