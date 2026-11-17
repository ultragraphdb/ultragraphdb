# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The frontend binary as a process
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md, and for the frontend's listener and its
  required settings, openspec/changes/stage-5-8-system-group/specs/role-binaries/spec.md

  Scenario: The role secret opens no client session
    When a client authenticates on the client endpoint with the role secret
    Then authentication is refused with the same error as a wrong password

  Scenario: The frontend binary carries no storage engine
    When the frontend binary's dependency graph is listed
    Then it names neither `ultragraphdb_store_kv` nor fjall

  Scenario: A frontend without the system group's address stops the start
    When the frontend binary receives settings with no `cluster.system_address`
    Then it exits with code 78 and an error naming `cluster.system_address`

  Scenario: A port held a moment is taken
    When a role starts at a TCP address another process holds, which lets it go within the bound
    Then the role logs each retry naming the address, binds it, and answers `/health`

  Scenario: The frontend's /health answers once its listener is bound
    When the frontend starts while another process holds its client address, which lets it go within the bound
    Then `/health` answers 503 while the address is held, and once it answers 200 a client connection to the address is accepted at once

  Scenario: A port held past the bound ends the start
    When a role starts at a TCP address another process holds past the bound
    Then it exits with code 69 after a retry logged per 50 ms of the bound, the error naming the address

  Scenario: Any other bind error ends the start at once
    When a role starts at a TCP address no interface of its host holds
    Then it exits with code 69 at once, the error naming the address, with no retry logged
