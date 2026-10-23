#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Sorted-set operations over UID lists
  Intersection, merge, difference and index-of run on sorted UID lists and on packed lists.

  Scenario: Intersection of packed and plain lists agrees
    Given sorted, duplicate-free UID lists of 0, 1, 3, 11, 100 and 1000 UIDs, each pair sharing some UIDs
    When each pair is intersected as plain lists, as packed lists and as a packed list with a plain one
    Then the three results are equal, sorted and free of duplicates

  Scenario: Merging several lists removes duplicates
    Given the UID lists "1, 3, 5", "3, 4" and "5, 6"
    When they are merged as plain lists and as packed lists
    Then both results are "1, 3, 4, 5, 6"
