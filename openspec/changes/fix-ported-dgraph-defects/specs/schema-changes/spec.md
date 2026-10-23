## ADDED Requirements

### Requirement: A namespace's data drop removes its split parts
Dropping a namespace's data SHALL remove its split-list parts (prefix `0x04`) with its data (prefix `0x00`), in the
same durable step, as Dgraph's index, count and reverse drops pair each prefix with its split twin. Dgraph's `DeleteData`
removes the data prefix alone (ledger 7).

#### Scenario: Dropping a namespace's data leaves no split part
- **WHEN** a namespace whose lists are split has its data dropped
- **THEN** no key under `0x00` or `0x04` of the namespace remains, and its schema and type keys are unchanged
