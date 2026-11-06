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

  Scenario: A port held a moment is taken
    When a role starts at a TCP address another process holds, which lets it go within the bound
    Then the role logs each retry naming the address, binds it, and answers `/health`

  Scenario: The frontend's /health answers once its listeners are bound
    When the frontend starts while another process holds its role address, which lets it go within the bound
    Then `/health` answers 503 while the address is held, and once it answers 200 a role connection to the address is accepted at once

  Scenario: A port held past the bound ends the start
    When a role starts at a TCP address another process holds past the bound
    Then it exits with code 69 after a retry logged per 50 ms of the bound, the error naming the address

  Scenario: Any other bind error ends the start at once
    When a role starts at a TCP address no interface of its host holds
    Then it exits with code 69 at once, the error naming the address, with no retry logged
