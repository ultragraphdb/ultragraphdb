# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@system @partitions
Feature: A storage process hosts partitions over bucket ranges
  openspec/changes/stage-5-8-system-group/specs/partitions/spec.md: the storage binary as processes, one of them hosting the
  system group, settings served by the config double.

  Scenario: The system group forms the placement
    When the system group's host with `partition_count` 4 starts on a metadata group holding no placement
    Then the metadata group holds an even map over 4 partitions, and a restart forms none

  Scenario: Two storage processes share the partitions
    When one storage process claims partitions 1 and 2 and another claims 3 and 4
    Then both join, Members names each partition's address, and a write reaches its partition on either process

  Scenario: A claim on a held partition is refused
    When one storage process holds partition 2 and another claims it
    Then the claim is refused with an error naming partition 2 and its holder, and the placement is unchanged

  Scenario: A restart claims its partitions again
    When a storage process holding partitions 1 and 2 restarts at the same addresses
    Then its claim succeeds
