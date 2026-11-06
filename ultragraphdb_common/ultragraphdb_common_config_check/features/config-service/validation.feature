# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Validating settings before anything serves them
  openspec/changes/stage-5-5-three-role-refactor/specs/config-service/spec.md

  Scenario: An instance's file overrides its role's
    When target `ci` holds `storage` with `partition_count = 2` and `storage_2` with `partition_first = 3`
    Then `storage_2` receives both settings, and `storage_1`, whose file names `partition_first = 1`, receives its own

  Scenario: A broken file refuses the revision
    When a revision's `storage_1` lacks `data_dir` and whose `compute_1` names `partiton`
    Then every defect is listed, each naming its instance and setting

  Scenario: A partition held twice is refused
    When `storage_1` and `storage_2` both hold partition 2
    Then the revision is refused naming partition 2 and both instances

  Scenario: A partition no one holds is refused
    When `partition_count` is 4 and the storage instances hold partitions 1 to 3
    Then the revision is refused naming partition 4

  Scenario: Two listeners on one address are refused
    When `storage_1`'s second partition derives the address `compute_1` listens at
    Then the revision is refused naming the address and both instances

  Scenario: A changed cluster identity is refused
    When a revision names a `cluster.id` other than the active revision's
    Then it is refused naming both values

  Scenario: No central file stops the start
    When the central file does not exist
    Then the error names the setting and the path it looked at

  Scenario: A central file declaring nothing stops the start
    When the central file holds no target, or a target with no file
    Then the error names the central file and says what it must declare, with the target when one is empty

  Scenario: A declared file that is absent stops the start
    When target `ci` declares `storage_1 = "ci_storage_1.toml"` and no such file exists
    Then the error names the target, the instance, the value as written and the path it resolved to

  Scenario: Every absent file is named at once
    When two declared files are absent and a third is malformed
    Then the check lists all three

  Scenario: The check finds what the service would refuse
    When an administrator checks settings that fail
    Then the report lists the defects, and does not pass

  Scenario: The check passes what the service would take
    When an administrator checks valid settings
    Then the report lists each target with its instances, and passes

  Scenario: A stray file is pointed out
    When the directory holds `ci_storge_1.toml`, which no target declares
    Then the check warns naming the file, and still passes

  Scenario: A value out of bounds says what it requires
    When `ci_storage_1.toml` sets `storage.partition_count = 0`
    Then the entry names the file, its line, the target, the instance, the setting, the value and the bound

  Scenario: A Scan's stall of no time is refused
    When `ci_storage_1.toml` sets `storage.scan_stall_ms = 0`
    Then the entry names `storage.scan_stall_ms`, the value 0 and the bound it must meet

  Scenario: A stall past the client's read timeout is refused
    When `ci_storage_1.toml` sets `storage.scan_stall_ms = 30001`
    Then the entry names `storage.scan_stall_ms`, the value 30001 and the bound it must meet

  Scenario: A hung compute's stall of no time is refused
    When `ci_frontend.toml` sets `storage.scan_stall_ms = 0`
    Then the entry names `storage.scan_stall_ms`, the value 0 and the bound it must meet

  Scenario: A retention of no time is refused
    When `ci_storage_1.toml` sets `storage.read_retain_ms = 0`
    Then the entry names `storage.read_retain_ms`, the value 0 and the bound it must meet

  Scenario: A held-bytes bound of nothing is refused
    When `ci_frontend.toml` sets `endpoint.held_bytes = 0`
    Then the entry names `endpoint.held_bytes`, the value 0 and the bound it must meet

  Scenario: A parked-bytes bound of nothing is refused
    When `ci_storage_1.toml` sets `storage.held_bytes = 0`
    Then the entry names `storage.held_bytes`, the value 0 and the bound it must meet

  Scenario: A misspelled setting is named where it stands
    When `ci_storage.toml` holds `partiton_count = 4` under `[storage]`
    Then the entry names the file, the line, `storage.partiton_count`, and says no such setting exists
