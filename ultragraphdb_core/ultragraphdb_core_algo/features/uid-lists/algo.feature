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

  Scenario: Intersecting from past a pack's last UID returns nothing
    Given the UIDs 1 to 100 packed and the plain list of the UIDs 1 to 11
    When the packed list is intersected with the plain one after UID 200
    Then the intersection is empty

  Scenario: Three packed lists intersect to their common UIDs
    Given the UID lists "1, 2, 3, 9", "2, 3, 9, 10, 11" and "9, 20, 21, 22, 23, 24"
    When they are packed and intersected
    Then the result is "9"

  Scenario: A packed difference reads every block of the first list
    When the UIDs 2 to 15 packed in blocks of 5 have the UIDs "1" subtracted
    Then the result is the UIDs 2 to 15

  Scenario: A UID in a later block of the second list is subtracted
    When the UIDs "642" have the UIDs "1, 2, 3, 4, 5, 642" packed in blocks of 5 subtracted
    Then the result is empty

  Scenario: Subtracting an empty pack keeps the first list
    When the UIDs "1, 2, 3" have an empty pack of block size 5 subtracted
    Then the result is "1, 2, 3"
