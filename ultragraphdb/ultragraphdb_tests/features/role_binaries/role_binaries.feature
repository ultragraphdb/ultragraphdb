# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The role binaries as processes
  The three binaries started as a deployment starts them
  (openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md). Scenarios reaching a role's logic join with its
  task.

  Scenario: The roles start in the order their readiness needs
    When the frontend starts, then storage, then compute
    Then each answers `/health`, storage answers `/ready`, and the frontend and compute answer `/ready` with 503

  Scenario: A signal stops a role cleanly
    When a role is running and receives SIGTERM
    Then it exits with code 0

  Scenario: An unknown setting stops the start
    When the storage binary starts with a TOML file whose `[storage]` table holds the key `partiton_count`
    Then it exits with code 78 and an error naming `partiton_count`

  Scenario: A configuration error exits with its code
    When the storage binary starts with a setting out of its bounds
    Then it exits with code 78 and an error naming the setting and the bound

  Scenario: A pinned role reports what it holds
    When a role starts with `pin_cores = true`
    Then it logs what became of its binding, and serves requests

  Scenario: The frontend binary carries no storage engine
    When the frontend binary's dependency graph is listed
    Then it names neither `ultragraphdb_store_kv` nor fjall

  Scenario: The compute binary carries no storage engine
    When the compute binary's dependency graph is listed
    Then it names neither `ultragraphdb_store_kv` nor fjall
