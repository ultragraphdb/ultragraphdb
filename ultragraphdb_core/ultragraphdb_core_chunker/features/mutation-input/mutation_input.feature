#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Mutation input
  An RDF line parses into an N-Quad with subject, predicate, object id or value, value type, language, facets and
  namespace label, as Dgraph's chunker parses it, and is refused where Dgraph refuses it. A JSON object or array
  parses into N-Quads: `uid` names the node, nested objects become edges, `|` separates a predicate from its facet,
  and a node without `uid` becomes a blank node.

  Scenario: Dgraph's RDF parser cases pass
    When the 130 cases of Dgraph's RDF parser table are parsed
    Then each gives Dgraph's N-Quad or Dgraph's error

  Scenario: A typed literal takes its type
    When the RDF line `<a> <age> "013"^^<xs:int> .` is parsed
    Then the object value is the int 13

  Scenario: A language tag is kept
    When the RDF line `<a> <name> "chat"@fr .` is parsed
    Then the N-Quad's language is `fr`

  Scenario: The star form deletes all predicates
    When the RDF line `<alice> * * .` is parsed
    Then the predicate and the object value are `_STAR_ALL`

  Scenario: A missing object is refused
    When the RDF line `<alice> <knows> .` is parsed
    Then parsing fails

  Scenario: Nested JSON becomes edges
    When the JSON `{"uid": "_:a", "name": "A", "friend": {"uid": "_:b", "name": "B"}}` is parsed
    Then the result holds `name` for both nodes and a `friend` edge from `_:a` to `_:b`

  Scenario: A facet travels with its predicate
    When the JSON `{"uid": "_:a", "friend": {"uid": "_:b", "friend|close": true}}` is parsed
    Then the `friend` edge carries the facet `close` with bool value true

  Scenario: A negative uid is refused
    When the JSON `{"uid": "-100", "name": "x"}` is parsed
    Then parsing fails

  Scenario: A node without uid gets a blank node
    When the JSON `[{"name": "a"}, {"name": "b"}]` is parsed
    Then each node is assigned `_:dg.<id>.<n>`, with one random u32 `<id>` and consecutive counters `<n>` from 1 up
