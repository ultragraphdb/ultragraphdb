# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The metadata group
  The metadata group's state machine over its ordered log (openspec/changes/stage-5-5-three-role-refactor/specs/metadata-group/spec.md):
  every scenario of every requirement, verbatim, run over the write-ahead log on its simulated storage. The sequencer
  closing the epochs is the real one (`ultragraphdb_cluster_sequencer`), whose closes the group learns. Bound on the
  group's side only: the partitions validating rounds against a version are models reading the group's
  `CatalogSource` (group 7 runs them against the partition's own code); the prepared statements staled by a version are
  the frontend's (groups 9 and 10), so here no catalog version is created; the client authenticating with the bootstrap
  credential runs the SCRAM exchange against the group's `Credentials`, not the frontend's endpoint (group 9); a commit
  through an evicted session is refused by the frontend's owner check, which reads the registry here.

  Rule: The metadata group is a log-backed state machine with one replica

    Scenario: Recovery rebuilds the state
      When the group applies a catalog version, two block leases, a placement change and a principal, and is killed and restarted
      Then its state after recovery equals its state before the kill, read at the same log position

    Scenario: A snapshot shortens recovery
      When a snapshot is taken at position 100 and 20 more commands are applied before a restart
      Then recovery loads the snapshot and replays exactly the 20 commands after it

  Rule: Catalog versions take effect at an epoch boundary

    Scenario: A version applies from its epoch
      When catalog version 3 is committed as pending while epoch 38 is the latest close, the sequencer closes epoch 39, and rounds of epochs 39 and 40 commit
      Then version 3 takes effect at epoch 40, assigned and announced with the close of 39, and the round of epoch 39 is validated against version 2 and the round of epoch 40 against version 3

    Scenario: A default entry is no catalog version
      When version 3 of `tenant` is current, a first write of `score` records its default entry, and a client offers version 4 built on version 3
      Then the default-entry record of (`tenant`, `score`) is held, version 4 is accepted, and no prepared statement of `tenant` is stale

    Scenario: A stale version is refused
      When a client offers version 4 built on version 2 while version 3 is current
      Then the offer is refused with an error naming versions 2 and 3, and version 3 stays current

  Rule: The metadata group leases sequence blocks

    Scenario: Blocks never overlap
      When partitions 1 and 2 lease blocks of the `Resource` type alternately ten times
      Then every block begins at the previous block's end, and no two overlap

    Scenario: A lease survives a crash
      When a block 1..1001 is leased and the group is killed and restarted
      Then the next lease of the same type begins at 1001

    Scenario: An oversized request is refused
      When a partition asks for a block larger than `block_size_max`
      Then the lease is refused with an error naming the size and the maximum

  Rule: The placement map assigns bucket ranges to partitions

    Scenario: The initial map covers every bucket once
      When the map is initialised for one tenant and three partitions
      Then that tenant's 65,536 buckets are assigned in three contiguous ranges whose sizes differ by at most one, and every other bucket is assigned to a partition

    Scenario: A change leaving a bucket unassigned is refused
      When a change moves the range 100..200 from partition 1 to nowhere
      Then the change is refused with an error naming bucket 100, and the map is unchanged

    Scenario: A bucket's partition is found
      When partition 2 holds the range 21,846..43,691 and bucket 30,000 is looked up
      Then the lookup answers partition 2

    Scenario: A placement change takes effect two epochs after the latest close
      When a change moving buckets 21,888..21,952 from partition 2 to partition 3 is committed while epoch 19 is the latest close, and the sequencer closes epoch 20
      Then the close of epoch 20 assigns it effective epoch 21, and the bucket group holding those 64 buckets resolves to partition 2 at epoch 20 and to partition 3 at epoch 21

    Scenario: A close announcing a placement version out of order is refused
      When placement versions 1 and 2 are pending and a close announces version 2
      Then the group refuses the close, and both versions stay pending

  Rule: Principals live in the metadata group

    Scenario: The bootstrap principal authenticates
      When a cluster starts for the first time with a bootstrap credential file and a client authenticates with it
      Then authentication succeeds, and the metadata group holds that principal without the password

    Scenario: A duplicate principal is refused
      When a principal named `ann` exists and another `ann` is created
      Then the creation is refused with an error naming `ann`

    Scenario: The last administrator stays
      When the only principal with the administrator role is removed
      Then the removal is refused with an error naming it

    Scenario: A removed principal's sessions are evicted
      When `dave` has two sessions and `carol` one, and `dave` is removed
      Then both of `dave`'s sessions are evicted in the removal and a commit through either is refused as evicted, and `carol`'s session stays

  Rule: The metadata group registers sessions

    Scenario: A registered session survives a restart
      When a session is registered for `alice` and the group is killed and restarted
      Then the session's principal is still `alice`

    Scenario: A full registry evicts the oldest session
      When the registry holds `sessions_max` sessions and one more is registered
      Then the least recently registered session is evicted, and a commit through it is refused as evicted
