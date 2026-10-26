#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The term dictionary under the simulator's faults
  The simulated store holds the keyspace without versions under the same faults and shadow model as its versioned
  keyspaces, and the dictionary stays whole across every fault class.

  Scenario: The dictionary workload passes on the simulator
    When a seeded workload creates terms and writes quads naming them under every fault class
    Then every stored UID and literal hash has its dictionary entry after every step and every recovery
