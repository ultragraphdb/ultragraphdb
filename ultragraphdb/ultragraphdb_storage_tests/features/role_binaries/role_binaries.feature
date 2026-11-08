# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The storage binary as a process
  openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md

  Scenario: A role with the wrong secret is refused
    When a storage process started with a different role secret connects to the frontend
    Then its authentication is refused, it sends no role command, and it retries until its deadline

  Scenario: A missing role secret stops the start
    When a role starts with `role_secret_file` naming a file that does not exist
    Then it exits with code 78 and an error naming the file

  Scenario: Two storage processes share the partitions
    When one storage process claims partitions 1 and 2 and another claims 3 and 4
    Then both join, Members names each partition's address, and a write reaches its partition on either process

  Scenario: A registration is a role command
    When a partition of one storage process registers a reference to an entity homed in another storage process
    Then it sends a registration role command to that process, and receives the entity's ID, the member's generation and the home's latest delivered close in the reply

  Scenario: A reference crosses storage processes
    When a write in one storage process names an entity homed in the other
    Then the home registers it by a Reference, and the write commits

  Scenario: Storage flushes on shutdown
    When storage receives SIGTERM after acknowledging commits
    Then every acknowledged commit is applied to its store before it exits, and a restart replays nothing

  Scenario: Storage is not ready before recovery
    When a storage process killed with rounds in its log restarts
    Then `/health` answers at once and `/ready` answers only after the replay

  Scenario: Storage stopped before it joined is never ready
    When storage receives SIGTERM while the frontend holds its Join's reply
    Then it completes its Join, exits with code 0, and never reports itself ready

  Scenario: A partition leases its blocks ahead of need
    When a storage partition joins the frontend
    Then it leased the current block and the block ahead of each type before it joined

  Scenario: An unknown setting stops the start
    When the storage binary receives settings whose `[storage]` table holds the key `partiton_count`
    Then it exits with code 78 and an error naming `partiton_count`

  Scenario: A configuration error exits with its code
    When the storage binary receives a setting out of its bounds
    Then it exits with code 78 and an error naming the setting and the bound

  Scenario: A pinned role reports what it holds
    When a role starts with `pin_cores = true` on a platform that allows binding
    Then it logs each runtime's CPU, read back from the operating system, each within the permitted set

  Scenario: A missing required setting stops the start
    When the storage binary receives settings with no `storage.data_dir`
    Then it exits with code 78 and an error naming `storage.data_dir`

  Scenario: A platform refusing to pin still serves
    When a role starts with `pin_cores = true` where binding is unavailable
    Then it logs that it runs unpinned, and serves requests

  Scenario: A subscription past the closes held exits with its code
    When the frontend ends a storage process's subscription, the close it asks for being no longer held
    Then the process shuts down through its ordinary path, and it exits with code 69 naming what the frontend said

  Scenario: An invariant violation exits with its code
    When a storage partition detects an invariant violation in a release build
    Then the partition stops, the process shuts down through its ordinary path, and it exits with code 70
