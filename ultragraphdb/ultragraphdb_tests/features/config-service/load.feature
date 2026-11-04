# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A role loads another revision by restarting
  openspec/changes/stage-5-5-three-role-refactor/specs/config-service/spec.md

  Scenario: A load restarts the node at the revision
    When an administrator asks a storage process to load a revision the service holds
    Then it answers 202, flushes as at SIGTERM, keeps its process id, and its `/health` names the revision

  Scenario: A load without the admin key changes nothing
    When a request to load a revision carries the read key
    Then it is refused with status 403, and the role runs on

  Scenario: A load of a revision that cannot be fetched changes nothing
    When a role is asked to load a revision the service does not hold, or while the service is down
    Then the request is refused naming the revision and why, and the role runs on at its revision

  Scenario: A loaded revision does not outlive a crash unless active
    When a role loaded a revision while another is active, and then crashes and restarts
    Then it starts at the active revision, and its `/health` says so

  Scenario: A setting named too early is named back
    When an active revision names a setting this release does not hold, and an instance restarts
    Then it exits with code 78 naming the setting
