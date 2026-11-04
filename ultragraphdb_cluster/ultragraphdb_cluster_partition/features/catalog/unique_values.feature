Feature: Catalog: unique values across partitions

  The catalog scenario of a unique value claimed from two partitions, verbatim from the change's specs/catalog/spec.md.

  Rule: Unique values

    Scenario: A plain write and an insert-if-absent from two partitions cannot both claim
      When `id` is declared unique, partition 1 owns the bucket of `"x"`, a transaction bound to partition 2 writes `<b> <id> "x"`, and a transaction bound to partition 1 insert-if-absents a node with `id` `"x"`
      Then the write in partition 2 aborts with `CrossPartition` naming partitions 2 and 1, the insert-if-absent commits, and one node holds `id` `"x"`
