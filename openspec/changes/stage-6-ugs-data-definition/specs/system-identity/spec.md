## RENAMED Requirements

- FROM: `### Requirement: Until UGS keys exist, an IRI is the key of the Resource type`
- TO: `### Requirement: An IRI is the key of the Resource type`

## MODIFIED Requirements

### Requirement: An IRI is the key of the Resource type
Every IRI-named entity that is no instance of a declared type SHALL be of the built-in type `Resource`, whose key is
the IRI, in a tenant with no schema and where an `@open` schema admits it, and every statement that is no declared
relation's instance SHALL be of the built-in type `Statement`; their prefixes SHALL be reserved before any declared type: prefix 0 invalid,
1 `Resource`, 2 `Statement`. The canonical encoding of an IRI key SHALL be its UTF-8 bytes as RDF 1.2 writes the IRI,
without surrounding angle brackets, prefixed by its length as a 4-byte big-endian count: the encoding of a key of one
component (ugs-identity, "Keys have one canonical encoding"), so governed and ungoverned data hash alike.

#### Scenario: The canonical encoding of an IRI
- **WHEN** the IRI `http://ex.org/a` is canonically encoded
- **THEN** the bytes are `00 00 00 0F` followed by the 15 UTF-8 bytes of `http://ex.org/a`

#### Scenario: The reserved prefixes
- **WHEN** a new IRI and a new statement receive IDs in a tenant with no schema
- **THEN** the IRI's type prefix is 1 and the statement's is 2

## ADDED Requirements

### Requirement: An instance of a declared type carries its type's prefix
In a governed tenant an instance's system ID SHALL take the type prefix of its concrete type at creation and keep it
for life, its sequence drawn from the block its partition leases for that type. A keyed instance's bucket SHALL be
computed from its key space's prefix and its canonical key, a keyless entity's SHALL be the one its handle states, and
an unkeyed relation instance's its anchor participant's (ugs-identity, "Buckets and claims come from the key space").

#### Scenario: A declared instance's ID
- **WHEN** an aviation `Engine` with `serial` `EN-4471-B` is created
- **THEN** its ID's type prefix is 5, `Engine`'s, and its bucket is the one computed from prefix 3, `Asset`'s, and the
  canonical key

#### Scenario: A keyless entity's bucket is its handle's
- **WHEN** a partition creates a keyless entity and mints its handle
- **THEN** the entity's ID carries the bucket the handle's first six digits state
