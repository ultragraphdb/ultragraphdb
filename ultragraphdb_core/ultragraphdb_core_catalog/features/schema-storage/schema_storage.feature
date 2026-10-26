#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Schema storage
  Predicate and type definitions are written to schema and type keys with the discard flag, and the schema state is
  loaded from them at startup, per namespace. The scenarios of writes to unknown predicates run in
  ultragraphdb_store_apply's features, where requests commit.

  Scenario: The schema survives a restart
    When a schema is applied and the store is closed and reopened
    Then the loaded schema state equals the state before closing

  Scenario: A deleted predicate stays deleted after a restart
    When a predicate's schema is deleted and the store is reopened
    Then the loaded schema does not hold the predicate
