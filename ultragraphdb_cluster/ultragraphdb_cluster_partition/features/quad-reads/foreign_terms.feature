# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Quad reads: a term another partition owns

  The quad-reads scenarios of a foreign term, verbatim from the change's specs/quad-reads/spec.md, over two partitions: one
  partition alone cannot tell an ID its home gave from one it minted.

  Rule: Storage's read kernel answers what compute scans

    Scenario: A term another partition owns is read through its copy
      When a transaction on partition 1 reads a statement of a predicate partition 2 owns, partition 1 holding the copy
      Then the Scan's range answers the predicate's ID, and the read returns the statement

    Scenario: A term another partition owns and no copy holds comes from its home
      When a transaction on partition 1 reads a predicate partition 2 owns, partition 1 holding no copy
      Then partition 1 takes the ID from partition 2, the range answers it, and a later commit into the list the read scanned aborts the transaction
