## MODIFIED Requirements

### Requirement: A storage process hosts partitions over bucket ranges
A storage process SHALL host the partitions the placement map gives it, each with its own log, its own commit step and
its own keyspaces, holding the data of its bucket ranges and nothing else. An entity's home SHALL be the partition the
placement map gives its bucket. A partition SHALL be the same code whether a process hosts one or many. The storage
binary SHALL host one partition unless configured otherwise. The system group SHALL form the placement when the metadata
group holds none, an even map over `[cluster] partition_count`. A storage process SHALL claim the partitions `[storage]
partition_first` and `partition_count` name, by a Join at the system group's address, and any number of processes SHALL
host the partitions between them; a claim on a partition outside the placement, or one another address holds, SHALL be
refused. In this stage the placement SHALL then stay fixed, a process holding its map for its lifetime.

#### Scenario: The system group forms the placement
- **WHEN** the system group's host with `partition_count` 4 starts on a metadata group holding no placement
- **THEN** the metadata group holds an even map over 4 partitions, and a restart forms none

#### Scenario: Two storage processes share the partitions
- **WHEN** one storage process claims partitions 1 and 2 and another claims 3 and 4
- **THEN** both join, Members names each partition's address, and a write reaches its partition on either process

#### Scenario: A claim on a held partition is refused
- **WHEN** one storage process holds partition 2 and another claims it
- **THEN** the claim is refused with an error naming partition 2 and its holder, and the placement is unchanged

#### Scenario: A restart claims its partitions again
- **WHEN** a storage process holding partitions 1 and 2 restarts at the same addresses
- **THEN** its claim succeeds
