# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A Commit nothing bound is answered by the frontend
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: An empty transaction commits at the frontend
    When a client begins a read-write transaction and sends Commit through its session with nothing else
    Then the frontend answers `commit_version` 0, the token zero and no results, and no partition sees it

  Scenario: A resent empty Commit answers not committed
    When that Commit's reply is lost, and the client sends it again
    Then every partition answers 18, and the client receives code 18
