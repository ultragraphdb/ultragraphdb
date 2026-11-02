# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Settings TOML
  The in-house parser of the TOML subset the roles' settings use, refusing every other feature by name, with typed
  accessors that refuse unread keys (openspec/changes/stage-5-5-three-role-refactor/specs/settings-toml/spec.md).

  Scenario: A settings file parses
    When the document `name = "dev"`, `[storage]`, `partition_count = 4  # one per core`, `sync = true` is parsed
    Then the root table holds `name` = `"dev"`, and the table `storage` holds `partition_count` = 4 and `sync` = true

  Scenario: Escapes and integers follow TOML
    When the values `"a\tbé"`, `-1_000` and `+7` are parsed
    Then they are the string `a`, tab, `b`, `é`, and the integers −1000 and 7

  Scenario: The subset agrees with TOML's own test suite
    When every document of the toml-test suite's valid cases that uses only the subset is parsed
    Then each parses to the values the suite's expected JSON states, the documents vendored under `testdata/` with their source and version

  Scenario: An array is refused as unsupported
    When a document holds `peers = ["a", "b"]` on line 3
    Then parsing fails with an unsupported-feature error naming arrays and line 3

  Scenario: A float is refused as unsupported
    When a document holds `ratio = 0.5`
    Then parsing fails with an unsupported-feature error naming floats and the line

  Scenario: A duplicate key is refused as invalid
    When a table holds `sync = true` twice
    Then parsing fails with an invalid-document error naming `sync` and the second line

  Scenario: TOML's invalid documents are refused
    When every document of the toml-test suite's invalid cases is parsed
    Then each is refused, as unsupported or as invalid, and none parses

  Scenario: A misspelled key is refused
    When a role reads `[storage] partition_count` from a document holding `[storage] partiton_count = 4`
    Then the read of `partition_count` reports it missing, and the final check refuses `storage.partiton_count`

  Scenario: A value of the wrong kind is refused
    When a role reads `[storage] partition_count` as an integer from `partition_count = "4"`
    Then the read fails with an error naming `storage.partition_count`, an integer expected and a string found

  Scenario: An oversized document is refused
    When a document of 64 KiB plus one byte is parsed
    Then parsing fails with an error naming the 64 KiB limit

  Scenario: Arbitrary bytes never panic
    When the fuzzer feeds the parser arbitrary bytes
    Then every input is accepted or refused with a typed error, and nothing panics
