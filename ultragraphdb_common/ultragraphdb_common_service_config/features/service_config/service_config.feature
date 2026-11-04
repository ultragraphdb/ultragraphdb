# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: One service configuration
  One configuration shared by every binary, mapping the settings file one to one
  (openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md).

  Scenario: A file's settings reach their fields
    When a file holds `[storage] partition_count = 4` and `[cluster] role_secret_file = "/run/secret"`
    Then the configuration's storage table holds 4 partitions and its cluster table that file

  Scenario: No file leaves the environment alone
    When no file is named and `ULTRAGRAPHDB_STORAGE_PARTITION_COUNT=2` is set
    Then the storage table holds 2 partitions and every other setting is absent

  Scenario: A named file that cannot be read stops the start
    When a file is named that does not exist
    Then the configuration is refused with an error naming the file

  Scenario: An empty unknown table is refused
    When a file holds the header `[storge]` and no key
    Then the configuration is refused with an error naming `storge`

  Scenario: A misspelled variable is refused
    When `ULTRAGRAPHDB_STORAGE_PARTITON_COUNT=2` is set
    Then the configuration is refused with an error naming the variable

  Scenario: A variable of the wrong kind is refused
    When `ULTRAGRAPHDB_STORAGE_PARTITION_COUNT=many` is set
    Then the configuration is refused with an error naming the variable and the integer it expected
