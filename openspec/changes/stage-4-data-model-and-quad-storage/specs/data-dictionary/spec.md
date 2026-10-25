## ADDED Requirements

### Requirement: One read API answers what the data holds
The store SHALL offer one read API, independent of any query language, that returns at a snapshot:
- the labels, the relationship types and the property keys;
- per label, its properties with their object kinds, datatypes and cardinality, and its relationship types with the
  labels they lead to;
- the declared entity types with their parents, abstract flags and preorder numbers, and the relations with their
  roles;
- the indexes and constraints the catalog declares;
- the statistics of each graph and predicate.

Stage 5's describe request, stage 8's SPARQL service description, VoID statistics and GQL catalog queries, and stage
9's Bolt schema procedures SHALL each be answered from it.

#### Scenario: Labels and their properties are listed
- **WHEN** `<a> rdf:type <Person>`, `<a> <name> "Ann"` and `<a> <knows> <b>` are held, `<b> rdf:type <Person>`, and the
  shapes are current
- **THEN** the API lists the label `Person`, the property `name` of `Person` with datatype `xsd:string`, and the
  relationship type `knows` from `Person` to `Person`

### Requirement: Declared entries are exact
For a predicate or entity type the catalog declares, the API SHALL return the declaration: its kind, cardinality,
datatype, indexes, parent and roles, whatever the data holds.

#### Scenario: A declared cardinality is reported without data
- **WHEN** `email` is declared a single-valued property of `Person` with datatype `xsd:string`, and no node holds it
- **THEN** the API reports `email` on `Person`, single-valued, of datatype `xsd:string`

### Requirement: Shapes of undeclared data come from a background pass
For data the catalog does not declare, per-label shapes SHALL be computed by a background pass over a snapshot, from the
subjects' `rdf:type` statements and their predicate lists, and replaced whole when the pass completes. No write SHALL read
or update them. The API SHALL report the snapshot timestamp the shapes were computed at. Per-predicate object kinds and
datatypes SHALL come from the statistics, exact at every snapshot.

#### Scenario: A new property appears after the next pass
- **WHEN** `<a> rdf:type <Person>` is held, `<a> <nick> "Al"` is written, and the next pass completes
- **THEN** the API lists `nick` on `Person`, and reports the pass's snapshot timestamp

#### Scenario: A write reads no shape
- **WHEN** `<a> <nick> "Al"` is written while a pass runs
- **THEN** the write reads no shape or label list, and the pass's result does not depend on the write's order against it
