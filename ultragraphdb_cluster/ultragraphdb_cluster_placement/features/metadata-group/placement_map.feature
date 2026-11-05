# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The placement map
  The placement map's own checks (openspec/changes/stage-5-5-three-role-refactor/specs/metadata-group/spec.md): the
  scenarios of the requirement "The placement map assigns bucket ranges to partitions", verbatim. The metadata group
  holding the map and recording its changes is group 6's.

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

    Scenario: A bucket group resolves at an epoch
      When a change moving buckets 21,888..21,952 from partition 2 to partition 3 applies from epoch 21, and the bucket group holding those 64 buckets is resolved at epochs 20 and 21
      Then it resolves to partition 2 at epoch 20 and to partition 3 at epoch 21
