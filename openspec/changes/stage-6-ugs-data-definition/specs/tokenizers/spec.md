## MODIFIED Requirements

### Requirement: Geo tokens index S2 cells
A geo value SHALL yield `p/` tokens for its parent cells and `c/` tokens for its cover: for a point, its cells at
levels 5 to 16 and its level-16 cell; for a polygon or multipolygon, an S2 cover of each outer ring at levels 5 to 16
with at most 18 cells, and every ancestor of each cover cell down to level 5. for a line string, an S2 cover of the polyline
at levels 5 to 16 with at most 18 cells, and every ancestor of each cover cell down to level 5. Other geometries SHALL
be refused with `Cannot index geometry of type <T>`.

#### Scenario: A point's tokens name its cells
- **WHEN** a point is tokenized by the geo tokenizer
- **THEN** it yields a `p/` token for its cell at each level from 5 to 16 and one `c/` token, its level-16 cell

#### Scenario: A polygon's cover holds its outer ring
- **WHEN** a polygon is tokenized by the geo tokenizer
- **THEN** it yields at most 18 `c/` tokens whose cells together contain its outer ring, and a `p/` token for each
  ancestor of those cells down to level 5

#### Scenario: A line string's cover holds the line
- **WHEN** a line string of three points is tokenized by the geo tokenizer
- **THEN** it yields at most 18 `c/` tokens whose cells together contain every point of the line, and a `p/` token for
  each ancestor of those cells down to level 5

## ADDED Requirements

### Requirement: A measure tokenizer orders by the canonical unit
The `measure` tokenizer SHALL take a `ugdb:measure` literal and the catalog's units, and emit its identifier byte
followed by the `Float64` comparison form (`STORAGE.md` sections 5.2 and 5.3) of the value converted to its quantity
kind's canonical unit, `value × multiplier + offset` with the unit's exact rational multiplier and offset, exact: never
truncated to an integer. The tokens of two measures of one kind SHALL compare as bytes as their canonical values
compare, and be equal exactly where those values are, so a range filter scans its tokens (`SCHEMA.md` section 6.10).
Its identifier byte SHALL be the registry's, and the registry SHALL report it sortable, not lossy, and valid for
`ugdb:measure`. A measure whose unit is declared `affine: false` SHALL take the token of its own value, comparing within
its unit only.

#### Scenario: Feet and metres order together
- **WHEN** `"1000 m"` and `"2000 [ft_i]"` are tokenized under Gen5's units
- **THEN** the token of the second, 609.6 in metres, sorts before the token of the first

#### Scenario: An offset unit converts before it is tokenized
- **WHEN** `"0 Cel"` and `"273.15 K"` are tokenized under Gen5's units
- **THEN** the two tokens are equal

#### Scenario: Measures within one integer keep their order
- **WHEN** `"609.2 m"` and `"609.6 m"` are tokenized
- **THEN** the two tokens differ, the first sorts before the second, and a range filter above 609.4 m keeps the second
  alone

### Requirement: A decimal tokenizer orders by the exact value
The `decimal` tokenizer SHALL index a `Decimal(p, s)` value under one token: its identifier byte, the registry's,
followed by the value's sortable encoding (`STORAGE.md` section 5.2, tag 13), the value scaled by 10^s as a signed
128-bit integer, big-endian with its sign bit inverted, so tokens compare as bytes exactly as the values compare and a
range filter over a `Decimal` attribute scans its tokens without converting a value to a float.

#### Scenario: Decimal tokens order exactly
- **WHEN** an attribute of type `Decimal(18, 2)` with `@index(range)` holds `-0.01`, `0.00`, `0.10` and `49000000.00`
- **THEN** their tokens, compared as bytes, order as the values, and a range filter `> 0.00` finds `0.10` and
  `49000000.00`

#### Scenario: Two values differing past a float's precision keep their order
- **WHEN** the same attribute holds `9007199254740993.00` and `9007199254740992.00`
- **THEN** their tokens differ and order as the values
