## REMOVED Requirements

### Requirement: Commit versions follow the clock
**Reason**: Versions are built from the sequencer's epochs, so a snapshot at an epoch is one bound on the version in
every partition (P13).
**Migration**: "Commit versions are built from the epoch".

## ADDED Requirements

### Requirement: Commit versions are built from the epoch
Each admitted transaction SHALL receive a commit version whose upper 42 bits are its partition's open epoch (sequencer,
"Partitions learn each closed epoch and stamp the open one") and whose lower 22 bits are an even sequence within that
epoch, strictly increasing within the partition. The open epoch SHALL be the larger of one above the latest close
delivered to the partition and the partition's own epoch, so a commit is never stamped with a closed epoch. A round
SHALL record its first version and transaction count. When the sequence of the open epoch would pass its 22 bits, the
partition SHALL advance its epoch by one on its own, as a logical clock, and continue from sequence 0.

#### Scenario: A version carries the open epoch
- **WHEN** epoch 9 is the latest close delivered to a partition and it admits its first transaction since
- **THEN** the version's upper 42 bits are 10 and its lower 22 bits are 0

#### Scenario: Versions increase within an epoch
- **WHEN** a partition admits three transactions in epoch 10
- **THEN** their sequences are 0, 2 and 4

#### Scenario: A full epoch advances the partition's epoch
- **WHEN** a partition's sequence in epoch 10 reaches 2^22 − 2 and it admits another transaction
- **THEN** the transaction's version carries epoch 11 and sequence 0

#### Scenario: A late close does not lower the open epoch
- **WHEN** a partition has advanced on its own to epoch 11 and the close of epoch 9 is delivered
- **THEN** its next commit still carries epoch 11
