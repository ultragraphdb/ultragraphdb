# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The frontend binary as a process
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md

  Scenario: The role secret opens no client session
    When a client authenticates on the client endpoint with the role secret
    Then authentication is refused with the same error as a wrong password

  Scenario: Epoch closes reach storage in order
    When the frontend's sequencer closes epochs 1 to 5 and storage is connected
    Then storage receives five epoch-close commands, in order

  Scenario: The frontend answers a partition's lease and its placement
    When a partition asks the frontend for a block and the placement
    Then the frontend answers both over the role connection

  Scenario: The frontend binary carries no storage engine
    When the frontend binary's dependency graph is listed
    Then it names neither `ultragraphdb_store_kv` nor fjall

  Scenario: A frontend without its role address stops the start
    When the frontend binary receives settings with no `frontend.role_address`
    Then it exits with code 78 and an error naming `frontend.role_address`
