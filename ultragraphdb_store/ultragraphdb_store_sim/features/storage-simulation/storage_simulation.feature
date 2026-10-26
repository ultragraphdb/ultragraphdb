#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Storage simulation
  The port reaches its store through the Store trait; the simulated store injects storage faults under a seeded,
  deterministic run and checks the store against a shadow model after every step and every recovery. The scenarios
  that need posting lists, "The posting lists run on the simulated store" and "A corrupted value is refused", run in
  ultragraphdb_store_posting's features.

  Scenario: A seed replays its run
    When a seed is run twice
    Then both runs perform the same operations, inject the same faults and end in the same state

  Scenario: A power loss keeps every synced batch
    When batches are written, some synced, and a power loss is injected
    Then after recovery every synced batch is visible whole, the surviving batches are a prefix of batch order, no batch is visible in part, and the applied timestamp is the highest commit timestamp the store holds

  Scenario: A failed batch poisons the store
    When a batch fails during its persist, a write follows, and the store is reopened
    Then the write before reopening is refused, and after reopening the failed batch is present whole or absent whole

  Scenario: A drop interrupted by a power loss finishes at recovery
    When a drop of everything has synced its record and a power loss is injected before it deletes the record
    Then after recovery the store holds only the entries the drop was given

  Scenario: A violated invariant names its seed
    When a run breaks an invariant
    Then the simulator stops and reports the invariant, the step and the seed

  Scenario: A power loss keeps a batch's two halves together
    When batches write versioned and versionless keys together and a power loss is injected
    Then after recovery every surviving batch's versionless entries are present, and every lost batch's are absent
