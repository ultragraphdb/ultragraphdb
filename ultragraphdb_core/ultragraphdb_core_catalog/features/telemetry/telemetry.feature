#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Telemetry
  The schema store logs what Dgraph's schema package logs, at Dgraph's level, with the facts of Dgraph's message as
  fields.

  Scenario: A schema key that does not parse is logged
    When the schema is loaded from a store holding a schema key that does not parse
    Then the load skips the key and emits an error event `Error while parsing key` with the key as a field
