# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A role starts from five bootstrap variables
  openspec/changes/stage-5-5-three-role-refactor/specs/config-service/spec.md

  Scenario: The service is down
    When a storage process starts and no config service answers for its patience
    Then it exits with code 69 and an error naming the URL, the target and the instance

  Scenario: The service starts late
    When a storage process starts and the config service begins answering a moment later
    Then the process takes its settings and starts

  Scenario: A wrong key is not retried
    When a storage process starts with a key the service refuses
    Then it exits with code 78 at once, naming the URL

  Scenario: An instance of another role is refused
    When the storage binary starts with `ULTRAGRAPHDB_CONFIG_INSTANCE=compute_1`
    Then it exits with code 78 naming the instance and its role, without asking the service

  Scenario: A stray variable is refused
    When a role starts with `ULTRAGRAPHDB_STORAGE_PARTITION_COUNT=2` in its environment
    Then it exits with code 78 naming the variable

  Scenario: A new instance takes what the others hold
    When a new storage process starts beside the running instances, the settings unchanged
    Then it receives the active revision the running instances report

  Scenario: A health answer names the revision
    When a role runs a revision of a target as an instance
    Then its `/health` body names the target, the instance and the revision

  Scenario: A role reads its settings over TLS
    When a role asks a config service that speaks TLS, trusting its authority
    Then it takes its settings over the verified connection
