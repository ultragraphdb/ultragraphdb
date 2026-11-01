## REMOVED Requirements

### Requirement: Commit versions follow the clock
**Reason**: Versions are built from the sequencer's epochs, so a snapshot at an epoch is one bound on the version in
every partition (P13).
**Migration**: "Commit versions are built from the epoch".

## ADDED Requirements

### Requirement: Commit versions are built from the epoch
Each admitted transaction SHALL receive a commit version whose upper 42 bits are the highest epoch delivered to its
partition and whose lower 22 bits are an even sequence within that epoch, strictly increasing within the partition. A
round SHALL record its first version and transaction count. When the sequence of an open epoch would pass its 22 bits,
the partition SHALL advance its epoch by one on its own, as a logical clock, and continue from sequence 0; the sequencer
SHALL never close an epoch below one a partition has stamped.

#### Scenario: A version carries the delivered epoch
- **WHEN** epoch 9 is the highest delivered to a partition and it admits its first transaction of epoch 9
- **THEN** the version's upper 42 bits are 9 and its lower 22 bits are 0

#### Scenario: Versions increase within an epoch
- **WHEN** a partition admits three transactions in epoch 9
- **THEN** their sequences are 0, 2 and 4

#### Scenario: A full epoch advances the partition's epoch
- **WHEN** a partition's sequence in epoch 9 reaches 2^22 − 2 and it admits another transaction
- **THEN** the transaction's version carries epoch 10 and sequence 0, and the sequencer's next close is at least 10
