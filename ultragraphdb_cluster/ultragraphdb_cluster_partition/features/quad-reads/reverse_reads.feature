Feature: Quad reads: reverse reads through referrer sets

  The quad-reads scenarios of reverse reads, verbatim from the change's specs/quad-reads/spec.md.

  Rule: A reverse read asks the object's referrers

    Scenario: A reverse read gathers every referrer
      When `<a>` in partition 1 and `<c>` in partition 3 each hold `knows` to `<b>`, homed in partition 2, and a read binds its object to `<b>` and its predicate to `knows` at a closed epoch
      Then compute reads `R(b)` at partition 2, asks partitions 1 and 3, and returns `<a>` and `<c>`

    Scenario: A partition the referrer set does not name is not asked
      When `<b>` is homed in partition 2, `R(b)` names groups of partitions 1 and 3 only, over four partitions, and a reverse read of `<b>` runs
      Then compute reads `R(b)` at partition 2, asks partitions 1 and 3 for their reverse entries, and sends nothing to partition 4

    Scenario: A lagging referrer delays the read, never truncates it
      When `R(b)` names groups of partitions 1 and 3, partition 1 holds a matching row, partition 3 has not closed the read's epoch, and the transaction's deadline of 50 ms passes
      Then the read fails with a timeout naming partition 3, and returns no row of partition 1 without partition 3's
