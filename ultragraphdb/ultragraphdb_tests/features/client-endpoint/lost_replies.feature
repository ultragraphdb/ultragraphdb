# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The frontend answers a resent empty Commit
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request. It follows the empty Commit of "An empty transaction commits at the frontend", its reply lost by a link between the client and the frontend.

  Scenario: A resent empty Commit answers not committed
    When that Commit's reply is lost, and the client sends it again
    Then every partition answers 18, and the client receives code 18
