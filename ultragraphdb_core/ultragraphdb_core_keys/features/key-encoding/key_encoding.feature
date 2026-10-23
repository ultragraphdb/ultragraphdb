#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

Feature: Key encoding
  Keys follow Dgraph's byte layout and parse back into their parts; split keys address the parts of a split posting
  list; attributes held in memory carry their namespace; names under `dgraph.` are reserved, some pre-defined, and
  plugins may claim them through the reserved-namespace registry.

  Scenario: A data key matches Dgraph's bytes
    When the data key for attribute "name" in namespace 0 and uid 1 is built
    Then it equals the bytes "00 00 00 00 00 00 00 00 00 00 04 6E 61 6D 65 00 00 00 00 00 00 00 00 01"

  Scenario: Every key kind matches its golden vector
    When data, index, reverse, count, reverse-count, schema and type keys are built for the golden inputs
    Then each equals the bytes Dgraph produces for the same input

  Scenario: Data keys of one predicate sort by uid
    When data keys for the attribute "name" are built for uids 1, 2, 256 and 1099511627776
    Then their byte order equals their numeric uid order

  Scenario: A data key round-trips
    When a data key for namespace 133, attribute "name" and uid 42 is parsed
    Then the result is a data key with namespace 133, attribute "name" and uid 42

  Scenario: A uid of zero is rejected
    When a data key for attribute "name" carrying uid 0 is parsed
    Then parsing fails with "Invalid UID with value 0 for key:" followed by the key after its namespace

  Scenario: An attribute containing a hyphen round-trips
    When a data key for attribute "first-name" in namespace 7 is built and parsed
    Then the parsed attribute is "first-name" and the namespace is 7

  Scenario: A split key round-trips
    When the split key for the data key of attribute "name", uid 9 and start uid 1000 is built and parsed
    Then the parse is a data key with attribute "name", uid 9 and start uid 1000

  Scenario: A split data key with a trailing byte is rejected
    When one byte is appended to the split key for the data key of attribute "name", uid 9 and start uid 1000
    Then parsing it fails

  Scenario: A namespaced attribute is formatted in hexadecimal
    When namespace 133 and predicate "name" are combined
    Then the result is "85-name", and parsing it returns namespace 133 and predicate "name"

  Scenario: A reserved name need not be pre-defined
    When "0-dgraph.blah" and "0-dgraph.type" are checked
    Then both are reserved, and only "0-dgraph.type" is pre-defined

  Scenario: The longest prefix owns a name
    When plugins register the value-locked prefixes "dgraph.acme." and "dgraph.acme.billing."
    Then "dgraph.acme.billing.total" belongs to the second and "dgraph.acme.users" to the first

  Scenario: Registering a name twice is refused
    When two plugins register the exact name "dgraph.acme.key"
    Then the second registration is refused with 'x.RegisterReservedNamespace: predicate "dgraph.acme.key" already registered'

  Scenario: A predicate prefix does not parse as a key
    When the predicate prefix of "aa" in namespace 0 is parsed as a key
    Then parsing fails with "Invalid format for key" and nothing panics
