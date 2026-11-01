## MODIFIED Requirements

### Requirement: Statement IDs
A statement ID SHALL be a system ID of the `Statement` type, its bucket the bucket of the statement's subject and its
sequence drawn from a block the subject's partition holds, allocated only for a statement that gains a property or
repeats a subject, predicate, object and graph already held. Its forward posting SHALL carry the statement ID, and its
statement record SHALL hold the statement and its properties. Removing the statement SHALL remove its record.

#### Scenario: A property allocates a statement ID
- **WHEN** `<a> <knows> <b>` is held without a statement ID and gains the property `<since> 2020`
- **THEN** the statement has a statement ID of type prefix 2 and the bucket of `<a>`, its forward posting carries it, and
  its record holds `<since> 2020`

#### Scenario: Removing a statement removes its record
- **WHEN** a statement with a statement ID is deleted
- **THEN** no statement record of that ID remains
