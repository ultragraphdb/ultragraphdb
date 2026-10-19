## ADDED Requirements

### Requirement: Exports read one timestamp across groups
An export SHALL read every group at one timestamp and write, per group, the data as gzipped RDF or JSON and the
schema, in Dgraph's formats and file names, without the GraphQL schema file.

#### Scenario: An RDF export matches Go Dgraph's
- **WHEN** the same data is exported as RDF from ultragraphdb and from Go Dgraph
- **THEN** the files hold the same N-Quads, namespaces and facets, after sorting and normalising uids

#### Scenario: A JSON export lists namespaces
- **WHEN** data in two namespaces is exported as JSON
- **THEN** each object carries its `namespace`

### Requirement: Exports load back
An export SHALL load into an empty cluster through the live loader and reproduce the data and schema.

#### Scenario: An export round-trips through the live loader
- **WHEN** an RDF export and its schema are loaded into an empty cluster
- **THEN** every query of the test suites returns what it returned on the source
