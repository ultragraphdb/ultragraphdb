# changesets Specification

## Purpose
Changesets, the write format the engines produce and storage applies: a changeset's inserts, deletes and
declarations, applied to the quads as one request, committed whole or not at all.
## Requirements
### Requirement: A changeset describes a write
A changeset SHALL name its tenant and hold, in order: statements to insert, statements to delete, deletes by pattern
(every object of a subject and predicate in a graph), statement-property sets and deletes, and catalog declarations. Its
terms SHALL be RDF terms, resolved to term IDs when the transaction holding it executes, against its snapshot; a term
new to the transaction SHALL take a provisional UID, confirmed by the commit step.

#### Scenario: A delete by pattern removes every object
- **WHEN** `a` holds three objects of `nick` and a changeset deletes the pattern `a`, `nick` in the default graph
- **THEN** `a` holds no `nick`, and the reverse lists of the three objects no longer hold `a`

#### Scenario: A provisional UID is confirmed at commit
- **WHEN** a transaction creates `<new>` and commits, and no other transaction created `<new>` meanwhile
- **THEN** `<new>` holds the UID the transaction gave it

### Requirement: A changeset applies whole or not at all
Applying a changeset SHALL write all of it in its commit batch, or nothing of it when the catalog refuses any of its
operations.

#### Scenario: One refused operation refuses the changeset
- **WHEN** a changeset inserts `<a> <knows> <b>` and `<a> <knows> "Bob"` and `knows` is declared an edge
- **THEN** the changeset is refused, and `<a> <knows> <b>` is not held

### Requirement: A changeset reports its regions
Executing a changeset SHALL report its read set (the lists read whole, the statements tested, the ranges scanned, the
dictionary keys found absent and the tenant's catalog range) and its write set (the keys written, with the objects
of each list it changed by no more objects than the commit step's bound), which the commit step validates, and the
regions of every key it reads and writes.

#### Scenario: Written regions are reported
- **WHEN** a changeset inserting `<a> <knows> <b>` is executed
- **THEN** the reported write regions include the forward and reverse regions of `knows` and the subject- and
  object-predicate regions of the default graph

#### Scenario: An insert records the statement it tested
- **WHEN** a changeset inserting `<a> <knows> <b>` is executed and `a` holds other objects of `knows`
- **THEN** its read set holds the statement `a`, `knows`, `b`, not the whole list of `knows` of `a`

### Requirement: Applying a changeset is deterministic
Applying the same changeset to the same stored state SHALL produce the same writes, byte for byte.

#### Scenario: Two applications write the same bytes
- **WHEN** one changeset is applied to two copies of one store
- **THEN** both commit batches hold the same keys, versions and values

