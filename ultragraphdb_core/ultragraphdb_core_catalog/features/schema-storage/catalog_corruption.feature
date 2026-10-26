#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: A catalog entry that does not read back
  A catalog key that does not parse, or an entry that does not decode, refuses the load of its namespace's catalog with
  the store's corruption error naming the key; nothing is skipped, since a declaration skipped would let writes go on
  under the default declaration.

  Scenario: A catalog key that does not parse refuses the load
    When a namespace's catalog is loaded from a store holding a catalog key that does not parse
    Then the load fails with a corruption error naming the key, and nothing panics
