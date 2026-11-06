# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A commit in flight through a network that loses frames
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request. A link between the client and the frontend takes the frame the scenario names.

  @both_shells
  Scenario: An unsent commit is lost, a sent one in doubt
    When the connection closes once with a commit queued and unsent, and once with a commit sent and unanswered
    Then the first answers `ConnectionLost`, the second `InDoubt`, and resolving the second reports whether it committed

  Scenario: A commit through no session has an unknown outcome
    When a transaction committing through no session loses its reply
    Then the client reports the outcome as unknown, and resends nothing

  Scenario: A drop retried through its session applies once
    When a Drop's reply is lost, and the client sends it again with the same session and request number
    Then the second answer is the stored reply, and the drop is applied once
