# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Scalar types
  Dgraph's scalar values: type identifiers, binary encodings, conversions, big floats, datetimes, value inference,
  passwords and sort order (openspec/specs/scalar-types/spec.md). The facet
  scenarios belong to ultragraphdb_core_facets.

  Scenario: Schema type names map to identifiers
    When the names "int", "datetime", "uid", "bigfloat" and "float32vector" are looked up
    Then they map to identifiers 2, 5, 7, 11 and 12

  Scenario: Every type's encoding matches its golden vector
    When the golden values of each type are converted to binary
    Then each result equals Dgraph's bytes

  Scenario: A UTC datetime uses version 1 of Go's layout
    When "2018-10-03T08:05:01Z" is converted to binary
    Then the result is 15 bytes starting with 0x01 and ending in the offset FF FF

  Scenario: Binary decodes back to the value
    When each golden binary value is converted back to its type
    Then the result equals the original value

  Scenario: A float beyond int64 is refused
    When the float 1e20 is converted to int
    Then conversion fails with "Float out of int64 range"

  Scenario: Not a number is refused as a float
    When the string "NaN" is converted to float
    Then conversion fails with "Got invalid value: NaN"

  Scenario: Binary decodes to bool only from zero or one
    When the bytes [], [0], [1] and [2] are converted to bool
    Then the results are false, false and true, and [2] fails

  Scenario: Floats format as Go's shortest G form
    When the float 2.5e-7 is converted to string
    Then the result is "2.5E-07"

  Scenario: Big-float golden vectors match Go
    When the golden big-float inputs are parsed and formatted: hand-picked, random, exact midpoints between adjacent 200-bit values, and powers of two
    Then every 200-bit value equals Go's and every text equals Go's MarshalText

  Scenario: A power of two formats as Go shortens it
    When 2^-1100 at 200 bits is formatted
    Then the text is "7.3621518290228626754368661771449651176491350325096363488672e-332"

  Scenario: Go's big-float syntax is accepted
    When "1_000", "0x1.8p1", "Inf" and "NaN" are parsed as bigfloat
    Then the first three give "1000", "3" and "+Inf", and "NaN" is refused

  Scenario: Partial dates parse
    When "2018", "2018-10" and "2018-10-03" are parsed
    Then each yields midnight UTC on the first day of the period it names

  Scenario: An out-of-range zone offset is refused
    When "2018-10-03T08:05:01+25:00" is parsed
    Then parsing fails with "time zone offset hour out of range"

  Scenario: Untyped values infer their type
    When "true", "TRUE", "t", "2018", "2018-10", "1World" and "2.99792e8" are inferred
    Then the types are bool, bool, default, int, datetime, default and float

  Scenario: A short password is refused
    When the string "abc" is converted to password
    Then conversion fails with "Password too short, i.e. should have at least 6 chars"

  Scenario: A Dgraph-written hash verifies
    When a golden password and its Go-generated bcrypt hash are checked
    Then the check succeeds, and a different password fails it

  Scenario: Ints and floats sort together
    When the values 3, 2.5 and -1 of mixed int and float types are sorted
    Then the order is -1, 2.5, 3

  Scenario: Language strings sort by collation
    Given the golden German strings
    When the golden German strings are sorted with language "de"
    Then the order equals Dgraph's
