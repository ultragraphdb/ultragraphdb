# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Reverse reads, levels and versions through compute
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the compute process's client endpoint, over real storage processes and a played system group.

  Rule: A reverse read asks the object's referrers

    Scenario: A partition the referrer set does not name is not asked
      When `<b>` is homed in partition 2, `R(b)` names groups of partitions 1 and 3 only, over four partitions, and a reverse read of `<b>` runs
      Then compute reads `R(b)` at partition 2, asks partitions 1 and 3 for their reverse entries, and sends nothing to partition 4

    Scenario: A lagging referrer delays the read, never truncates it
      When `R(b)` names groups of partitions 1 and 3, partition 1 holds a matching row, partition 3 has not closed the read's epoch, and the transaction's deadline of 50 ms passes
      Then the read fails with a timeout naming partition 3, and returns no row of partition 1 without partition 3's

  Rule: A read's transaction names its level

    Scenario: A monotonic-fast read does not wait for a lagging partition
      When partition 2 has closed epoch 9 and partition 1 epoch 12, and a monotonic-fast read with its subject unbound runs over both
      Then it answers at once, reading both partitions at epoch 9, the lower of their closed epochs

  Rule: Compute relays what a partition refuses, and refuses a frame it cannot trust

    Scenario: A later latest-state reverse read of another home is refused
      When a read at latest level bound to one partition reverses over an object another partition homes and nobody holds
      Then the read is refused with code 25

    Scenario: A reverse read at a named version finds an edge whose member stands above it
      When an edge's commit version in its partition lies below its member's version in the home's own order, and a reverse read names the edge's version
      Then the read returns the edge

    Scenario: A resumed statement read passes a property removed since
      When a read of a reifier at latest level with limit 2 is resumed, the last property it returned having been removed
      Then the resumed read returns the third property alone
