#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Quads on posting lists
  A statement is stored in the forward, reverse, subject-predicate and object-predicate lists of its graph, a vector in
  none that names it as an object; statistics are kept by deltas; large lists split; a fresh term's lists are not read.

  Scenario: A triple maintains four lists
    When <a> <knows> <b> is written to the default graph
    Then the forward list of knows for a holds b, the reverse list of knows for b holds a, the subject predicates of a hold knows, and the object predicates of b hold knows

  Scenario: A bound literal object is a list read
    When <a> <name> "Ann" and <c> <name> "Ann" are written
    Then the reverse list of name for "Ann" holds a and c

  Scenario: A vector has no reverse entry
    When <a> <embedding> "[0.5, -1]"^^ugdb:float32vector is written
    Then the forward list of embedding for a holds the vector, and no reverse or object-predicate key names it

  Scenario: Graphs do not mix
    When <a> <p> <b> is written to graph <g1> and read from graph <g2>
    Then the read returns nothing

  Scenario: Inserts raise the statement count
    When 100 distinct statements of knows are written to the default graph
    Then the statistics of knows report 100 statements

  Scenario: Datatypes seen are counted
    When <a> <age> 30 and <b> <age> "unknown" are written
    Then the statistics of age report one xsd:integer and one xsd:string literal

  Scenario: A large reverse list splits
    When a reverse list grows past the split threshold and is rolled up
    Then it is stored as parts, and reading it returns every subject in order

  Scenario: Linking a new subject reads none of its lists
    When a write creates <new> and writes <new> <knows> <b>
    Then the write reads no forward, subject-predicate or statement key of <new>
