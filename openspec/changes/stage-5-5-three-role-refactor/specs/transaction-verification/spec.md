## ADDED Requirements

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
before it holds the version; and that reclamation removes a referrer-set member only on a report naming its unchanged
generation, so it never removes a member whose bucket group holds an edge to the entity at an epoch the history retains.
Reclamation is the rule stage 8 builds: a partition reports "no edges to the entity as of epoch `e`" with its generation
in the commit that clears its mark, the home removes the member as an MVCC write only when the generation is unchanged,
physical deletion waits for the history-retention horizon, and the home's IRI-to-ID entry is reclaimed once the referrer
set is empty below the horizon and the entity has no committed data. The model SHALL live in `lean/` as the library
`PlacementModel`, beside `RoundModel`, built with `lake` outside Bazel, with no `sorry`; "The placement model builds without gaps" SHALL be checked by `lake build PlacementModel` succeeding
with no `sorry` in the library, and every other scenario below SHALL be a theorem of the model named after it.

#### Scenario: The placement model builds without gaps
- **WHEN** `lake build PlacementModel` runs in `lean/`
- **THEN** it succeeds, and no proof in it uses `sorry`

#### Scenario: A stale reclamation report removes nothing
- **WHEN** a partition reports no edges to an entity with generation 3, re-registers before the report arrives, and the
  home raises the member to generation 4
- **THEN** the model's home keeps the member when the report arrives, and the reverse read completeness theorem holds
  for the partition's new edge
