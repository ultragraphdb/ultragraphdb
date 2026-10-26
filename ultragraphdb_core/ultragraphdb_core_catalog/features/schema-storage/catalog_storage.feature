#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The catalog in the store
  The catalog is written to catalog keys with the discard flag and loaded from them when the store opens.

  Scenario: The catalog survives a restart
    When declarations are made and the store is closed and reopened
    Then the loaded catalog equals the catalog before closing

  Scenario: A removed declaration stays removed after a restart
    When a predicate's declaration is removed and the store is reopened
    Then the loaded catalog does not declare the predicate
