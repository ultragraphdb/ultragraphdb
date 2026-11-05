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
    When a statement is prepared, the catalog of its tenant changes, and the statement is executed
    Then the execution is refused as stale, and preparing it again succeeds

  Scenario: A literal at a node slot is refused
    When a statement's subject parameter is bound to a literal
    Then the execution is refused with code 11, `detail` the statement

  Scenario: Describe answers from the data dictionary
    When `name` is declared a property with a `term` index and a client sends Describe for the tenant
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

  Scenario: A malformed write is refused
    When a DirectWrite holds no change, or a ConditionalWrite no operation
    Then it is refused with `CountMismatch`, naming `changes` or `operations`

  Scenario: A write in one frame holds what it uses
    When a DirectWrite or ConditionalWrite holds a name, or names a request number that does not fit its session
    Then it is refused with `CountMismatch` naming `names`, or `FieldValue` naming `request`

  Scenario: A client may not send a role command
    When a client connection sends a block-lease command
    Then the frame is refused with a protocol error naming the command, and nothing is leased

  Scenario: A Scan naming a transaction names its IRIs by their text
    When a Scan names a transaction and the IRIs its first ranges look up
    Then the view reads each IRI's text from the Scan's term table, borrowed from the frame

  Scenario: A Scan naming no transaction holds no term table
    When a Scan with `transaction` zero holds a term
    Then it is refused with `CountMismatch` naming `terms`

  Scenario: A Scan's term table holds at most as many terms as ranges
    When a Scan's term table holds more terms than the Scan has ranges
    Then it is refused with `CountMismatch` naming `terms`

  Scenario: A Scan naming a transaction reads as the transaction does
    When a Scan names a transaction with `at` not 1, a nonzero `epoch`, or a term that is not an IRI
    Then it is refused with `FieldValue` naming `at`, `epoch` or `scan term`

  Scenario: A Scan names its deadline
    When a Scan whose `epoch` is 0 holds `deadline_ms` above 0, with a transaction or with none
    Then each is refused with `FieldValue` naming `deadline_ms`

  Scenario: A list larger than a frame continues across frames
    When a list's object records span two Entries frames, the first with `continues` 1
    Then the second's first entry holds the same key, and the records of both are the list's, in order

  Scenario: A frame whose value continues has end 0 and an entry
    When a frame holds `continues` 1 with `end` not 0, or with no entry
    Then it is refused with `FieldValue` naming `continues`

  Scenario: A continued value cannot stop
    When the frame after one with `continues` 1 holds no entry, or a first key that is not the same
    Then the receiver refuses it with `FieldValue` naming `entry order`

  Scenario: An answer ending early cannot read as whole
    When a frame carries `LAST` before the last range has ended, or the last range ends with no `LAST`
    Then the receiver refuses it with `FieldValue` naming `end`

  Scenario: An object record past its value is refused
    When an object record's encoding or statement IDs run past the value
    Then the reader refuses it with `EntryOutOfBounds`
