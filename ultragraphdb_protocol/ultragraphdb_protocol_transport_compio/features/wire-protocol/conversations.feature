#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: handshake, authentication, streams, credit and cancellation
  A connection begins with Hello and Welcome and authenticates before its first request; each request takes a stream,
  answered independently; a result stream sends frames only while the client holds credit; Cancel ends a stream. These
  are the conformance suite's conversations over the transport, between a client connection and a server connection
  on the simulated network, the server's connection run by `ServerConnection`; the server's dispatch of requests is
  task group 10's, so a server here plays its session, queuing replies and refusing a malformed body directly.

  Scenario: Credit stops a stream
    When a client grants credit for two frames of a result of ten
    Then the server sends two frames and waits until more credit arrives

  Scenario: Cancel ends a stream
    When a client cancels a stream mid-result
    Then no further frame of that stream arrives, and other streams continue

  Scenario: A handshake agrees on a protocol
    When a client speaking protocols 1 to 3 says Hello to a server speaking 1 to 2
    Then the server welcomes it at protocol 2 with its limits

  Scenario: Streams are answered in whatever order their results are ready
    When a client sends requests on streams 1 and 2 and the result of stream 2 is ready first
    Then the reply of stream 2 arrives before the reply of stream 1, each on its own stream

  Scenario: Credit holds over split and delayed frames
    When a client grants credit for two frames of a result of ten over a network that splits and delays frames
    Then the server sends two frames and waits until more credit arrives

  # Requirement: Frames are validated before use and never panic

  Scenario: A malformed body keeps the connection
    When a Write whose term table repeats an entry arrives on a transaction's stream while another stream reads
    Then the transaction's stream receives an error of code 1 naming the rule, its transaction ends, and the connection and the other stream continue

  Scenario: A request on stream 0 closes the connection
    When a client sends Begin on stream 0
    Then the server answers an error of code 1 on stream 0 and closes the connection

  # Requirement: Frames are at most 1 MiB

  Scenario: A frame past the receiver's announced maximum is refused
    When a client sends a frame larger than the Welcome's `max_frame`
    Then the server refuses it as too large, answers on stream 0 and closes the connection

  # Requirement: Handshake, streams, credit and cancellation

  Scenario: A transaction's reads run on streams of their own
    When a transaction runs two reads at once, each on a stream of its own naming the transaction
    Then both stream their results at the transaction's snapshot, each with its own credit, and the transaction then commits on its own stream

  Scenario: Cancelling a read keeps its transaction
    When a client cancels one of its transaction's reads mid-result and then commits the transaction
    Then no further frame of the read arrives, and the commit succeeds

  Scenario: A transaction's end ends its reads
    When a transaction commits while one of its reads has frames left to send
    Then the read's stream sends no further frame, and the commit's reply arrives

  # Requirement: Connections authenticate

  Scenario: A client authenticates with SCRAM-SHA-256
    When a server requiring authentication lists `SCRAM-SHA-256` and a client completes the exchange with the principal's password
    Then the server's final Authenticate names the principal, and the client's requests are served

  Scenario: A wrong password is refused without saying why
    When a client's SCRAM proof does not verify, once for a principal that exists and once for a name no principal holds
    Then each is refused with code 19 on stream 0 and the connection closed, and the two Error frames are the same bytes

  Scenario: A request before authentication closes the connection
    When a client sends Begin after Welcome and before the final Authenticate
    Then the server answers code 20 on stream 0 and closes the connection

  Scenario: An unfinished authentication times out
    When a client sends Hello and then nothing for longer than the authentication timeout
    Then the server answers code 20 on stream 0 and closes the connection

  Scenario: A credential is renewed on the open connection
    When a connection whose credential expires runs a new exchange as the same principal before the expiry, while one of its reads streams
    Then the read continues, and the connection serves requests past the first expiry

  Scenario: Renewing as another principal is refused
    When a connection's new exchange ends as another principal
    Then the server answers code 19 on stream 0 and closes the connection

  Scenario: A server without authentication says so
    When a client connects to a server configured without authentication
    Then the Welcome says no authentication is required and lists no mechanism, and the client's requests are served without an exchange
