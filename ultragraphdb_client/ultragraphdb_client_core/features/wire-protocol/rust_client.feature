#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: the Rust client
  The Rust client speaks the protocol: handshake, authentication and its renewal, sessions, transactions, data
  requests on streams of their own, prepared statements, commit-time operations, drops and result streams read
  through views. It retries a commit only through its session, so a retry never commits twice: a commit refused as
  not committed (code 18) runs its transaction again, a busy session (code 14) is retried after a delay, and a retry
  refused as evicted (code 12) reports an unknown outcome and is not retried. Here the client's sans-I/O core runs
  against the test server over the simulated network; the end-to-end runs against the node are task 10.1's.

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

  Scenario: A retry of a commit that never arrived runs the transaction again
    When the connection drops before a commit reaches the commit step, and the client retries it through its session
    Then the retry is refused with code 18, the client runs its transaction again, and it commits once

  Scenario: A retry of an evicted session reports an unknown outcome
    When the client retries a commit whose session the table evicted
    Then the retry is refused with code 12, and the client reports the commit's outcome as unknown without retrying

  Scenario: A repeated frame closes the connection
    When the network delivers one of the client's frames twice
    Then the server refuses the second as out of sequence and closes the connection, and the client reconnects and retries its commit through its session, which commits once

  Scenario: A transaction's reads run on streams of their own
    When a transaction runs two reads at once, each on a stream of its own naming the transaction
    Then both stream their results at the transaction's snapshot, each with its own credit, and the transaction then commits on its own stream

  Scenario: Cancelling a read keeps its transaction
    When a client cancels one of its transaction's reads mid-result and then commits the transaction
    Then no further frame of the read arrives, and the commit succeeds

  Scenario: A transaction's end ends its reads
    When a transaction commits while one of its reads has frames left to send
    Then the read's stream sends no further frame, and the commit's reply arrives

  Scenario: A client authenticates with SCRAM-SHA-256
    When a server requiring authentication lists `SCRAM-SHA-256` and a client completes the exchange with the principal's password
    Then the server's final Authenticate names the principal, and the client's requests are served

  Scenario: A credential is renewed on the open connection
    When a connection whose credential expires runs a new exchange as the same principal before the expiry, while one of its reads streams
    Then the read continues, and the connection serves requests past the first expiry
