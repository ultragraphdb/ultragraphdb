#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Schema storage
  The write scenarios of schema storage: a write to a predicate the schema does not hold defines it in a schema
  overlay of its request, committed in the request's batch; the first commit's schema wins. The scenarios of the
  schema surviving a restart run in ultragraphdb_core_schema's features.

  Scenario: A first write defines the predicate
    When `<0x1> <nickname> "Al"` is written and `nickname` has no schema
    Then the schema holds `nickname` with type default

  Scenario: A failed write defines no predicate
    When a request writing `<0x1> <alias> "Al"` fails before it commits and `alias` had no schema
    Then the schema does not hold `alias`

  Scenario: A new uid predicate is a list
    When `<0x1> <knows> <0x2>` is written and `knows` has no schema
    Then the schema holds `knows: [uid]`

  Scenario: Concurrent first writes keep the first commit's type
    When two requests read before either commits, the first writes `<0x1> <score> "12"^^<xs:int>` and commits, and the second writes `<0x2> <score> "high"` and commits after it
    Then the schema holds `score: int`, and the second commit fails with `strconv.ParseInt: parsing "high": invalid syntax` and writes nothing
