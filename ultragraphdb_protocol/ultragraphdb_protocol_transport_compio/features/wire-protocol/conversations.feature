#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: handshake, streams, credit and cancellation
  A connection begins with Hello and Welcome; each request takes a stream, answered independently; a result stream
  sends frames only while the client holds credit; Cancel ends a stream. These are the conformance suite's
  conversations over the transport, between a client connection and a server connection on the simulated network; the
  server's dispatch of requests is task group 10's, so a server here queues its result frames directly.

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
