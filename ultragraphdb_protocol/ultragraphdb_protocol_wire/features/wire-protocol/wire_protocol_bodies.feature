#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Wire protocol: bodies, statements, describe, result batches and the conformance suite
  A body is a fixed part and sections located by offsets and lengths, checked once and read in place; statements are
  prepared from quad patterns and refused as stale once their catalog changes; Describe answers from the data
  dictionary; result batches are columnar with one term table; every truncation of every golden frame is refused;
  requests name their terms by text in a term table of their own.
  These are the scenarios whose decision this crate makes. Handshake, streams, credit and cancellation run in the
  bus's features; "A refused request keeps the connection" and "A large result streams" in the server's (task group
  10), which owns the dispatch of a stale statement's refusal and the cutting of a large result into frames.

  Scenario: An offset past the body is refused
    When a body's section offset points past its end
    Then the frame is refused with an error naming the section

  Scenario: A stale statement is refused
    When a statement is prepared, the catalog of its namespace changes, and the statement is executed
    Then the execution is refused as stale, and preparing it again succeeds

  Scenario: A literal at a node slot is refused
    When a statement's subject parameter is bound to a literal
    Then the execution is refused with code 11, `detail` the statement

  Scenario: Describe answers from the data dictionary
    When `name` is declared a property with a `term` index and a client sends Describe for the namespace
    Then the answer lists `name` as a property with its `term` index, as the data dictionary reports it

  Scenario: A repeated term is sent once per batch
    When a batch of 1,000 rows names the same IRI in every row
    Then its term table holds that IRI once

  Scenario: A decimal the payload cannot hold is kept by its lexical form
    When a literal `"0.1234567890123456789"^^xsd:decimal`, of 19 fraction digits, is written and read back
    Then its term carries tag `0x31`, a known datatype's code and the hash of its lexical form, and the batch's text holds the lexical form unchanged

  Scenario: Every truncation is refused cleanly
    When each golden frame is truncated at every length below its size and sent
    Then each truncation yields a `ProtocolError` and none panics

  Scenario: A request names an IRI by its text
    When a Read binds its subject to the IRI `http://ex.org/a`
    Then the view reads an IRI entry whose text is `http://ex.org/a`, borrowed from the frame

  Scenario: A request names a blank node by its label
    When a Write inserts a statement whose subject is the blank node labelled `b1`
    Then the view reads a blank-node entry labelled `b1`

  Scenario: A request names a typed literal by its datatype's IRI
    When an Execute binds the literal `35` of datatype `http://www.w3.org/2001/XMLSchema#integer`
    Then the view reads a literal entry of lexical form `35` and that datatype's IRI

  Scenario: A request names a language-tagged literal
    When an Operation's value is `chat` tagged `fr`
    Then the view reads a literal entry of lexical form `chat` and language tag `fr`

  Scenario: A request names a triple term by its parts
    When a Write's object is the triple term of `http://ex.org/a`, `http://ex.org/knows` and `http://ex.org/b`
    Then the view reads a triple-term entry naming three earlier entries, those IRIs

  Scenario: A forward index is refused
    When a triple term's entry names an entry at or after its own index
    Then the frame is refused with an error naming the entry and the index

  Scenario: A triple term nested deeper than the maximum is refused
    When a request's term table holds a triple term nested 33 deep
    Then the frame is refused with an error naming the entry and its depth
