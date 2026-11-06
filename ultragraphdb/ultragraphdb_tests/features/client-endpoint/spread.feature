# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Requests routed over a spread tenant
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A first request spreading over partitions is refused before any Bind
    When a transaction's first Write names subjects the frontend's map places in partitions 1 and 2
    Then it is refused with code 23 on the transaction's stream, `detail` partitions 1 and 2, no partition receives a Bind, and the transaction ends

  Scenario: A first read the map does not move is refused
    When a transaction's first request is a read, compute answers code 23, and the refreshed map leaves the read's subject where it was
    Then the client's read is refused with code 23, and its next request on the transaction's stream too
