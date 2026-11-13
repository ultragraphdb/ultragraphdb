# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Scalar types
  Dgraph's scalar values: type identifiers, binary encodings, conversions, big floats, datetimes, value inference,
  passwords and sort order (openspec/specs/scalar-types/spec.md).

  Scenario: Schema type names map to identifiers
    When the names "int", "datetime", "uid", "decimal" and "float32vector" are looked up
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

  Scenario: Two to the 63 is not an int
    When the float 9223372036854775808 is converted to int
    Then conversion fails with "Float out of int64 range"

  Scenario: A float beyond int64 seconds is not a datetime
    When the floats "1e19", "+Inf" and "NaN" and the decimal "9223372036854775808" are each converted to datetime
    Then each conversion fails with "Float out of int64 range"

  Scenario: Bytes that are not UTF-8 do not become a string
    When the binary value with the bytes "e697ff" is converted to string
    Then conversion fails with "Value is not valid UTF-8 for type string"

  Scenario: Valid UTF-8 bytes become a string
    When the binary value of "日本" is converted to string
    Then the result is "日本"

  Scenario: Not a number is refused as a float
    When the string "NaN" is converted to float
    Then conversion fails with "Got invalid value: NaN"

  Scenario: Binary decodes to bool only from zero or one
    When the bytes [], [0], [1] and [2] are converted to bool
    Then the results are false, false and true, and [2] fails

  Scenario: Floats format as Go's shortest G form
    When the float 2.5e-7 is converted to string
    Then the result is "2.5E-07"

  Scenario: A decimal parses its XSD form
    When "1000", "-0.5", "1.50" and "1e3" are parsed as decimal
    Then the first three give "1000", "-0.5" and "1.5", and "1e3" is refused

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
    When the strings "zebra", "ä", "b" and "Apfel" are sorted with language "de"
    Then the order is "ä", "Apfel", "b", "zebra"
