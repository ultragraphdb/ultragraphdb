#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Quad key encoding
  Every quad key is its keyspace byte, the namespace as a u64 big-endian and the fields its keyspace defines, graphs,
  predicates and subjects as UIDs and objects as their encodings. A key parses back into its parts, a split key
  addresses one part of a split list, and a truncated key is refused without a panic.

  Scenario: A forward key round-trips
    When a forward key for namespace 133, the default graph, predicate UID 7 and subject UID 42 is parsed
    Then the result is a forward key with namespace 133, graph 0, predicate 7 and subject 42

  Scenario: A subject UID of zero is refused
    When a forward key carrying subject UID 0 is parsed
    Then parsing fails with an error naming the key's bytes

  Scenario: A literal object round-trips
    When a reverse key whose object is the encoding of "Ann"@en is built and parsed
    Then the parsed object decodes to "Ann"@en

  Scenario: A split key round-trips
    When the split key for a forward key and start 1000 is built and parsed
    Then the parse returns the base key's parts and start 1000

  Scenario: A split key with a trailing byte is refused
    When one byte is appended to a valid split forward key and the result is parsed
    Then parsing fails

  Scenario: Every prefix of a key fails cleanly
    When every proper prefix of a forward, reverse, value-index and statement key is parsed
    Then each parse fails with an error, and nothing panics
