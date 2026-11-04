# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The role binaries as processes
  The three binaries started as a deployment starts them
  (openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md). Scenarios reaching a role's logic join with its
  task.

  Scenario: The start order reaches readiness
    When the frontend starts, then storage, then compute
    Then every role's `/ready` answers, the frontend's last, and the frontend's members name storage and compute

  Scenario: The roles join over Unix sockets
    When the frontend starts, then storage, then compute, each at a Unix socket
    Then every role's `/ready` answers, the frontend's last, and the frontend's members name storage and compute
    And each role's socket has mode 0600

  Scenario: A role with the wrong secret is refused
    When a storage process started with a different role secret connects to the frontend
    Then its authentication is refused, it sends no role command, and it retries until its deadline

  Scenario: The role secret opens no client session
    When a client authenticates on the client endpoint with the role secret
    Then authentication is refused with the same error as a wrong password

  Scenario: A missing role secret stops the start
    When a role starts with `role_secret_file` naming a file that does not exist
    Then it exits with code 78 and an error naming the file

  Scenario: Epoch closes reach storage in order
    When the frontend's sequencer closes epochs 1 to 5 and storage is connected
    Then storage receives five epoch-close commands, in order

  Scenario: The frontend answers a partition's lease and its placement
    When a partition asks the frontend for a block and the placement
    Then the frontend answers both over the role connection

  Scenario: Two storage processes share the partitions
    When one storage process claims partitions 1 and 2 and another claims 3 and 4
    Then both join, Members names each partition's address, and a write reaches its partition on either process

  Scenario: A reference crosses storage processes
    When a write in one storage process names an entity homed in the other
    Then the home registers it by a Reference, and the write commits

  Scenario: Storage flushes on shutdown
    When storage receives SIGTERM after acknowledging commits
    Then every acknowledged commit is applied to its store before it exits, and a restart replays nothing

  Scenario: Storage is not ready before recovery
    When a storage process killed with rounds in its log restarts
    Then `/health` answers at once and `/ready` answers only after the replay

  Scenario: A signal stops a role cleanly
    When a role is running and receives SIGTERM
    Then it exits with code 0

  Scenario: An unknown setting stops the start
    When the storage binary receives settings whose `[storage]` table holds the key `partiton_count`
    Then it exits with code 78 and an error naming `partiton_count`

  Scenario: A configuration error exits with its code
    When the storage binary receives a setting out of its bounds
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

  Scenario: A missing required setting stops the start
    When the storage binary receives settings with no `storage.data_dir`
    Then it exits with code 78 and an error naming `storage.data_dir`

  Scenario: A compute process without its address stops the start
    When the compute binary receives settings with no `compute.role_address`
    Then it exits with code 78 and an error naming `compute.role_address`
