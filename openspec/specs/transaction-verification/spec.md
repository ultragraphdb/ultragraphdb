# transaction-verification Specification

## Purpose
Verification of the transaction protocol: the Lean model of the round protocol and the fuzzing of every decoder.
## Requirements
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

### Requirement: A Lean model of placement
A Lean 4 model SHALL model partitions over bucket groups with a placement map that changes by epoch, the sequencer's
closes delivered to each partition in order, commits stamped exactly one above the latest delivered close, catalog
versions announced with a close, registrations with their replies, marks and generations, referrer sets under MVCC,
commits writing forward and local reverse entries, reverse reads at closed epochs, tombstones, and reclamation, under
any interleaving, delay or reordering of messages and crashes after durable steps. It SHALL prove P24's rules 4, 5, 6, 9
and 11, for every number of partitions, entities, registrations and steps: that every write is stamped at or above the
epoch of every registration it references, on every write path (the epoch inequality); that a reverse read at a closed
epoch, reading the entity's referrer set at that epoch and the reverse entries of the partitions it names, returns every
edge to the entity committed at or below that epoch; that a partition never stamps a catalog version's effective epoch
before it holds the version, nor routes at a placement version's effective epoch before it holds that version, placement
versions being assigned one per close at the epoch after it, so the placement in effect at an announced epoch never
changes; and that reclamation removes a referrer-set member only on a report naming its unchanged generation, so it
never removes a member whose bucket group holds an edge to the entity at an epoch the history retains. Reclamation is
the rule stage 8 builds: a partition reports "no edges to the entity as of epoch `e`" with its generation in the commit
that clears its mark, the home removes the member as an MVCC write only when the generation is unchanged, physical
deletion waits for the history-retention horizon, and the home's IRI-to-ID entry is reclaimed once the referrer set is
empty below the horizon and the entity has no committed data. The model SHALL live in `lean/` as the library
`PlacementModel`, beside `RoundModel`, built with `lake` outside Bazel, with no `sorry`; "The placement model builds
without gaps" SHALL be checked by `lake build PlacementModel` succeeding with no `sorry` in the library, and every other
scenario below SHALL be a theorem of the model named after it.

#### Scenario: The placement model builds without gaps
- **WHEN** `lake build PlacementModel` runs in `lean/`
- **THEN** it succeeds, and no proof in it uses `sorry`

#### Scenario: A stale reclamation report removes nothing
- **WHEN** a partition reports no edges to an entity with generation 3, re-registers before the report arrives, and the
  home raises the member to generation 4
- **THEN** the model's home keeps the member when the report arrives, and the reverse read completeness theorem holds
  for the partition's new edge

