## ADDED Requirements

### Requirement: Predicate declarations
The catalog SHALL hold, per namespace, a declaration for each predicate IRI it knows: its kind (edge, property, or
either), its cardinality (single- or multi-valued), an optional datatype constraint, whether its values are unique, its
indexes (tokenizers, an exact count index, and a vector index with its parameters), and the entity types it is declared
on, which the data dictionary reports it under. A declaration whose datatype constraint is not among the datatypes one
of its tokenizers takes SHALL be refused. A predicate the catalog does not declare SHALL behave as declared with kind
either, multi-valued, no datatype constraint, values not unique, no index and no entity type.

#### Scenario: An undeclared predicate accepts any object
- **WHEN** `<a> <p> <b>` and `<a> <p> "text"` are written and `p` is not declared
- **THEN** both statements are held

#### Scenario: A tokenizer that cannot take the datatype constraint is refused
- **WHEN** `age` is declared with datatype `xsd:string` and an `int` index
- **THEN** the declaration fails with an error naming `age`, `int` and `xsd:string`

#### Scenario: A declared index is read by index maintenance
- **WHEN** `name` is declared a property with a `term` index and `<a> <name> "big apple"` is written
- **THEN** the term index of `name` lists `a` under `big` and `apple`

### Requirement: Writes are checked against the catalog
A write SHALL be refused, writing nothing of its changeset, when it names a literal object for a predicate declared an
edge, an IRI or blank-node object for a predicate declared a property, or a literal whose datatype differs from the
predicate's datatype constraint. A write to a single-valued predicate SHALL replace the subject's previous object.

#### Scenario: A literal on an edge predicate is refused
- **WHEN** `knows` is declared an edge and `<a> <knows> "Bob"` is written
- **THEN** the write fails with an error naming `knows` and the literal, and nothing of its changeset is held

#### Scenario: A datatype constraint refuses another datatype
- **WHEN** `age` is declared with datatype `xsd:integer` and `<a> <age> "old"` is written
- **THEN** the write fails with an error naming `age`, `xsd:integer` and `xsd:string`

#### Scenario: A single-valued property is overwritten
- **WHEN** `age` is declared single-valued, `<a> <age> 30` is held, and `<a> <age> 31` is written
- **THEN** `a` holds `age` 31 only

### Requirement: Unique values
A write SHALL be refused, writing nothing of its changeset, when it gives a predicate declared unique an object that
another subject holds for that predicate in the same graph. Writing an object the same subject already holds SHALL be
accepted.

#### Scenario: A second subject with a unique value is refused
- **WHEN** `id` is declared unique, `<a> <id> "x"` is held, and `<b> <id> "x"` is written
- **THEN** the write fails with an error naming `id`, `"x"` and `a`, and nothing of its changeset is held

#### Scenario: The same subject may write its unique value again
- **WHEN** `id` is declared unique, `<a> <id> "x"` is held, and `<a> <id> "x"` is written
- **THEN** the write succeeds and `a` holds `id` `"x"`

#### Scenario: A unique value is unique per graph
- **WHEN** `id` is declared unique and `<a> <id> "x"` is held in graph `g1`, and `<b> <id> "x"` is written in graph `g2`
- **THEN** both statements are held

### Requirement: Entity types
The catalog SHALL hold entity types, each with at most one parent type and an abstract flag, and relations, each with
named roles. Declaring a second parent for a type or a parent cycle SHALL be refused. Each entity type SHALL be given a
number in a preorder walk of the type hierarchy, recorded for the UID type bits the quad layout reserves.

#### Scenario: A second parent is refused
- **WHEN** `Employee` is declared a subtype of `Person` and then of `Agent`
- **THEN** the second declaration fails with an error naming `Employee`, `Person` and `Agent`

#### Scenario: Preorder numbers follow the hierarchy
- **WHEN** `Agent` has the subtypes `Person` and `Org`, and `Person` the subtype `Employee`
- **THEN** the preorder numbers place `Employee` between `Person` and `Org`
