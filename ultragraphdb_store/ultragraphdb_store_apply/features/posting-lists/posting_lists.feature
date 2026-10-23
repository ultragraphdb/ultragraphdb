#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Posting lists
  The posting-lists scenario that is index maintenance's rule (Dgraph's index.go): a delete of a scalar predicate's
  value that is not the value it holds is ignored. The other posting-lists scenarios run in
  ultragraphdb_store_posting's features.

  Scenario: Deleting a non-matching scalar value changes nothing
    When `name` holds `A` and a delete of `name` with value `B` is applied
    Then `name` still holds `A`
