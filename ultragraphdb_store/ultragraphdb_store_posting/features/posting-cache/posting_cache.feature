#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The posting cache
  The cache holds immutable lists keyed by key, each with the timestamp it is current from and the cache's epoch; a
  commit replaces or removes the entries of the lists it writes under the writer lock, a fill is refused after a
  commit to its key or an epoch change, and drops, schema changes and rebuilds advance the epoch.

  Scenario: A read after a commit sees the commit
    When a list is cached, a commit at 20 changes it, and it is read at 21
    Then the read reflects the commit at 20

  Scenario: Cached and uncached reads agree under concurrent commits
    When commits and reads at increasing timestamps run concurrently against the same keys with the cache on and with it off
    Then every read returns the same result in both runs

  Scenario: No reader sees the batch without the cache update
    When a commit writes a list that is cached
    Then no read that sees the batch in the store returns the cached list from before the batch

  Scenario: A stale fill is refused
    When a reader misses, a commit then writes the key, and the reader then inserts its list
    Then the insert is refused and a read at the commit's timestamp reflects the commit

  Scenario: A drop clears the cache
    When a cached predicate is dropped
    Then a read of that predicate finds nothing

  Scenario: A fill begun before a drop is refused
    When a reader misses a list, the list's predicate is then dropped, and the reader inserts its list
    Then the insert is refused, and a read of the predicate finds nothing
