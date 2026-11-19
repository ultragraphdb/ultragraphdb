# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The compute binary as a process
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md, and compute's parts of
  openspec/changes/stage-5-8-system-group/specs/role-binaries/spec.md: "A lost dependency exits with its code" is that
  change's scenario as it stands, and compute's readiness once the system group and a storage process answer is
  "Compute is ready once a partition has joined". What compute's client endpoint decides is endpoint_process.feature's.

  Scenario: A signal stops a role cleanly
    When a role is running and receives SIGTERM
    Then it exits with code 0

  Scenario: Compute is ready once a partition has joined
    When compute has started while no process listens at `cluster.system_address`
    Then its `/health` answers, its `/ready` does not, and it turns ready once a partition joins

  Scenario: A lost dependency exits with its code
    When compute loses its storage process, which does not return within compute's deadline
    Then compute shuts down through its ordinary path and exits with code 69

  Scenario: A peer that stops answering without closing is a lost dependency
    When compute's storage process stops answering with its connections open, and does not return within compute's deadline
    Then compute shuts down through its ordinary path and exits with code 69
