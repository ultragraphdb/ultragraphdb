## ADDED Requirements

### Requirement: The store is reached through one trait
Everything above the store SHALL reach it through the `Store` trait, which has two implementations: fjall for
production and an in-memory store for the simulator; outside the fjall implementation, no storage code SHALL name fjall,
read the wall clock, spawn a thread or draw from an ambient random source.

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
The simulated store SHALL inject, as the run's fault stream draws them: a write batch failing before or after it is
applied, a process crash losing what was not handed to the operating system, a power loss losing what was not synced, a
failed read, a corrupted stored value, and a slow operation.

#### Scenario: A power loss keeps every synced batch
- **WHEN** batches are written, some synced, and a power loss is injected
- **THEN** after recovery every synced batch is visible whole, no unsynced batch is visible in part, and the applied
  timestamp names the last batch the store holds

#### Scenario: A corrupted value is refused
- **WHEN** a stored posting list's bytes are corrupted and the list is read
- **THEN** the read fails with an error naming the key, and no value is returned

### Requirement: Invariants hold after every step
After every step and every recovery the simulator SHALL check the store against a shadow model: a batch visible whole
or not at all, every synced batch present, a read returning only a value written at or below its timestamp, a dropped
prefix absent, the applied timestamp naming the last batch held.

#### Scenario: A violated invariant names its seed
- **WHEN** a run breaks an invariant
- **THEN** the simulator stops and reports the invariant, the step and the seed
