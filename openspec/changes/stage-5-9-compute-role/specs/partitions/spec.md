## MODIFIED Requirements

### Requirement: A read-write transaction stays within one partition in this stage
A read-write transaction SHALL read and write only what its partition owns. It SHALL bind to the partition owning the
first subject it reads or writes; when its first operation claims a value by an insert-if-absent or an upsert-and-link
and names no subject but new blank nodes, to the partition owning the claimed value's bucket; when its first write names
only new blank nodes, to a partition compute chooses in turn (term-dictionary, "IRIs and blank nodes get system IDs");
and when its first operation names no subject otherwise, to the partition owning the tenant's whole bucket run if one
does, that first operation aborting it with `CrossPartition` when none does. It SHALL register its references to
entities other partitions own as idempotent pre-steps outside it. A reverse read, or a read that fans out (a value-index
lookup, a scan, a pattern with an unbound subject, a bound literal object), SHALL be allowed only when the transaction's
partition owns the tenant's whole run, as it always does with one partition; it then reads the partition's own entries,
the referrer set included, at its latest state. Cross-partition work SHALL be aborted with the reason `CrossPartition`,
naming the transaction's partition and the partitions the operation would reach, and writing nothing: a forward read of
a subject another partition owns; a reverse or fan-out read when the partition does not own the tenant's whole run;
writing a statement whose subject another partition owns; a delete by pattern reaching another partition's state ("A
delete by pattern reaching another partition is refused in this stage"); a write of a unique predicate's value whose
bucket (system-identity, "The bucket comes from the tenant and the key") another partition owns; and any Drop reaching
several partitions, of a predicate, a graph, a tenant's data or everything in a tenant. A read-only transaction reads
across partitions at its level (quad-reads). Stage 8 executes cross-partition work through the sequencer.

#### Scenario: A transaction reaching a second partition aborts
- **WHEN** a transaction writes a statement whose subject partition 1 owns and then writes one whose subject partition
  2 owns
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and holds nothing

#### Scenario: Write skew across partitions is refused, not committed
- **WHEN** `alice` is homed in partition 1 and `bob` in partition 2, both `on-call` `true`, and a read-write transaction
  reads `on-call` of `alice` and then of `bob`
- **THEN** the second read aborts the transaction with `CrossPartition` naming partitions 1 and 2, and it writes
  nothing

#### Scenario: A reverse read over a spread tenant aborts a read-write transaction
- **WHEN** a tenant's run spans partitions 1 and 2, `<b>` is homed in partition 2, and a read-write transaction bound
  to partition 1 reads the subjects of `knows` pointing at `<b>`
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2

#### Scenario: One partition reads its own reverse entries
- **WHEN** one partition owns the tenant's whole run, and a read-write transaction reads the subjects of `knows`
  pointing at `<b>` and then writes `<x> <seen> true`
- **THEN** the read answers from the partition's own reverse entries at its latest state, and the transaction commits

#### Scenario: A first read naming no subject over a spread tenant aborts
- **WHEN** a tenant's run spans partitions 1 and 2, and a read-write transaction's first operation looks up the
  `term` index of `name`
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and holds nothing

#### Scenario: A unique value owned by another partition is refused
- **WHEN** `epi.id` is declared unique, the bucket of `"x"` under `epi.id` belongs to partition 2, and a transaction
  bound to partition 1 insert-if-absents a node with `epi.id` `"x"`
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and no node holds `epi.id` `"x"`

#### Scenario: A plain write of a unique value owned by another partition is refused
- **WHEN** `epi.id` is declared unique, the bucket of `"x"` under `epi.id` belongs to partition 2, and a transaction
  bound to partition 1 writes `<a> <epi.id> "x"`
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and `<a>` holds no `epi.id`

#### Scenario: A first insert-if-absent binds to its claim's partition
- **WHEN** `epi.id` is declared unique, partition 2 owns the bucket of `"x"`, and a read-write transaction's first
  operation is an insert-if-absent of a node with `epi.id` `"x"`
- **THEN** the transaction binds to partition 2 and commits there

#### Scenario: A reference to another partition's entity stays in one partition
- **WHEN** a transaction bound to partition 1 writes `<a> <knows> <b>`, `<b>` homed in partition 2
- **THEN** it registers the reference at partition 2, commits in partition 1 alone, and does not abort

#### Scenario: A Drop over several partitions is refused
- **WHEN** a tenant's buckets span partitions 1 and 2, and a client drops the tenant's data
- **THEN** the Drop is refused with `CrossPartition` naming partitions 1 and 2, and nothing is dropped
