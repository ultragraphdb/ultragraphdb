## ADDED Requirements

### Requirement: The store is reached through one trait
Everything above the store SHALL reach it through the `Store` trait of `ultragraphdb_traits_store`, which has two
implementations: fjall for production and an in-memory store for the simulator. Background work SHALL run on an
`Executor` the caller passes, and the posting cache SHALL sit behind the `ListCache` trait; outside
`ultragraphdb_store_kv`, no storage code SHALL name fjall or moka, read the wall clock, spawn a thread or draw from an
ambient random source.

#### Scenario: The posting lists run on the simulated store
- **WHEN** the posting-list operations of a workload run on the simulated store
- **THEN** they pass the same checks as on fjall

### Requirement: A simulation run is a function of its seed
The simulator SHALL derive the workload, the faults and the order in which it steps the apply loop and its readers from
one seed, through separate streams, and SHALL advance time only itself; a failing run SHALL print its seed, and running
that seed again SHALL reproduce the run step for step.

#### Scenario: A seed replays its run
- **WHEN** a seed is run twice
- **THEN** both runs perform the same operations, inject the same faults and end in the same state

### Requirement: The simulator injects storage faults
The simulated store SHALL inject, as the run's fault stream draws them: a failed write batch, which poisons the store
until recovery; a process crash, which may lose the batch being written; a power loss, which may lose a suffix of the
batches written after the last sync, the surviving batches being a prefix of batch order; a crash or power loss during
a drop; a read that fails, at once or partway through a scan; a corrupted stored value; and a slow operation. After a
failed batch, the only next step SHALL be recovery, and the fault stream SHALL decide whether the failed batch is
present, whole, or absent, whole, after it.

#### Scenario: A power loss keeps every synced batch
- **WHEN** batches are written, some synced, and a power loss is injected
- **THEN** after recovery every synced batch is visible whole, the surviving batches are a prefix of batch order, no
  batch is visible in part, and the applied timestamp is the highest commit timestamp the store holds

#### Scenario: A failed batch poisons the store
- **WHEN** a batch fails during its persist, a write follows, and the store is reopened
- **THEN** the write before reopening is refused, and after reopening the failed batch is present whole or absent whole

#### Scenario: A drop interrupted by a power loss finishes at recovery
- **WHEN** a drop of everything has synced its record and a power loss is injected before it deletes the record
- **THEN** after recovery the store holds only the entries the drop was given

#### Scenario: A corrupted value is refused
- **WHEN** a stored posting list's bytes are corrupted and the list is read
- **THEN** the read fails with an error naming the key, and no value is returned

### Requirement: Invariants hold after every step
After every step and every recovery the simulator SHALL check the store against a shadow model: a batch visible whole
or not at all, every synced batch present, the surviving batches a prefix of batch order, a read returning only a value
written at or below its timestamp, a dropped prefix absent, no write landing below a deletion of the same key, and the
applied timestamp the highest commit timestamp held, which rollup and drop batches leave unchanged.

#### Scenario: A violated invariant names its seed
- **WHEN** a run breaks an invariant
- **THEN** the simulator stops and reports the invariant, the step and the seed
