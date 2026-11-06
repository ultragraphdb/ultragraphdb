# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Index maintenance through compute
  openspec/changes/stage-5-5-three-role-refactor/specs/index-maintenance/spec.md: compute's lookup for the query core, over played partitions until a catalog version reaches storage.

  Rule: Count indexes

    Scenario: A reverse count sums the referrers' partitions
      When `friend` has an exact count index, two subjects in partition 1 and one in partition 2 hold `friend` to `<b>`, and compute's API looks up the objects of reverse count 3
      Then partition 1 lists `<b>` under reverse count 2 and partition 2 under 1, and the lookup returns `<b>`

    Scenario: A reverse count names only what it returns
      When `friend` lists many objects the dictionary does not name beside `<b>`, whose reverse count is 3, and compute's API looks up the objects of reverse count 3
      Then the lookup returns `<b>` alone, in a few Scans of each partition
