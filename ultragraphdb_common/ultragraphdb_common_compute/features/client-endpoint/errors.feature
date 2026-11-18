# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Errors reach the client through the error table
  openspec/changes/stage-5-5-three-role-refactor/specs/client-endpoint/spec.md

  Scenario: A partition's invariant violation answers Unavailable
    When a partition stops on an invariant violation and a client writes to it
    Then the client receives `Unavailable`, and requests to other partitions still succeed

  Scenario: A role connection closing under a request answers code 17
    When the connection between the frontend and a storage process closes under a Write and a Commit
    Then the client receives code 17 for each, the Commit is in doubt, and requests to other partitions still succeed

  Scenario: A vector index is refused
    When a client declares a vector index on `embedding`
    Then the declaration is refused with an error naming `embedding`, and the catalog is unchanged

  Scenario: A transaction's first refusal is the one its client sees
    When a client sends a Write the frontend refuses and, before reading its answer, a second Write of the same transaction
    Then the client receives the first Write's refusal on the transaction's stream, and nothing after it

  Scenario: A role connection's dial past its bound closes it
    When the frontend dials a role address whose peer accepts and says nothing, carrying a request
    Then past `transaction_ms` the connection is closed, the request is told so, and the log names the address and the bound
