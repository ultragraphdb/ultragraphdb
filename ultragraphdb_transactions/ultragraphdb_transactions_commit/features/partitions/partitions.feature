# The single-store scenarios of the partitions capability, verbatim from
# openspec/changes/stage-5-5-three-role-refactor/specs/partitions/spec.md: a read-write transaction bound to one
# partition of a placement map over one store, the map giving each bucket its owner, so the second partition is the
# map's and no store of its own. The scenarios needing several partitions' stores are `ultragraphdb_cluster_partition`'s.

Feature: A read-write transaction stays within one partition

  # Requirement: A read-write transaction stays within one partition in this stage

  Scenario: A transaction reaching a second partition aborts
    When a transaction writes a statement whose subject partition 1 owns and then writes one whose subject partition 2 owns
    Then the transaction aborts with `CrossPartition` naming partitions 1 and 2, and holds nothing

  Scenario: Write skew across partitions is refused, not committed
    When `alice` is homed in partition 1 and `bob` in partition 2, both `on-call` `true`, and a read-write transaction reads `on-call` of `alice` and then of `bob`
    Then the second read aborts the transaction with `CrossPartition` naming partitions 1 and 2, and it writes nothing

  Scenario: A reverse read over a spread namespace aborts a read-write transaction
    When a namespace's run spans partitions 1 and 2, `<b>` is homed in partition 2, and a read-write transaction bound to partition 1 reads the subjects of `knows` pointing at `<b>`
    Then the transaction aborts with `CrossPartition` naming partitions 1 and 2

  Scenario: One partition reads its own reverse entries
    When one partition owns the namespace's whole run, and a read-write transaction reads the subjects of `knows` pointing at `<b>` and then writes `<x> <seen> true`
    Then the read answers from the partition's own reverse entries at its latest state, and the transaction commits

  Scenario: A first read naming no subject over a spread namespace aborts
    When a namespace's run spans partitions 1 and 2, and a read-write transaction's first operation looks up the `term` index of `name`
    Then the transaction aborts with `CrossPartition` naming partitions 1 and 2, and holds nothing

  Scenario: A unique value owned by another partition is refused
    When `epi.id` is declared unique, the bucket of `"x"` under `epi.id` belongs to partition 2, and a transaction bound to partition 1 insert-if-absents a node with `epi.id` `"x"`
    Then the transaction aborts with `CrossPartition` naming partitions 1 and 2, and no node holds `epi.id` `"x"`

  Scenario: A plain write of a unique value owned by another partition is refused
    When `epi.id` is declared unique, the bucket of `"x"` under `epi.id` belongs to partition 2, and a transaction bound to partition 1 writes `<a> <epi.id> "x"`
    Then the transaction aborts with `CrossPartition` naming partitions 1 and 2, and `<a>` holds no `epi.id`

  # Requirement: A delete by pattern reaching another partition is refused in this stage

  Scenario: A delete by pattern of another partition's subject is refused
    When a transaction bound to partition 1 deletes the pattern `<b>`, `knows`, and `<b>` is homed in partition 2
    Then the transaction aborts with `CrossPartition` naming partitions 1 and 2, and every `knows` of `<b>` is still held

  Scenario: A delete by pattern leaves other partitions' edges
    When `<a>`, homed in partition 1, holds `<a> <knows> <b>`, and a transaction in partition 2, `<b>`'s home, deletes the pattern `<b>`, `name`
    Then it commits in partition 2 alone, `<b>` holds no `name`, and `<a> <knows> <b>` is still held
