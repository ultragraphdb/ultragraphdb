#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Storage simulation of the posting lists
  The posting lists run on the simulated store under its seeded faults, their workload checked against a shadow
  model of every key's commits, beside the simulator's own model of the store. The simulator's other scenarios run in
  ultragraphdb_store_sim's features.

  Scenario: The posting lists run on the simulated store
    When the posting-list operations of a workload run on the simulated store
    Then they pass the same checks as on fjall

  Scenario: A corrupted value is refused
    When a stored posting list's bytes are corrupted and the list is read
    Then the read fails with an error naming the key, and no value is returned
