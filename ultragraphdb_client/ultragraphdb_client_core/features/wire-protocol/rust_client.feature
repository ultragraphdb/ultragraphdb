#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: the Rust client
  The Rust client speaks the protocol: handshake, sessions, transactions, data requests, prepared statements,
  commit-time operations and result streams read through views. It retries a commit only through its session, so a
  retry never commits twice. Here the client's sans-I/O core runs against the scripted server over the simulated
  network; the end-to-end runs against the node are task 10.1's.

  Scenario: The client retries a lost reply through its session
    When the connection drops after a commit is sent and the client reconnects and retries
    Then the commit is applied once and the client receives its reply

  Scenario: Credit stops a stream
    When a client grants credit for two frames of a result of ten
    Then the server sends two frames and waits until more credit arrives

  Scenario: Cancel ends a stream
    When a client cancels a stream mid-result
    Then no further frame of that stream arrives, and other streams continue

  Scenario: A stale statement is refused
    When a statement is prepared, the catalog of its namespace changes, and the statement is executed
    Then the execution is refused as stale, and preparing it again succeeds

  Scenario: A repeated term is sent once per batch
    When a batch of 1,000 rows names the same IRI in every row
    Then its term table holds that IRI once
