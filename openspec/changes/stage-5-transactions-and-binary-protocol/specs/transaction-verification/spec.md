## ADDED Requirements

### Requirement: A Lean model of the round protocol
A Lean 4 model SHALL model rounds, exact validation, commit-time operations, the watermark, the log's sync, crash and
recovery, and sessions, and SHALL prove, for every number of transactions, rounds and crashes: that committed
transactions are strictly serializable; that no acknowledged commit is lost across a crash, under the model's stated
assumption that a completed sync is durable; that replaying a round is idempotent; and that a session's request
commits at most once. The model SHALL live in the repository's `lean/` folder, the Lake project that holds its Lean
proofs, as the library `RoundModel`; it SHALL be built with `lake` outside Bazel, pin its toolchain, and contain no
`sorry`.

#### Scenario: The model builds without gaps
- **WHEN** `lake build` runs in `lean/`
- **THEN** it succeeds, and no proof in it uses `sorry`

### Requirement: The node simulator
A deterministic simulator SHALL run workers, the commit step, the log over simulated storage, the simulated store, a
network that drops, delays, duplicates and splits frames, and a stepped clock, from a seed; a seed SHALL replay its run.
After every step and every recovery it SHALL check that every acknowledged commit is present, except one in a round
the storage tore while reporting success, which recovery must have reported by name; that the store equals the replay
of the log; and the invariants that restate the Lean model's theorems over the run's trace.

#### Scenario: A power loss mid-round loses no acknowledged commit
- **WHEN** a seed injects a power loss while a round is being synced and the node recovers
- **THEN** every commit acknowledged before the power loss is present

### Requirement: History checkers
Elle's list-append and read-write register workloads SHALL run through the Rust client against the server, with
real-time edges, and SHALL find no anomaly. A white-box checker SHALL read every read's and every commit's version from
the log and the simulator's trace and check the serialization graph with real-time edges.

#### Scenario: Elle finds no anomaly
- **WHEN** Elle's list-append workload runs at 8 concurrent clients against the server
- **THEN** Elle reports the history strictly serializable

### Requirement: Every decoder of untrusted or stored bytes is fuzzed
Every function that decodes bytes from a client, a peer or the disk SHALL have a `cargo-fuzz` target: the frame
header and its validation, the chunk table, every body's view, the round and write-set record, the posting format,
the WAL scan over arbitrary segment bytes, and the superblock. Each target SHALL assert that the decoder returns a
value or its typed error and never panics, that a value it accepts re-encodes to the bytes it read where the format
has one encoding, and that the WAL scan never accepts a round whose checksums fail. Targets SHALL be seeded with the
conformance suite's golden frames and the format's golden values. `scripts/fuzz.sh` SHALL run them, time-boxed, outside
Bazel; every input that made a target fail SHALL be minimized and committed as a regression case that a unit test
replays under Cargo and Bazel. Each target SHALL itself be audited: defects typical of a decoder, planted in a
throwaway copy, SHALL each be found by the target within its time budget, or the target SHALL be strengthened.

#### Scenario: A fuzz target finds a planted defect
- **WHEN** a throwaway copy of the posting decoder skips the bound on a postings length behind a valid checksum, and its
  fuzz target runs for its time budget
- **THEN** the target reports a failing input, and the time it took is recorded

#### Scenario: A fuzzer finding becomes a regression test
- **WHEN** a fuzz target finds an input that panics a decoder, and the decoder is fixed
- **THEN** the minimized input is committed beside the crate's tests, and a unit test replaying it passes under Cargo and
  Bazel

#### Scenario: A time-boxed run reports per target
- **WHEN** `scripts/fuzz.sh ultragraphdb_protocol_wire` runs with its default budget
- **THEN** each of the crate's targets runs for its share of the budget, and the script reports executions, coverage
  and any failing input per target

### Requirement: Kill tests on the binary
The server binary SHALL be killed during rounds, syncs and checkpoints and restarted; after each restart every
acknowledged commit SHALL be present, and every round SHALL be present whole or absent whole. A commit killed before its
reply MAY be present, since its round may have been synced; its client resolves it through its session.

#### Scenario: A kill during a sync
- **WHEN** the server is killed while a round's sync is in progress and restarted
- **THEN** the round's transactions are all present or all absent, none of them was acknowledged, and each client's
  retry through its session learns which
