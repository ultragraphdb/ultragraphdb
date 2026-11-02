# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Sorted-set operations over ID lists
  Intersection, merge, difference and index-of over sorted lists of 128-bit system IDs and over packed lists
  (openspec/changes/stage-5-5-three-role-refactor/specs/uid-lists/spec.md): the scenarios of the requirements
  "Sorted-set operations over UID lists" and "Packed set operations equal the plain ones", and "Intersecting from past
  a pack's last ID returns nothing" of "Decoders seek without panicking or hanging", verbatim.

  Rule: Sorted-set operations over UID lists

    Scenario: Intersection of packed and plain lists agrees
      When two lists of system IDs are intersected as plain lists and as packed lists
      Then both results are equal, sorted and free of duplicates

    Scenario: Merging several lists removes duplicates
      When the lists of type 1, bucket 0 and sequences `[1,3,5]`, `[3,4]` and `[5,6]` are merged
      Then the result holds sequences `[1,3,4,5,6]` of type 1, bucket 0

  Rule: Decoders seek without panicking or hanging

    Scenario: Intersecting from past a pack's last ID returns nothing
      When the IDs of type 1, bucket 0 and sequences 1 to 100, packed in blocks of 10, are intersected with sequences 1 to 11 after sequence 200
      Then the result is empty and nothing panics

  Rule: Packed set operations equal the plain ones

    Scenario: Three packed lists intersect to their common IDs
      When the sequences `[1,2,3,9]`, `[2,3,9,10,11]` and `[9,20,21,22,23,24]` of type 1, bucket 0 are packed and intersected
      Then the result is the ID of sequence 9

    Scenario: A packed difference reads every block of the first list
      When sequences `2..15` packed in blocks of 5 have sequence `[1]` subtracted, all of type 1, bucket 0
      Then the result is sequences `2..15`

    Scenario: An ID in a later block of the second list is subtracted
      When sequence `[642]` has sequences `[1,2,3,4,5,642]` packed in blocks of 5 subtracted, all of type 1, bucket 0
      Then the result is empty

    Scenario: Subtracting an empty pack keeps the first list
      When sequences `[1,2,3]` have an empty pack of block size 5 subtracted
      Then the result is sequences `[1,2,3]`
