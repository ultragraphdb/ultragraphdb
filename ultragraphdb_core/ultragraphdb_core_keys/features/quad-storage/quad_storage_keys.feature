#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The order and regions of quad keys
  A predicate's forward keys sort by subject, and every key maps to one region computed from the key alone.

  Scenario: Forward keys of a predicate sort by subject
    When forward keys of one predicate are built for subjects with UIDs 1, 2, 256 and 2^40
    Then their byte order equals the UIDs' numeric order

  Scenario: Two subjects of one predicate share a region
    When the forward keys of knows for a and for c in the default graph are mapped to regions
    Then both map to the same region, and the forward key of name for a maps to another
