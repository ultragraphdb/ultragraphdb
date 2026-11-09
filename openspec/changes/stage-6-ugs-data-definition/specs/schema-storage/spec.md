## MODIFIED Requirements

### Requirement: Writes to an unknown predicate create its schema
A write to a predicate the catalog version in effect does not declare SHALL, in a tenant with no schema and in a
governed tenant where `@open` on the schema or on the subject's type admits it, be validated with the default entry, kind
either, multi-valued, no datatype constraint and no index, and SHALL ask the metadata group to record that entry as a
default-entry record keyed by (tenant, predicate), a command that finds the record when it already exists and creates
no catalog version; a write that fails SHALL ask for none. No write SHALL wait for the command; on recovery a partition
SHALL send it again for every undeclared predicate its replayed rounds wrote, which the command's finding an existing
record makes harmless; and concurrent first writes of one predicate SHALL record the same default entry, so neither
depends on the other (commit-rounds, "Catalog changes, drops and index builds are ordered in rounds"). Every other write to an undeclared
predicate in a governed tenant SHALL be refused with `UGS1001` and SHALL ask for no record.

#### Scenario: A first write records the predicate
- **WHEN** `<a> <nickname> "Al"` is written and `nickname` is not declared
- **THEN** once the command is answered the metadata group holds the default-entry record of `nickname`, and the
  catalog reads `nickname` with the default entry

#### Scenario: Recovery asks again for a default entry
- **WHEN** a commit first writes the undeclared `alias` and the process is killed before its default-entry command is
  sent, and the partition recovers
- **THEN** replay sends the command for `alias`, and once it is answered the metadata group holds its record

#### Scenario: A failed write records no predicate
- **WHEN** a write naming `<alias>` fails before it commits and `alias` was not declared
- **THEN** the catalog does not hold `alias`

#### Scenario: Concurrent first writes both commit
- **WHEN** two writes read before either commits, the first writes `<a> <score> 12` and the second `<b> <score> "high"`,
  and `score` was not declared
- **THEN** both commit, and the catalog holds `score` with the default entry

#### Scenario: A governed tenant records nothing for a refused predicate
- **WHEN** Gen5 is in effect and a write names the undeclared `db:nickname` on a `Company`
- **THEN** the commit is refused with `UGS1001`, and the metadata group holds no default-entry record of `db:nickname`

#### Scenario: An open type records its undeclared attribute
- **WHEN** a schema declares the entity type `Note` `@open` and a write names an undeclared predicate on a `Note`
- **THEN** the commit succeeds, and the metadata group holds the predicate's default-entry record
