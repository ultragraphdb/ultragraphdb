## MODIFIED Requirements

### Requirement: Posting operations and identities
Insert, delete and single-valued overwrite SHALL apply to a list as a set of objects. An IRI or blank-node object SHALL
be identified by its UID. A literal object SHALL be identified by its encoded term: the posting's 64-bit fingerprint of
the encoding orders the list, and postings whose fingerprints are equal SHALL be told apart by their encodings, so a
fingerprint collision never merges two literals. Literals that differ in language tag SHALL be distinct objects.

#### Scenario: A single-valued predicate replaces its object
- **WHEN** `name` is declared single-valued and `<a> <name> "A"` is written and then `<a> <name> "B"`
- **THEN** the list holds one posting, `"B"`

#### Scenario: Language-tagged literals coexist
- **WHEN** `<a> <label> "A"`, `<a> <label> "Á"@fr` and `<a> <label> "B"@fr` are written to a multi-valued `label`
- **THEN** the list holds all three

#### Scenario: A multi-valued predicate accumulates
- **WHEN** `<a> <knows> <b>` and then `<a> <knows> <c>` are written
- **THEN** the list holds `b` and `c`

#### Scenario: Colliding fingerprints keep both literals
- **WHEN** two different literals whose encodings have the same 64-bit fingerprint are written to one list
- **THEN** the list holds both, and deleting one leaves the other

#### Scenario: Deleting an object the list does not hold changes nothing
- **WHEN** `a` holds `"A"` for `name` and `<a> <name> "B"` is deleted
- **THEN** `a` still holds `"A"`
