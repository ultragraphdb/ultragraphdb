#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: DQL parsing
  A DQL query parses into Dgraph's query tree, with its variables, typed parameters, language lists and fragments
  checked as Dgraph checks them; mutation and upsert blocks parse into their N-Quad text and conditions; an N-Quad
  converts into a directed edge.

  Scenario: Parsed queries match Dgraph's trees
    When each query of the golden DQL corpus is parsed
    Then the tree, serialized as JSON, equals the tree Dgraph's parser produced

  Scenario: Dgraph's parser tests pass
    When the ported cases of Dgraph's dql/parser_test.go run
    Then every case gives Dgraph's result or Dgraph's error text

  Scenario: An undefined variable is refused
    When the query "{ me(func: uid(aliceInWonderland)) { name } }" is parsed
    Then parsing fails with "Some variables are used but not defined"
    And the error names "aliceInWonderland"

  Scenario: Count of a value variable is refused
    When the query "{ me(func: uid(1)) { Upvote { u as Author } count(val(u)) } }" is parsed
    Then parsing fails with "Count of a variable is not allowed"

  Scenario: uid of a variable as a child is refused
    When the query "{ var(func: uid(0x0a)) { a as friends } me(func: uid(a)) { uid(a) } }" is parsed
    Then parsing fails with "Cannot do uid() of a variable"

  Scenario: An int parameter takes an int
    When the query "query test($a: int) { q(func: uid(0x1)) { x as count(uid) p : math(x + $a) } }" is parsed with $a = "3"
    Then the parameter in the math expression is the int 3

  Scenario: An int parameter refuses a float
    When the query "query test($a: int) { q(func: uid(0x1)) { x as count(uid) p : math(x + $a) } }" is parsed with $a = "3.3"
    Then parsing fails with "Expected an int but got 3.3"

  Scenario: A language list parses in order
    When the query "{ me(func: uid(0x1)) { name@en:ru:hu } }" is parsed
    Then the languages of the predicate "name" are "en", "ru", "hu" in that order

  Scenario: Two language lists are refused
    When the query "{ me(func: uid(0x1)) { name@en@ru } }" is parsed
    Then parsing fails with "Expected directive or language list, got @ru"

  Scenario: A fragment cycle is refused
    When the query "{ me(func: uid(0x1)) { ...a } } fragment a { name ...b } fragment b { age ...a }" is parsed
    Then parsing fails with "Cycle detected"

  Scenario: An empty upsert block is refused
    When the request "upsert {}" is parsed as DQL
    Then parsing fails with "Empty mutation block"

  Scenario: An unclosed upsert block is refused
    When the request "upsert {" is parsed as DQL
    Then parsing fails with "Unclosed upsert block"

  Scenario: A condition spanning lines parses
    When an upsert block whose mutation carries a condition split across lines is parsed as DQL
    Then the mutation's condition is the text "@if(eq(len(m), 1)\n        AND\n        gt(len(f), 0))" as written

  Scenario: The mutation keyword is refused
    When the request 'mutation { set { _:a <name> "x" . } }' is parsed as a mutation
    Then parsing fails with "Unexpected token: [mutation]"

  Scenario: A value N-Quad becomes a value edge
    When the N-Quad <0x1> <name> "A"@en . is converted into a directed edge
    Then the edge has entity 1, attribute "name", value bytes "A" of type default, language "en" and operation SET

  Scenario: An N-Quad without an object is refused as a value edge
    When the N-Quad with subject "0x1", predicate "name" and no object becomes an edge
    Then the conversion fails with "Unknown value type for nquad" and nothing panics

  Scenario: A predicate-deletion edge carries the star value
    When the deletion edge of predicate "name" in namespace 0 is built
    Then its value is "_STAR_ALL", its type is string and its operation is DEL
