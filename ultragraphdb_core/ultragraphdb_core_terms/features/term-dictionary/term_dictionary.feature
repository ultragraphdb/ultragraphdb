#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Literals encoded by content
  A literal is encoded as one tag byte and at most 32 bytes, with no allocation and no read: by value when its lexical
  form is canonical, by datatype and lexical form otherwise, short strings inline and long ones as a 128-bit hash that
  names a dictionary entry. A vector of the system datatype is the one exception, encoded by value at any length.

  Scenario: A canonical integer is encoded by value
    When "42"^^xsd:integer is encoded
    Then its encoding holds the value 42 and decodes to "42"^^xsd:integer

  Scenario: A non-canonical integer keeps its lexical form
    When "042"^^xsd:integer and "42"^^xsd:integer are encoded
    Then their encodings differ, and the first decodes to "042"^^xsd:integer

  Scenario: A long string is hashed
    When a 40-byte string literal is encoded
    Then its encoding names a 128-bit hash, and decoding through the dictionary returns the string

  Scenario: A short language-tagged string is inline
    When "chat"@fr is encoded
    Then its encoding holds chat and fr inline, and decodes to "chat"@fr

  Scenario: A vector is encoded by value
    When a 768-element ugdb:float32vector literal is written
    Then its encoding is the tag byte, the dimension 768 and 3,072 bytes of elements, and the dictionary gains no entry

  Scenario: A canonical vector round-trips
    When "[0.5, -1, 2.25]"^^ugdb:float32vector is written and read back
    Then the read returns "[0.5, -1, 2.25]"^^ugdb:float32vector
