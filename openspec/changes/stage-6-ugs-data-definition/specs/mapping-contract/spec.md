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
IRI SHALL be accepted as that role's object term. The reifier SHALL be minted into its type's IRI space as a handle is
(term-dictionary, "Handle IRIs are held both ways") and take the bucket of its statement's subject.

#### Scenario: A reifier fills a role
- **WHEN** a Gen5 `IsAbout` instance exists and a `GroundedIn` is inserted with its reifier IRI as `statement`
- **THEN** the commit succeeds, and the quad `reifier db:grounded_in ground` names the `IsAbout` instance
