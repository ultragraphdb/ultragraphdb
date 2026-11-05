# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The compute binary as a process
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md

  Scenario: A signal stops a role cleanly
    When a role is running and receives SIGTERM
    Then it exits with code 0

  Scenario: The compute binary carries no storage engine
    When the compute binary's dependency graph is listed
    Then it names neither `ultragraphdb_store_kv` nor fjall

  Scenario: A compute process without its address stops the start
    When the compute binary receives settings with no `compute.role_address`
    Then it exits with code 78 and an error naming `compute.role_address`

  Scenario: Compute is ready once a partition has joined
    When compute has joined a frontend no partition has joined
    Then its `/health` answers, its `/ready` does not, and it turns ready once a partition joins

  Scenario: Compute hosts no command yet
    When a role asks compute for a command it does not host
    Then the connection authenticates and the command is refused as not hosted

  Scenario: Compute refuses a role with another secret
    When a role asks compute with a different role secret
    Then its authentication is refused

  Scenario: A lost dependency exits with its code
    When compute loses its storage process, which does not return within compute's deadline
    Then compute shuts down through its ordinary path and exits with code 69

  Scenario: A peer that stops answering without closing is a lost dependency
    When compute's storage process stops answering with its connections open, and does not return within compute's deadline
    Then compute shuts down through its ordinary path and exits with code 69
