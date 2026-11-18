# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: One-frame writes, Drops and retried Commits on their way to partitions
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: A retried Commit still in a partition's commit step is busy
    When a client retries a Commit through its session, and the partition answers its Recall with code 14
    Then the Commit is refused with code 14

  Scenario: A retried Commit past a partition's last request is out of order
    When a client retries a Commit through its session, and the partition answers its Recall with code 18 naming its last request
    Then the Commit is refused with code 18 naming that request

  Scenario: A retried Commit a partition cannot be asked of stays in doubt
    When a client retries a Commit through its session, and one of the two partitions has no process joined
    Then the Commit is refused as unavailable

  Scenario: A DirectWrite refused naming another owner is resent there once
    When partition 1 refuses a DirectWrite with code 26 naming partition 2 as the owner
    Then the frontend resends it to partition 2, and its reply reaches the client

  Scenario: A one-frame write and a Drop whose partition closes under them are unavailable
    When a partition closes its connection while it holds a DirectWrite and a Drop's Bind
    Then each is refused as unavailable

  Scenario: Drops of a tenant, of a graph and of a tenant's data
    When a client drops the whole tenant, a graph and the tenant's data
    Then each Drop is relayed to the partition, and its reply reaches the client
