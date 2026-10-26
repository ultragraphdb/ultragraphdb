# mapping-contract Specification

## Purpose
The mapping contract between RDF and the property graph: statements as the canonical model, statement IDs, and
the RDF and property-graph views of one dataset.
## Requirements
### Requirement: Statements are the canonical model
The store SHALL hold statements of the form (graph, subject, predicate, object, optional statement ID): the graph is
the default graph or an IRI, the subject an IRI or a blank node, the predicate an IRI, and the object an IRI, a blank
node or a literal. A statement ID SHALL be allocated only for a statement that carries properties or that repeats a
subject, predicate and object already held in the same graph.

#### Scenario: A plain triple has no statement ID
- **WHEN** the statement `<a> <knows> <b>` is written to the default graph
- **THEN** it is held once, with no statement ID

#### Scenario: A parallel edge gets its own statement ID
- **WHEN** `<a> <knows> <b>` is written twice to the default graph as two distinct edges
- **THEN** both statements are held, each with its own statement ID

### Requirement: The RDF view
The RDF view SHALL present every statement as a quad of its graph, subject, predicate and object. Statements that share
a subject, predicate, object and graph SHALL present as one quad. A statement ID SHALL present as the statement's
reifier and its properties as annotations on the reifier, as RDF 1.2 defines them.

#### Scenario: Parallel edges are one quad
- **WHEN** `<a> <knows> <b>` is held twice with two statement IDs
- **THEN** the RDF view holds the quad `<a> <knows> <b>` once, with two reifiers

#### Scenario: An edge property is an annotation
- **WHEN** the statement `<a> <knows> <b>` carries the property `<since> "2020"^^xsd:gYear`
- **THEN** the RDF view holds the quad and a reifier of it annotated with `<since> "2020"^^xsd:gYear`

### Requirement: The property-graph view
The property-graph view SHALL present an IRI or blank node as a node with its identity; an `rdf:type` statement as a
label of its subject; a statement with a literal object as a property of its subject, scalar when the catalog declares
the predicate single-valued and a list otherwise; a statement with an IRI or blank-node object as an edge; each
statement ID as a separate edge; and a statement ID's properties as the edge's properties.

#### Scenario: A label is an rdf:type statement
- **WHEN** `<a> rdf:type <Person>` is held
- **THEN** the property-graph view shows node `a` with the label `Person`

#### Scenario: Parallel edges stay separate
- **WHEN** `<a> <knows> <b>` is held twice with two statement IDs
- **THEN** the property-graph view shows two `knows` edges from `a` to `b`

#### Scenario: A multi-valued property is a list
- **WHEN** `<a> <nick> "Al"` and `<a> <nick> "Ally"` are held and `nick` is not declared single-valued
- **THEN** the property-graph view shows `nick` on `a` as a list of both values

### Requirement: Names map between the views
A property-graph label, property name or edge type SHALL map to the IRI formed by the catalog's base IRI and the name. A
node created through the property-graph view SHALL be named by an IRI under the base IRI and its UID. A blank node
SHALL be held as a skolem IRI, marked as skolemized so that the RDF view presents it as a blank node again.

#### Scenario: A property name is an IRI under the base
- **WHEN** the base IRI is `http://ex.org/` and a property `age` is written through the property-graph view
- **THEN** the RDF view holds it under the predicate `<http://ex.org/age>`

#### Scenario: A blank node round-trips as a blank node
- **WHEN** `_:x <knows> <b>` is written through the RDF view and read back through it
- **THEN** the subject is presented as a blank node, the same blank node for every statement that named `_:x`

### Requirement: Literals are kept whole
A literal SHALL be held with its lexical form, datatype, language tag and base direction, and the RDF view SHALL return
it exactly as written. A literal whose lexical form is not valid for its datatype SHALL be held unchanged.

#### Scenario: A non-canonical integer keeps its lexical form
- **WHEN** `<a> <n> "01"^^xsd:integer` is written
- **THEN** the RDF view returns `"01"^^xsd:integer`, a term distinct from `"1"^^xsd:integer`

#### Scenario: An ill-typed literal is kept
- **WHEN** `<a> <n> "twelve"^^xsd:integer` is written
- **THEN** the RDF view returns `"twelve"^^xsd:integer` unchanged

### Requirement: The contract is versioned
The mapping contract SHALL carry a version, and the store SHALL record the version it was written under. Opening a store
written under another major version SHALL be refused with an error naming both versions.

#### Scenario: A store of another major version is refused
- **WHEN** a store written under contract version 2 is opened by a build implementing version 1
- **THEN** opening fails with an error naming contract versions 2 and 1, and nothing is written

### Requirement: Statements round-trip through both views
Statements written through the RDF view SHALL read back through it unchanged, and statements written through the
property-graph view SHALL read back through it unchanged, statement IDs and edge properties included.

#### Scenario: Quads round-trip
- **WHEN** a set of quads over two named graphs, with blank nodes, language-tagged and typed literals, is written and
  read back through the RDF view
- **THEN** the quads read back equal the quads written

#### Scenario: A property graph round-trips
- **WHEN** nodes with labels and properties, and parallel edges with properties, are written and read back through the
  property-graph view
- **THEN** the nodes, labels, properties, edges and edge properties read back equal those written

