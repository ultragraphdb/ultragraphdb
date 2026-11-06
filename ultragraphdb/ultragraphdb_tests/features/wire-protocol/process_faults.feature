# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: A role process lost under a client
  openspec/changes/stage-5-5-three-role-refactor/specs/wire-protocol/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: Code 17 on a commit is in doubt
    When a client's Commit through a session is answered code 17
    Then the client answers `InDoubt`, keeps the request number, and sends no other commit through the session until a resend resolves it

  Scenario: Code 17 runs a read again and leaves a commit in doubt
    When a read is refused with code 17, and a Commit with code 17
    Then the client's error says of the first that the read runs again, and of the second that it is in doubt

  Scenario: A tenant's data drop keeps its catalog
    When a client drops a tenant's data after declaring `name` a property
    Then no statement of the tenant remains, and Describe still lists `name`

  Scenario: A Register answered busy is sent again
    When a client of 4 commit attempts registers, and a full registry answers its first two Registers code 14
    Then the client sends the Register three times, each on a new stream, and returns the session

  Scenario: A Register busy past its attempts returns code 14
    When a client of 3 commit attempts registers, and a full registry answers every Register code 14
    Then the client sends the Register three times and returns code 14, and the connection stays open

  Scenario: A read with a future token waits
    When a read-only transaction begins with a token of epoch 30 while epoch 29 is the latest closed, reads, and epoch 30 closes
    Then the read answers after epoch 30 closes, at that epoch or later

  Scenario: A read past its transaction's deadline is refused
    When a read-only transaction begins with a token of epoch 30 and a deadline of 50 ms, and epoch 30 does not close within it
    Then its read is refused as timed out

  Scenario: A long deadline survives the client's timeout
    When a read-only transaction's deadline is 40 s, and its read waits 35 s for a close, the client's timeout set at 30 s
    Then the connection stays open, and the read answers once the close arrives

  Scenario: A stream without credit runs no timeout
    When a read's stream has spent its credit, and its caller takes no batch for longer than the timeout
    Then the connection stays open
