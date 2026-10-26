#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Posting lists
  A posting list is read by merging its deltas, newest first, over its newest complete version at or below the read
  timestamp; it is rolled up into a complete version and split into parts when it grows large. The scenario
  "Deleting a non-matching scalar value changes nothing" is index maintenance's rule (Dgraph's index.go) and runs in
  ultragraphdb_store_apply's features.

  Scenario: A read does not see a later delta
    When a list holds a delta committed at 6 and is read at 5
    Then the result excludes the delta's postings

  Scenario: A read merges deltas over the complete list
    When a list holds a complete version at 4 and deltas at 5 and 6 and is read at 6
    Then the result is the complete list with both deltas applied in commit order

  Scenario: A read below a rollup reads the older versions
    When a list with deltas at 3 and 5 was rolled up into a complete version at 6 and is read at 4
    Then the result is the list as of the delta at 3

  Scenario: A read below an empty rollup reads the older versions
    When a list holds a delta at 4, a delete-all at 5 and an empty rollup at 6, and is read at 4
    Then the result holds the delta's postings

  Scenario: Delete-all hides earlier postings from later reads
    When a list holds postings, a delete-all is committed at 8, and the list is read at 8 and at 7
    Then the read at 8 is empty and the read at 7 returns the postings

  Scenario: A rollup preserves every read above it
    When a list with a complete version and several deltas is rolled up
    Then reads at every timestamp at or above the rollup's version return the same results as before it

  Scenario: A rollup at the maximum timestamp takes the last commit's version
    When a list holds deltas committed at 2, 4 and 6 and a delete-all committed at 10, and is rolled up at the maximum timestamp
    Then the complete version is written at 10, the delete-all's commit timestamp

  Scenario: A rollup at a finite timestamp writes one above the last commit
    When the same list is rolled up at timestamp 12
    Then the complete version is written at 11

  Scenario: Facets survive a rollup
    When a reference posting carrying a facet is rolled up
    Then the posting and its facet are read back after the rollup

  Scenario: A crash during a split rollup leaves one list whole
    When the process crashes while the rollup of a split list is written, and the store is reopened
    Then the list reads as it did before the rollup, with its old parts, or as after it, with its new parts

  Scenario: A large list is split into parts
    When a list grows past 512 KiB over several blocks and is rolled up
    Then its complete version records its parts and each part is stored under a split key

  Scenario: Iteration after a uid crosses part boundaries
    When a split list is iterated from a uid in its first part
    Then the result is every uid above it, across all parts, in order

  Scenario: Deleting every uid of a split list empties it
    When every uid of a split list is deleted and the list is rolled up
    Then the list is stored as empty and no split key of the list remains

  Scenario: A split part cannot be read as a list
    When a split key is read directly as a posting list
    Then the read fails with `cannot read posting list using multi-part list key`
