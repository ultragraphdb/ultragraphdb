# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: System identity
  The 128-bit system ID, its bucket and the canonical key encoding
  (openspec/changes/stage-5-5-three-role-refactor/specs/system-identity/spec.md): the scenarios of the requirements
  "A system ID is 128 bits of type, sequence, bucket and reserved bits", "The bucket comes from the namespace and the
  key" and "Until UGS keys exist, an IRI is the key of the Resource type", verbatim.

  Rule: A system ID is 128 bits of type, sequence, bucket and reserved bits

    Scenario: The fields pack and unpack
      When an ID is built from type prefix 3, bucket 0xAB_CDEF and sequence 42
      Then its 16 big-endian bytes are `00 03`, `00 00 00 00 00 00 00 2A`, `AB CD EF` and `00 00 00`, and unpacking returns type 3, bucket 0xAB_CDEF and sequence 42

    Scenario: Nonzero reserved bits are refused
      When 16 bytes whose last byte is 0x01 are decoded as an ID
      Then decoding fails with an invalid-ID error naming the reserved bits

    Scenario: A zero type prefix or sequence is refused
      When an ID with type prefix 0, and another with sequence 0, are decoded
      Then each fails with an invalid-ID error naming the field

    Scenario: IDs of one type sort together
      When the IDs (type 2, bucket 9, sequence 1), (type 1, bucket 900, sequence 7) and (type 1, bucket 3, sequence 99) are sorted by their bytes
      Then the order is (1, 900, 7), (1, 3, 99), (2, 9, 1): type first, then sequence

  Rule: The bucket comes from the namespace and the key

    Scenario: The bucket matches the XXH3 reference
      When the bucket of the IRI `http://ex.org/a` in namespace 0 is computed
      Then its top 8 bits equal the top byte of the xxHash reference implementation's XXH3-64 of eight zero bytes, and its low 16 bits equal the top two bytes of the reference XXH3-64 of the `Resource` prefix's 2 bytes followed by the IRI's canonical encoding, both values committed as golden values with their source

    Scenario: A statement shares its subject's bucket
      When a statement with subject `http://ex.org/a` receives a system ID
      Then the statement's bucket equals the bucket of `http://ex.org/a`

    Scenario: Two namespaces separate one key
      When the IRI `http://ex.org/a` is bucketed in namespaces 0 and 1
      Then the two buckets' low 16 bits are equal, and their top 8 bits are the reference values for each namespace

    Scenario: A unique value's bucket is computed from its predicate and value
      When the bucket of the value `"x"` of the unique predicate `epi.id` is computed in namespace 0
      Then its top 8 bits equal the namespace's reference value, and its low 16 bits equal the top two bytes of the reference XXH3-64 of `epi.id`'s 16 ID bytes followed by `"x"`'s term encoding

  Rule: Until UGS keys exist, an IRI is the key of the Resource type

    Scenario: The canonical encoding of an IRI
      When the IRI `http://ex.org/a` is canonically encoded
      Then the bytes are `00 00 00 0F` followed by the 15 UTF-8 bytes of `http://ex.org/a`

    Scenario: The reserved prefixes
      When a new IRI and a new statement receive IDs
      Then the IRI's type prefix is 1 and the statement's is 2
