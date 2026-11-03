# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Quad key encoding
  The quad layout's keys over 128-bit system IDs
  (openspec/changes/stage-5-5-three-role-refactor/specs/key-encoding/spec.md): the scenarios of the requirements "Keys
  parse back into their parts", "Split keys address the parts of a split posting list" and "A truncated key is
  refused", verbatim.

  Rule: Keys parse back into their parts

    Scenario: A forward key round-trips
      When a forward key for namespace 133, the default graph, the predicate with ID (type 1, bucket 7, sequence 7) and the subject with ID (type 1, bucket 42, sequence 42) is parsed
      Then the result is a forward key with namespace 133, the default graph, and those predicate and subject IDs

    Scenario: An invalid subject ID is refused
      When a forward key carrying a subject ID whose sequence is 0 is parsed
      Then parsing fails with an error naming the key's bytes

    Scenario: A literal object round-trips
      When a reverse key whose object is the encoding of `"Ann"@en` is built and parsed
      Then the parsed object decodes to `"Ann"@en`, and the key ends with the object's encoding

    Scenario: A mark key round-trips and stays apart from object predicates
      When a mark key for namespace 7, the entity with ID (type 1, bucket 42, sequence 9) and bucket group 0x2_ABCD is built and parsed, and the object-predicates keys of the graph with that same ID are scanned by prefix
      Then the parse returns the marks keyspace, namespace 7, that ID and group 0x2_ABCD from the bytes `02 AB CD`, and the prefix scan returns no mark key

    Scenario: A referrer key round-trips
      When a referrer key for namespace 7 and the entity with ID (type 1, bucket 42, sequence 9) is built and parsed
      Then the parse returns the referrers keyspace, namespace 7 and that entity's ID

  Rule: Split keys address the parts of a split posting list

    Scenario: A split key round-trips
      When the split key for a forward key and the start ID (type 1, bucket 5, sequence 1000) is built and parsed
      Then the parse returns the base key's parts and that start ID

    Scenario: An ID start cut by 8 bytes is refused
      When the split key of a forward key at the start ID (type 1, bucket 5, sequence 1000) loses its last 8 bytes and is parsed
      Then parsing fails with an error naming the key's bytes

    Scenario: A split key with a trailing byte is refused
      When one byte is appended to a valid split forward key and the result is parsed
      Then parsing fails

  Rule: A truncated key is refused

    Scenario: Every prefix of a key fails cleanly
      When every proper prefix of a forward, reverse, value-index and statement key is parsed
      Then each parse fails with an error, and nothing panics

    Scenario: Every prefix of a 128-bit ID field fails cleanly
      When a forward key is cut inside its subject ID, at each of the ID's 15 inner positions, and parsed
      Then each parse fails with an error naming the key's bytes
