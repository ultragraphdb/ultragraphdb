## ADDED Requirements

### Requirement: A changeset describes a write
A changeset SHALL name its namespace and hold, in order: statements to insert, statements to delete, deletes by pattern
(every object of a subject and predicate in a graph), statement-property sets and deletes, and catalog declarations. Its
terms SHALL be RDF terms, resolved to term IDs when it is applied.

#### Scenario: A delete by pattern removes every object
- **WHEN** `a` holds three objects of `nick` and a changeset deletes the pattern `a`, `nick` in the default graph
- **THEN** `a` holds no `nick`, and the reverse lists of the three objects no longer hold `a`

### Requirement: A changeset applies whole or not at all
Applying a changeset SHALL write all of it in its commit batch, or nothing of it when the catalog refuses any of its
operations.

#### Scenario: One refused operation refuses the changeset
- **WHEN** a changeset inserts `<a> <knows> <b>` and `<a> <knows> "Bob"` and `knows` is declared an edge
- **THEN** the changeset is refused, and `<a> <knows> <b>` is not held

### Requirement: A changeset reports its regions
Applying a changeset SHALL report the regions of every key it writes and of every key it reads, so that stage 5 can
build read summaries from them.

#### Scenario: Written regions are reported
- **WHEN** a changeset inserting `<a> <knows> <b>` is applied
- **THEN** the reported write regions include the forward and reverse regions of `knows` and the subject- and
  object-predicate regions of the default graph

### Requirement: Applying a changeset is deterministic
Applying the same changeset to the same stored state SHALL produce the same writes, byte for byte.

#### Scenario: Two applications write the same bytes
- **WHEN** one changeset is applied to two copies of one store
- **THEN** both commit batches hold the same keys, versions and values
