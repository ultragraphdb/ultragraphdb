# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: One service configuration
  One configuration shared by every binary, mapping the settings file one to one
  (openspec/changes/stage-5-5-three-role-refactor/specs/role-binaries/spec.md).

  Scenario: A file's settings reach their fields
    When a file holds `[storage] partition_count = 4` and `[cluster] role_secret_file = "/run/secret"`
    Then the configuration's storage table holds 4 partitions and its cluster table that file

  Scenario: An empty unknown table is refused
    When a file holds the header `[storge]` and no key
    Then the configuration is refused with an error naming `storge`
