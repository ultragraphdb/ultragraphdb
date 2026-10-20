Feature: Facet values
  A facet value takes the first type it parses as: an empty or quoted string, an int with Go's base prefixes, a
  float, a bool, a datetime; anything else is refused. A facet list is sorted by key and refuses a repeated key.

  Scenario: Facet types are inferred
    When the facets `since=2006-01-02T15:04:05`, `weight=0.7`, `close=true`, `note="x"` and `n=0x10` are parsed
    Then their types are datetime, float, bool, string and int
    And the facet `n` holds the int 16

  Scenario: An unquoted text facet is refused
    When the facet `note=abc` is parsed
    Then parsing fails with "Could not parse the facet value : [abc]"

  Scenario: A repeated facet key is refused
    When a facet list naming `weight` twice is sorted and validated
    Then it fails with "Repeated keys are not allowed in facets. But got weight"
