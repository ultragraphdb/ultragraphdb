# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@both_carriers
Feature: A predicate's first write through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/commit-rounds/spec.md, through the real frontend and storage: the partition asks, and Describe answers from the metadata group.

  Scenario: Concurrent first writes of a predicate both commit
    When two transactions of `t1` read before either commits, the first writes `<a> <score> 12` and the second `<b> <score> "high"`, and `score` was not declared
    Then both commit, and the metadata group holds `score`'s default entry once
