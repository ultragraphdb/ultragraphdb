#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: bodies, statements, describe, result batches and the conformance suite
  A body is a fixed part and sections located by offsets and lengths, checked once and read in place; statements are
  prepared from quad patterns and refused as stale once their catalog changes; Describe answers from the data
  dictionary; result batches are columnar with one term table; every truncation of every golden frame is refused.
  These are the scenarios whose decision this crate makes. Handshake, streams, credit and cancellation run in the
  bus's features; "A refused request keeps the connection" and "A large result streams" in the server's (task group
  10), which owns the dispatch of a stale statement's refusal and the cutting of a large result into frames.

  Scenario: An offset past the body is refused
    When a body's section offset points past its end
    Then the frame is refused with an error naming the section

  Scenario: A stale statement is refused
    When a statement is prepared, the catalog of its namespace changes, and the statement is executed
    Then the execution is refused as stale, and preparing it again succeeds

  Scenario: Describe answers from the data dictionary
    When `name` is declared a property with a `term` index and a client sends Describe for the namespace
    Then the answer lists `name` as a property with its `term` index, as the data dictionary reports it

  Scenario: A repeated term is sent once per batch
    When a batch of 1,000 rows names the same IRI in every row
    Then its term table holds that IRI once

  Scenario: Every truncation is refused cleanly
    When each golden frame is truncated at every length below its size and sent
    Then each truncation yields a `ProtocolError` and none panics
