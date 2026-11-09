## ADDED Requirements

### Requirement: A measure tokenizer orders by the canonical unit
The `measure` tokenizer SHALL take a `ugdb:measure` literal and the catalog's units, and emit the sortable token of the
value converted to its quantity kind's canonical unit, `value × multiplier + offset` with the unit's exact rational
multiplier and offset, as the `float` tokenizer emits a number's. Two measures of one kind SHALL order by their tokens
as their canonical values order. A measure whose unit is declared `affine: false` SHALL take the token of its own
value, comparing within its unit only.

#### Scenario: Feet and metres order together
- **WHEN** `"1000 m"` and `"2000 [ft_i]"` are tokenized under Gen5's units
- **THEN** the token of the second, 609.6 in metres, sorts before the token of the first

#### Scenario: An offset unit converts before it is tokenized
- **WHEN** `"0 Cel"` and `"273.15 K"` are tokenized under Gen5's units
- **THEN** the two tokens are equal
