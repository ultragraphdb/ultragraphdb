# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@system @hosting
Feature: Readiness, health and shutdown of a storage process hosting the system group
  openspec/changes/stage-5-8-system-group/specs/role-binaries/spec.md, "Readiness, health and shutdown", storage's parts:
  the storage binary hosting the system group, settings served by the config double.

  Scenario: Storage is not ready before recovery
    When a storage process restarts with 10,000 rounds to replay
    Then `/health` answers once it holds its settings, and `/ready` answers only after the replay

  Scenario: Storage flushes on shutdown
    When storage receives SIGTERM after acknowledging commits
    Then every acknowledged commit is applied to its store before it exits, and a restart replays nothing
