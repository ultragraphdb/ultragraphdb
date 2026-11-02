## MODIFIED Requirements

### Requirement: A changeset describes a write
A changeset SHALL name its namespace and hold, in order: statements to insert, statements to delete, deletes by pattern
(every object of a subject and predicate in a graph), statement-property sets and deletes, and catalog declarations. Its
terms SHALL be RDF terms, resolved when the transaction holding it executes: an IRI or blank node to its system ID, got
or created at its home before the commit (term-dictionary, "An IRI's ID is got or created at its home"), a reference to
an entity another partition owns registering there ("A reference to another partition's entity registers at its home");
a literal to its encoding, with its hash and dictionary entry written blind where the store holds it by hash, since a
literal has no system ID and no home. No ID SHALL be provisional, and creating one SHALL never abort the transaction.

#### Scenario: A delete by pattern removes every object
- **WHEN** `a` holds three objects of `nick` and a changeset deletes the pattern `a`, `nick` in the default graph
- **THEN** `a` holds no `nick`, and the reverse lists of the three objects no longer hold `a`

#### Scenario: A new term's ID is fixed before the commit
- **WHEN** a transaction creates `<new>`, and another transaction writing `<new>` executes before the first commits
- **THEN** both receive the same system ID for `<new>` while executing, and whichever commits holds `<new>` under it

### Requirement: A changeset reports its regions
Executing a changeset SHALL report its read set (the lists read whole, the statements tested, the ranges scanned and
the terms found absent, each recorded as no statement of the term in the transaction's partition) and its write set
(the keys written, with the objects of each list it changed by no more objects than the commit step's bound), which the
commit step validates (transactions, "What a transaction's reads record and what conflicts with them"), and the regions
of every key it reads and writes. No dictionary key SHALL be in the read set.

#### Scenario: Written regions are reported
- **WHEN** a changeset inserting `<a> <knows> <b>` is executed
- **THEN** the reported write regions include the forward and reverse regions of `knows` and the subject- and
  object-predicate regions of the default graph

#### Scenario: An insert records the statement it tested
- **WHEN** a changeset inserting `<a> <knows> <b>` is executed and `a` holds other objects of `knows`
- **THEN** its read set holds the statement `a`, `knows`, `b`, not the whole list of `knows` of `a`

#### Scenario: A new term leaves no dictionary key in the read set
- **WHEN** a changeset inserting `<new> <knows> <b>` is executed and `<new>` had no ID
- **THEN** its read set holds the absent term `<new>` and no dictionary key
