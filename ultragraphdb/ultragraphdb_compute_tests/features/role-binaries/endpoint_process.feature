# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The compute binary's client endpoint as a process
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md, for compute's listener and its required settings
  openspec/changes/stage-5-8-system-group/specs/role-binaries/spec.md, and the two roles of
  openspec/changes/stage-5-9-compute-role/specs/role-binaries/spec.md

  Scenario: The role secret opens no client session
    When a client authenticates on the client endpoint with the role secret
    Then authentication is refused with the same error as a wrong password

  Scenario: The compute binary carries no storage engine and no book
    When the compute binary's dependency graph is listed
    Then it names neither `ultragraphdb_store_kv`, fjall, `ultragraphdb_common_system`, `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_sequencer` nor `ultragraphdb_transactions_log`

  Scenario: Compute is ready only once the books answer
    When compute starts while the hosting storage process still recovers
    Then compute's `/health` answers, and its `/ready` answers only after storage's

  Scenario: Compute without the system group's address stops the start
    When the compute binary receives settings with no `cluster.system_address`
    Then it exits with code 78 and an error naming `cluster.system_address`

  Scenario: A port held a moment is taken
    When a role starts at a TCP address another process holds, which lets it go within the bound
    Then the role logs each retry naming the address, binds it, and answers `/health`

  Scenario: Compute's /health answers once its listener is bound
    When compute starts while another process holds its client address, which lets it go within the bound
    Then `/health` answers 503 while the address is held, and once it answers 200 a client connection to the address is accepted at once

  Scenario: A port held past the bound ends the start
    When a role starts at a TCP address another process holds past the bound
    Then it exits with code 69 after a retry logged per 50 ms of the bound, the error naming the address

  Scenario: Any other bind error ends the start at once
    When a role starts at a TCP address no interface of its host holds
    Then it exits with code 69 at once, the error naming the address, with no retry logged
