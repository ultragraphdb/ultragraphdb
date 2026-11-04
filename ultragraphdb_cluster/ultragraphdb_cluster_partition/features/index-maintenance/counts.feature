Feature: Index maintenance: reverse counts across partitions

  The index-maintenance scenario of a reverse count summed over partitions, verbatim from the change's specs/index-maintenance/spec.md.

  Rule: Count indexes

    Scenario: A reverse count sums the referrers' partitions
      When `friend` has an exact count index, two subjects in partition 1 and one in partition 2 hold `friend` to `<b>`, and a read looks up the objects of reverse count 3
      Then partition 1 lists `<b>` under reverse count 2 and partition 2 under 1, and the lookup returns `<b>`
