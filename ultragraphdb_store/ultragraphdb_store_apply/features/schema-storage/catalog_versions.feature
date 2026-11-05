#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The schema persists in the metadata group
  Each tenant's catalog is committed to the metadata group as numbered versions and loaded from it at start, in
  place of catalog keys in a partition's store: a partition holds the version in effect in memory, and no other copy.
  The scenario of a version that does not decode runs in ultragraphdb_core_catalog's features.

  Scenario: The catalog survives a restart
    When declarations are made and the frontend and storage are stopped and started
    Then the loaded catalog equals the catalog before stopping

  Scenario: A removed declaration stays removed after a restart
    When a predicate's declaration is removed and the roles are restarted
    Then the loaded catalog does not declare the predicate

  Scenario: No catalog key is written to a partition
    When a predicate is declared
    Then no partition's store holds a catalog key for it
