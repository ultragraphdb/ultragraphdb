# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@system @hosting
Feature: Every runtime of a storage process hosting the system group runs where its settings pin it
  openspec/changes/stage-5-8-system-group/specs/role-binaries/spec.md, "Each role runs one runtime per core", storage's part:
  the storage binary hosting the system group, settings served by the config double. Linux binds and reads each binding
  back; macOS binds nothing and says so for each runtime.

  Scenario: Every runtime of a pinned role holds a CPU of its own
    When a storage process of five partitions starts with `pin_cores = true` and `pin_skip = 1` where it is permitted six CPUs
    Then each partition's runtime logs a CPU of its own among the five highest, read back from the operating system, and no runtime of the role runs on the lowest
