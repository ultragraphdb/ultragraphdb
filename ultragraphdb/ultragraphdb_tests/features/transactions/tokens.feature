# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

@core
Feature: Commits answer a session token through the three binaries
  openspec/changes/stage-5-5-three-role-refactor/specs/transactions/spec.md, through the real frontend, compute and storage, the client crates' own client sending every request.

  Scenario: A token names its commit
    When a transaction commits in partition 1 at position 40 with a version of epoch 12
    Then its reply carries the token (partition 1, position 40, epoch 12)

  Scenario: A read with a token sees the commit
    When a client commits `<a> <knows> <b>` and reads `<a>`'s statements with the commit's token from another connection
    Then the read returns `<a> <knows> <b>`

  Scenario: A reply through no session carries no token
    When a transaction and a Drop each commit through no session, `session` all zero
    Then each reply's `commit_version` is its version and its token is 24 zero bytes, and a receiver accepts it

  Scenario: A named read version takes nothing from a token
    When a read-only transaction at snapshot level names a read version in epoch 12, and carries a token of epoch 20
    Then its reads answer at the named version, and none waits for epoch 20

  Scenario: A token above the open epoch is refused
    When the sequencer's open epoch is 30, and a Begin, a DirectWrite and a ConditionalWrite each carry a token of epoch 31
    Then each is refused with Error 1 on its stream, the message naming epoch 31

  Scenario: A one-frame write carries the session's token
    When a client's session commits a DirectWrite whose reply's token is of epoch 12, resolves a resend whose stored reply's is of epoch 9, then sends a ConditionalWrite through the session
    Then the ConditionalWrite carries the token of epoch 12

  Scenario: A zero token replaces nothing
    When a client's session holds a token of epoch 12, and a commit through no session answers the zero token
    Then the session still sends the token of epoch 12
