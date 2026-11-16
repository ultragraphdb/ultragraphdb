# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The frontend forms the placement and takes claims on it

  Scenario: The frontend forms the placement
    When a frontend with `partition_count` 4 starts on a metadata group holding no placement
    Then the metadata group holds an even map over 4 partitions, and a restart forms none

  Scenario: A claim on a held partition is refused
    When one storage process holds partition 2 and another claims it
    Then the claim is refused with an error naming partition 2 and its holder, and the placement is unchanged

  Scenario: A restart claims its partitions again
    When a storage process holding partitions 1 and 2 restarts at the same addresses
    Then its claim succeeds
