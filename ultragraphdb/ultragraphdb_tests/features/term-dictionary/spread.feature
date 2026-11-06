# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: The dictionary over a spread tenant
  openspec/changes/stage-5-5-three-role-refactor/specs/term-dictionary/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: Transactions naming only new blank nodes bind in turn
    When three transactions whose first writes name only new blank nodes arrive over three partitions
    Then the frontend binds them to partitions 1, 2 and 3 in turn, and each blank node's bucket is one its partition owns

  Scenario: A forged skolem IRI in a write is refused at its home
    When a transaction in partition 1 writes an edge to a skolem IRI whose bucket partition 2 owns, and partition 2's dictionary does not hold it
    Then the registration's outcome is 7, and the Write is refused with code 7, ending the transaction
