## MODIFIED Requirements

### Requirement: Names map between the views
A property-graph label, property name or edge type SHALL map to the IRI formed by the catalog's base IRI and the name. A
node created through the property-graph view without an IRI SHALL be named by a skolem IRI minted as a blank node's,
`…/.well-known/genid/{bucket}-{random}` (term-dictionary, "IRIs and blank nodes get system IDs"), holding no part of
its system ID beyond its bucket. A blank node SHALL be held as a skolem IRI, marked as skolemized so that the RDF view
presents it as a blank node again.

#### Scenario: A property name is an IRI under the base
- **WHEN** the base IRI is `http://ex.org/` and a property `age` is written through the property-graph view
- **THEN** the RDF view holds it under the predicate `<http://ex.org/age>`

#### Scenario: A blank node round-trips as a blank node
- **WHEN** `_:x <knows> <b>` is written through the RDF view and read back through it
- **THEN** the subject is presented as a blank node, the same blank node for every statement that named `_:x`

#### Scenario: A property-graph node is named without its ID
- **WHEN** a node is created through the property-graph view without an IRI
- **THEN** it is named by a skolem IRI whose text holds its bucket and 26 random hexadecimal digits, and no sequence or
  type prefix of its system ID
