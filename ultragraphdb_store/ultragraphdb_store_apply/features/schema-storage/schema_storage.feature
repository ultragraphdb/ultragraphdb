#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Schema storage
  A write to a predicate the catalog version in effect does not declare is validated with the default entry and asks
  the metadata group to record it, a command no write waits for, which a refused write never sends. The scenario of
  recovery runs in ultragraphdb_transactions_commit's features.

  Scenario: A first write records the predicate
    When `<a> <nickname> "Al"` is written and `nickname` is not declared
    Then once the command is answered the metadata group holds the default-entry record of `nickname`, and the catalog reads `nickname` with the default entry

  Scenario: A failed write records no predicate
    When a write naming `<alias>` fails before it commits and `alias` was not declared
    Then the catalog does not hold `alias`

  Scenario: Concurrent first writes both commit
    When two writes read before either commits, the first writes `<a> <score> 12` and the second `<b> <score> "high"`, and `score` was not declared
    Then both commit, and the catalog holds `score` with the default entry
