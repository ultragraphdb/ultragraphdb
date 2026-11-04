Feature: The benchmark measures N partitions

  The partitions capability's benchmark scenarios, verbatim from the change's specs/partitions/spec.md. A scenario's 10
  seconds runs as one, tests staying small and fast; what it checks does not depend on the run's length.

  Rule: The benchmark measures N partitions in one process

    Scenario: The benchmark reports every partition
      When `wal_bench` runs 4 partitions for 10 seconds
      Then its report lists 4 partitions' relations per second and their total, and two runs with one seed generate the same commits

    Scenario: The benchmark exercises registration
      When `wal_bench` runs 4 partitions with 10% of edges pointing at other partitions' entities
      Then its report lists registrations per second and the wait a first reference costs, and a reverse read, at the run's last closed epoch, of each entity that received an edge from another partition returns the subjects the same seed's run on one partition returns
