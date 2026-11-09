## MODIFIED Requirements

### Requirement: Data requests, prepared statements and describe
A read SHALL be a quad pattern with each position bound or unbound, a limit and a resume key, naming its transaction,
whose Begin names the level, session token and deadline the read takes; a row SHALL name its statement by its quad and,
when the statement holds a statement ID, its reifier IRI, never by the statement ID. A write SHALL be a changeset, in
its transaction's tenant, addressing a statement's properties, or one of several parallel edges, through the
statement's reifier IRI, and a statement without one by its triple term and graph; a commit-time operation SHALL be its
own request, and its result in the Commit reply SHALL name the node it found or created by its IRI or skolem IRI. A
tenant SHALL bind per Begin, so one connection's transactions MAY act in several. A statement SHALL be prepared once per
connection, which holds it, in the tenant its Prepare names, from a quad pattern with parameter slots, answered with a
statement id, its parameter and result column types and a schema id, and executed with parameters bound by position; Execute
SHALL refuse a statement prepared in another tenant than its transaction's with code 10 (UnknownStatement), `detail` the
statement; after a new catalog version of its tenant its schema id SHALL no longer match, and an execution SHALL be refused as
stale, while recording a default entry (metadata-group, "Catalog versions take effect at an epoch boundary") SHALL keep
the schema id; a literal bound at a slot that takes a node SHALL be refused. A resume key SHALL continue its own request
only, and rows SHALL come in the store's key order. The frontend SHALL answer Describe itself from the metadata group, asking
no other role, as far as the connection's principal may read it, so it names what is declared and recorded rather than what
data holds: `read_version` the version bound of the latest epoch the sequencer closed, or of epoch 0 before the first close;
the entity types declared in effect at that epoch; the predicates of kind edge or either that it declares or holds a default
entry for as relationship types, those of kind property or either as property keys; and the catalog's declarations, which for a governed tenant
are the record kinds `PROTOCOL.md` section 8.3 gives the entries of `SCHEMA.md` section 10.2, in the catalog version's
order, with the installed sources. An answer not fitting one frame within the client's `max_frame` SHALL travel in
continuation frames, the last flagged `LAST`.

#### Scenario: A stale statement is refused
- **WHEN** a statement is prepared, the catalog of its tenant changes, and the statement is executed
- **THEN** the execution is refused as stale, and preparing it again succeeds

#### Scenario: Describe answers from the metadata group
- **WHEN** `name` is declared a property with a `term` index, `score` holds a default entry, and a client sends Describe
  for the tenant
- **THEN** the frontend answers asking no other role: `read_version` is the version bound of the latest closed epoch,
  the declarations list `name` with its `term` index, the property keys hold `name` and `score`, and the relationship
  types hold `score`

#### Scenario: A literal at a node slot is refused
- **WHEN** a statement's subject parameter is bound to a literal
- **THEN** the execution is refused with code 11, `detail` the statement

#### Scenario: A commit-time operation's node is named by its IRI
- **WHEN** a transaction's insert-if-absent finds the node `http://ex.org/x` holding the value, and the transaction
  commits
- **THEN** the Commit reply names `http://ex.org/x` by its text, and holds no system ID

#### Scenario: A Describe past the client's frame continues
- **WHEN** a tenant's Describe answer does not fit one frame within the client's `max_frame`
- **THEN** it arrives in continuation frames whose sections concatenate to the whole answer, the last flagged `LAST`

#### Scenario: A statement prepared in another tenant is refused
- **WHEN** a connection prepares a statement in tenant 1 and executes it in a transaction of tenant 0
- **THEN** the execution is refused with code 10, `detail` the statement, and the transaction goes on

#### Scenario: One connection's transactions act in two tenants
- **WHEN** a connection begins one transaction in tenant 0 and another in tenant 1, and each writes and reads `<a> <p>
  <b>`
- **THEN** each reads its own statement, and neither reads the other's

## ADDED Requirements

### Requirement: The frames follow PROTOCOL.md as extended for UGS
`ultragraphdb_protocol_wire` SHALL read and build, as `PROTOCOL.md` lays them out: the warnings structure of section
4.9 in Result's first batch and in Prepare's and Install's replies; Describe's entry kinds 1 to 20 for the catalog entries, the sources and the retired names, its request of nine
sections and its continuation frames (section 8.3); Install with `staged`, the migration report's
section and the warnings' sections, and a source's text (section 8.4); Reference's `mode` and `status`, a retracted participant's among them (section 13.15);
the error codes 30 (EntityDeleted) and 31 (Fenced); the datatypes 34 to 36 with their tags and by-value payloads
(sections 10.1, 10.4, 10.5); Catalog's continuation frames and the limit `catalog_bytes_max` (sections 11.9, 13.7); and
Purge (24) and Migrate (25), whose frames decode and are refused with code 7 naming their stage. Every decoder SHALL
refuse a malformed body with the refusal its section's order names and SHALL never panic. Each golden frame of Result,
Prepare, Describe, Install and Reference SHALL be regenerated by its generator.

#### Scenario: An Install reply carries its warnings
- **WHEN** an Install reply with one warning `UGS0902` is built and read back
- **THEN** the view answers the version, the epoch, the report and one warning whose `diagnostic` is 902 in language
  4, with its source, location and message

#### Scenario: A request with a report is refused
- **WHEN** an Install request's section 2 is not empty
- **THEN** it is refused with `CountMismatch` naming `report`

#### Scenario: Purge and Migrate decode and are refused by stage
- **WHEN** a client sends a well-formed Purge and a well-formed Migrate
- **THEN** each is refused with code 7 on its stream, the message naming the command and its stage, and the connection
  stays open

#### Scenario: The golden frames match their generators
- **WHEN** the conformance suite reads every golden frame of Result, Prepare, Describe, Install and Reference
- **THEN** each decodes, and building it again from its view gives the same bytes

### Requirement: Query carries UQL as language 3
Query (22) and Prepare (10) SHALL take `language` 3 for UQL, beside 1 for GQL and 2 for SPARQL; any other value SHALL
be refused with `FieldValue` naming `language`. A refusal of a UQL text SHALL be code 29 with the diagnostic's number
in `detail` and 3 in its bits 16 to 23.

#### Scenario: Language 3 is read and built
- **WHEN** a Query with `language` 3, the text `Company { id }` and no parameters is built and read back
- **THEN** the view answers language 3 and the text

#### Scenario: Language 4 is refused
- **WHEN** a Query names `language` 4
- **THEN** it is refused with `FieldValue` naming `language`

### Requirement: Operation deletes and retracts an instance
Operation (9) SHALL take kind 5, delete an instance, and kind 6, retract an instance, each with two args, the graph, a
term or `ABSENT`, and the instance's IRI, and with `delta`, `properties` and `links` zero. Its result in the Commit
reply SHALL hold `node` `ABSENT`, `value` 0 and `created` 0, as a compare-and-set's does. A ConditionalWrite's
operations SHALL take both kinds. A kind above 6 SHALL be refused with `FieldValue`
naming `kind`.

#### Scenario: A retract operation is read and built
- **WHEN** an Operation of kind 6 naming the default graph and `https://example.org/deep_brain#Node/ORG%3Aacme` is
  built and read back
- **THEN** the view answers kind 6, the graph `ABSENT` and the IRI

#### Scenario: A delete operation with a third arg is refused
- **WHEN** an Operation of kind 5 carries three args
- **THEN** it is refused with `CountMismatch` naming `args`
