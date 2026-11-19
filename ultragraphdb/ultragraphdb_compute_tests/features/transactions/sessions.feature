# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A partition's session table follows the registry
  openspec/changes/stage-5-8-system-group/specs/transactions/spec.md, "Exactly-once commits through sessions": compute
  and a storage process hosting the system group, its closes asked for by the test.

  Scenario: A partition purges an entry only after the registry's eviction
    When a session has committed in partition 1, and the registry evicts it
    Then partition 1 keeps the session's entry until the close carrying the eviction is delivered to it, and then purges it
