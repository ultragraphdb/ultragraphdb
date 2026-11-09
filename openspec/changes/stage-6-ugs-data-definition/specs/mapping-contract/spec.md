## MODIFIED Requirements

### Requirement: Statements are the canonical model
The store SHALL hold statements of the form (graph, subject, predicate, object, optional statement ID): the graph is
the default graph or an IRI, the subject an IRI or a blank node, the predicate an IRI, and the object an IRI, a blank
node or a literal. A statement ID SHALL be allocated only for a statement that carries properties or that repeats a
subject, predicate and object already held in the same graph. In a governed tenant a statement ID SHALL be allocated also as ugs-views, "A relation type is edge-encoded
or node-encoded by its shape", states: for an instance of a `@multi` or `@bitemporal` relation, of a relation tree with
several concrete types, or of a type a role names.

#### Scenario: A plain triple has no statement ID
- **WHEN** the statement `<a> <knows> <b>` is written to the default graph
- **THEN** it is held once, with no statement ID

#### Scenario: A parallel edge gets its own statement ID
- **WHEN** `<a> <knows> <b>` is written twice to the default graph as two distinct edges
- **THEN** both statements are held, each with its own statement ID

### Requirement: Names map between the views
A property-graph label, property name or edge type SHALL map to the IRI formed by the catalog's base IRI and the name. A
node created through the property-graph view without an IRI SHALL be named by a skolem IRI minted as a blank node's,
`…/.well-known/genid/{bucket}-{random}` (term-dictionary, "IRIs and blank nodes get system IDs"), holding no part of
its system ID beyond its bucket. A blank node SHALL be held as a skolem IRI, marked as skolemized so that the RDF view
presents it as a blank node again. A blank node naming a new keyless instance of a declared type SHALL be held as that instance's handle IRI
and presented as an IRI (term-dictionary, "Handle IRIs are held both ways"); the rule above holds of every other
blank node.

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

### Requirement: Statements round-trip through both views
Statements written through the RDF view SHALL read back through it unchanged, and statements written through the
property-graph view SHALL read back through it unchanged, statement IDs and edge properties included. In a governed tenant the round trip SHALL be `SCHEMA-APG.md` proposition 17.3's: what is read back is the
instance's quads of definition 17.1, its defaults, its supertypes' types and its minted names among them.

#### Scenario: Quads round-trip
- **WHEN** a set of quads over two named graphs, with blank nodes, language-tagged and typed literals, is written and
  read back through the RDF view
- **THEN** the quads read back equal the quads written

#### Scenario: A property graph round-trips
- **WHEN** nodes with labels and properties, and parallel edges with properties, are written and read back through the
  property-graph view
- **THEN** the nodes, labels, properties, edges and edge properties read back equal those written

## ADDED Requirements

### Requirement: A governed tenant's RDF view is the APG's
In a governed tenant the RDF view SHALL be the function `R` of `specification/schema/SCHEMA-APG.md` definition 17.1
over the tenant's instances: the mapping contract's RDF half is that definition, and proposition 17.3's round trip is
its property (ugs-views, "The RDF view presents a governed tenant's instances"). The `rdf:type` of an instance of a
declared type SHALL be answered from its ID's prefix and the catalog's prefix ranges, with no stored quad. An `Iri`
value and an enum member SHALL be literals, `xsd:anyURI` and `xsd:string`, creating no entity. A tenant with no schema
SHALL keep the view this capability states for statements.

#### Scenario: An Iri attribute creates no entity
- **WHEN** a Gen5 `Object` is written with `uri` `"https://example.org/x"^^xsd:anyURI`
- **THEN** the object's triple holds the literal, and the dictionary holds no entity for `https://example.org/x`

#### Scenario: A declared type's rdf:type is not stored
- **WHEN** a `Company` is created and its partition's forward keyspace is listed
- **THEN** no quad with the predicate `rdf:type` is among its entries, and a read of its `rdf:type` answers its types

### Requirement: A relation instance is named through its reifier where a role names it
An edge-encoded relation instance SHALL be allocated a statement ID, carrying its relation's prefix, also when a role
of any relation in the catalog names its type, a supertype of it or a statement type covering it, and its reifier's
IRI SHALL be accepted as that role's term: the object of a node-encoded relation's role triple, and the subject or the
object of an edge-encoded relation's triple. The reifier SHALL be minted into its type's IRI space as a handle is
(term-dictionary, "Handle IRIs are held both ways") and take the bucket of its statement's subject.

#### Scenario: A reifier fills a role
- **WHEN** a Gen5 `IsAbout` instance exists and a `GroundedIn` is inserted with its reifier IRI as `statement`
- **THEN** the commit succeeds, and the quad `reifier db:grounded_in ground` names the `IsAbout` instance

#### Scenario: A reifier fills a role as an object
- **WHEN** a Gen5 `IsAbout` instance exists and a `GroundedIn` is inserted with its reifier IRI as `ground`
- **THEN** the commit succeeds, and the quad `statement db:grounded_in reifier` names the `IsAbout` instance
