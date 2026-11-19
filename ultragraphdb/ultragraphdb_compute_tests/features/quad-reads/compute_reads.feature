# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Quad reads through compute
  openspec/changes/stage-5-5-three-role-refactor/specs/quad-reads/spec.md, through the compute process's client endpoint, over real storage processes and a played system group.

  Rule: Compute executes quad-pattern reads over storage

    Scenario: A term absent where its list lies answers no row
      When a read-only transaction's read of `<a> <p> ?o` runs, `<a>` homed in partition 1, `<p>` homed in partition 2, and partition 1's dictionary holds no entry of `<p>`
      Then the dictionary Scan of partition 1 answers no entry, compute sends no list Scan and nothing to partition 2, and the read answers one batch flagged `LAST` with no row

    Scenario: A term and its list take two Scans
      When a read of `<a> <knows> ?o` runs against a partition holding both terms
      Then compute's first Scan holds the terms' dictionary ranges, and its Scan of the list, keyed by their IDs, comes after that Scan's answer

    Scenario: A read reads one version of a partition
      When a read-only transaction at latest level takes three Scans of its partition, and a commit lands between the first and the second
      Then the second and third Scans set `at` 1 and `read_version` the first Entries' `version`, and the read returns none of the commit

    Scenario: A list Scan waits for every partition's close
      When a snapshot read asks partitions 1 and 2, partition 1 has closed the read's epoch and partition 2 has not
      Then compute sends each a dictionary Scan at that epoch and no list Scan until both have answered, so no list Scan of partition 1 is refused as stalled while partition 2 waits
