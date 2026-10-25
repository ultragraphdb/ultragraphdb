## ADDED Requirements

### Requirement: A keyspace without versions
The store SHALL provide a keyspace whose keys carry no version: a point read returns the key's one value, a lookup of an
absent key is answered by fjall's bloom filters, and a write replaces the value. Writes to it SHALL be entries of the
same atomic write batch as versioned writes, so a batch is visible whole or not at all across both, and drops of
everything SHALL clear it. Reads of it SHALL not be bounded by the discard timestamp.

#### Scenario: A versionless write is atomic with versioned writes
- **WHEN** a batch writes a versioned key and a key without a version, and the process crashes during the batch
- **THEN** after reopening, either both are found or neither is

#### Scenario: An absent key is a point read
- **WHEN** a key that was never written is read from the keyspace without versions
- **THEN** the read returns nothing, through one point lookup

#### Scenario: Dropping everything clears it
- **WHEN** keys without versions are written and everything is dropped
- **THEN** none of them is found
