#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: The mapping contract
  The store holds statements, a subject's predicate and object in a graph, each with a statement ID when it carries
  properties or repeats a triple. The RDF view presents them as quads, statement IDs as reifiers with annotations; the
  property-graph view presents them as nodes, labels, properties and edges, a statement ID as an edge of its own.

  Scenario: A plain triple has no statement ID
    When the statement <a> <knows> <b> is written to the default graph
    Then it is held once, with no statement ID

  Scenario: A parallel edge gets its own statement ID
    When <a> <knows> <b> is written twice to the default graph as two distinct edges
    Then both statements are held, each with its own statement ID

  Scenario: Parallel edges are one quad
    When <a> <knows> <b> is held twice with two statement IDs
    Then the RDF view holds the quad <a> <knows> <b> once, with two reifiers

  Scenario: An edge property is an annotation
    When the statement <a> <knows> <b> carries the property <since> "2020"^^xsd:gYear
    Then the RDF view holds the quad and a reifier of it annotated with <since> "2020"^^xsd:gYear

  Scenario: A label is an rdf:type statement
    When <a> rdf:type <Person> is held
    Then the property-graph view shows node a with the label Person

  Scenario: Parallel edges stay separate
    When <a> <knows> <b> is held twice with two statement IDs
    Then the property-graph view shows two knows edges from a to b

  Scenario: A multi-valued property is a list
    When <a> <nick> "Al" and <a> <nick> "Ally" are held and nick is not declared single-valued
    Then the property-graph view shows nick on a as a list of both values

  Scenario: A property name is an IRI under the base
    When the base IRI is http://ex.org/ and a property age is written through the property-graph view
    Then the RDF view holds it under the predicate <http://ex.org/age>

  Scenario: A blank node round-trips as a blank node
    When _:x <knows> <b> is written through the RDF view and read back through it
    Then the subject is presented as a blank node, the same blank node for every statement that named _:x

  Scenario: A non-canonical integer keeps its lexical form
    When <a> <n> "01"^^xsd:integer is written
    Then the RDF view returns "01"^^xsd:integer, a term distinct from "1"^^xsd:integer

  Scenario: An ill-typed literal is kept
    When <a> <n> "twelve"^^xsd:integer is written
    Then the RDF view returns "twelve"^^xsd:integer unchanged

  Scenario: Quads round-trip
    When a set of quads over two named graphs, with blank nodes, language-tagged and typed literals, is written and read back through the RDF view
    Then the quads read back equal the quads written

  Scenario: A property graph round-trips
    When nodes with labels and properties, and parallel edges with properties, are written and read back through the property-graph view
    Then the nodes, labels, properties, edges and edge properties read back equal those written
