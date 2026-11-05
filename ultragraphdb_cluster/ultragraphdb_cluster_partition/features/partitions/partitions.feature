Feature: Partitions

  The partitions capability's scenarios that need several partitions, verbatim from the change's specs/partitions/spec.md.

  Rule: A storage process hosts partitions over bucket ranges

    Scenario: Two partitions keep their data apart
      When a storage process hosts partitions 1 and 2, and an entity of each partition's buckets is written
      Then each entity's keys are held only in its own partition's keyspaces, and each partition's log holds only its own rounds

    Scenario: The default is one partition
      When the storage binary starts with no partition count configured
      Then it hosts one partition owning every bucket

  Rule: Requests reach the partition of their bucket

    Scenario: A write reaches its bucket's partition
      When a write's subject falls in bucket 30,000, which partition 2 owns
      Then partition 2 applies it

    Scenario: A misrouted write is refused with its owner
      When a write for bucket 30,000 reaches partition 1
      Then partition 1 refuses it with an error naming bucket 30,000 and partition 2, and writes nothing

    Scenario: A later subject outside the partition is cross-partition
      When a DirectWrite's first subject lies in partition 1's ranges and a later one in partition 2's, and it reaches partition 1
      Then partition 1 refuses it with code 23, not code 26, and writes nothing

  Rule: A partition owns every entry its subjects' writes derive

    Scenario: An edge to another partition's entity stays with its subject
      When `<a>`, homed in partition 1, gains `<a> <knows> <b>`, and `<b>` is homed in partition 2
      Then partition 1 holds the forward entry of `knows` for `a`, its own reverse entry of `knows` for `b` naming `a`, and the object-predicates entry of `b`, all written in one commit, and partition 2's keyspaces gain no entry of the statement

    Scenario: A hub's inbound entries spread over its referrers
      When subjects homed in partitions 1, 2 and 3 each gain an edge to `<hub>`, homed in partition 1
      Then each partition's reverse index of `<hub>` names only its own subjects, and together they name every subject

  Rule: An entity's referrer set names where its inbound edges may be

    Scenario: A first registration adds a member
      When `R(o)` holds no member for bucket group 7 and the home accepts, at epoch 12, a registration for group 7 naming generation 0
      Then `R(o)` holds group 7 with epoch 12 and generation 1, and a read of `R(o)` at epoch 11 does not name it

    Scenario: A repeated registration changes nothing
      When `R(o)` holds group 7 at generation 3, and a registration for group 7 naming generation 2 arrives
      Then `R(o)` still holds group 7 at generation 3 with its epoch, and the reply names generation 3

    Scenario: A renewal raises the generation
      When `R(o)` holds group 7 at generation 3, and a registration for group 7 naming generation 3 arrives
      Then `R(o)` holds group 7 at generation 4 with its earlier epoch, and the reply names generation 4

    Scenario: The home adds its own group without a registration
      When a subject of bucket group 3, homed with `<hub>` in partition 1, gains an edge to `<hub>` in a commit at epoch 8
      Then `R(hub)` at epoch 8 names group 3, added by that commit, and no registration is sent

    Scenario: A group resolves through the map at the read's epoch
      When group 7's buckets belong to partition 2 up to epoch 20 and to partition 3 from epoch 21, and `R(o)` names group 7
      Then a reverse read of `o` at epoch 20 asks partition 2 for group 7 and one at epoch 21 asks partition 3, and `R(o)` is unchanged by the move

  Rule: A commit under a registration reply reads its mark

    Scenario: A renewal between the reply and the commit conflicts
      When transaction T in partition 1 receives a reply naming generation 4 for `(o, 7)`, and another transaction of partition 1 renews `(o, 7)` to generation 5 and commits before T
      Then T aborts with a conflict naming the mark key, writes nothing, and its retry marks `(o, 7)` at generation 5

  Rule: A registration's epoch is at most the epoch of every write referencing it

    Scenario: The registrant waits for the home's close
      When partition 1, whose latest delivered close is 9, registers a reference to an entity homed in partition 2, whose latest delivered close is 11, and the reply names close 11
      Then partition 2 adds `R(o)`'s member at epoch 12, and partition 1 commits the referencing write only after close 11 is delivered to it, at epoch 12 or later

    Scenario: A reverse read at a closed epoch misses no edge
      When partitions 1 and 2 each commit an edge to `o` at epochs 14 and 15, and a reverse read of `o` runs at epoch 15 once every partition has closed it
      Then `R(o)` at epoch 15 names both partitions' groups, and the read returns both subjects

  Rule: A read-write transaction stays within one partition in this stage

    Scenario: A first insert-if-absent binds to its claim's partition
      When `epi.id` is declared unique, partition 2 owns the bucket of `"x"`, and a read-write transaction's first operation is an insert-if-absent of a node with `epi.id` `"x"`
      Then the transaction binds to partition 2 and commits there

    Scenario: A reference to another partition's entity stays in one partition
      When a transaction bound to partition 1 writes `<a> <knows> <b>`, `<b>` homed in partition 2
      Then it registers the reference at partition 2, commits in partition 1 alone, and does not abort

    Scenario: A Drop over several partitions is refused
      When a tenant's buckets span partitions 1 and 2, and a client drops the tenant's data
      Then the Drop is refused with `CrossPartition` naming partitions 1 and 2, and nothing is dropped
