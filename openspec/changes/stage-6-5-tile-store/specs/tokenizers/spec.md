## ADDED Requirements

### Requirement: A datetime tokenizer orders temporal values by their instant
The `datetime` tokenizer SHALL index a `Date`, `Time`, `DateTime` or `LocalDateTime` value under one token: its
identifier byte followed by the value's comparison form, the first 16 bytes of its sortable encoding, the instant
(`STORAGE.md` sections 5.2 and 5.3), so tokens compare as bytes as the instants compare and one instant written with two
offsets is one token (`SCHEMA.md` sections 5.5 and 6.10; T48, C25, C28). The registry SHALL report it sortable, not
lossy, and valid for the four temporal types. It takes the place of `hour` and `day`, which the registry does not hold.

#### Scenario: One instant with two offsets is one token
- **WHEN** a `DateTime` attribute with `@index(range)` holds `2017-01-01T12:12:12+01:00` and `2017-01-01T11:12:12Z`
- **THEN** both values tokenize to one token, and each stored value keeps its own offset

#### Scenario: Temporal tokens order by instant
- **WHEN** `2017-01-01T12:12:12+01:00` and `2017-01-01T11:30:00Z` are tokenized by the `datetime` tokenizer
- **THEN** the first token sorts before the second, as 11:12:12 UTC precedes 11:30:00 UTC

#### Scenario: Hour and day are not tokenizers
- **WHEN** the registry is asked for `hour`, `day` and `datetime` by name
- **THEN** it finds `datetime` alone

## MODIFIED Requirements

### Requirement: Tokenizer identifiers
Each index token SHALL start with its tokenizer's identifier byte. The registry SHALL assign the identifier bytes in its
own order, one per tokenizer and distinct, none 0 and none `0xFF`, the byte that marks a hashed token (`STORAGE.md`
section 5.3; `PROTOCOL.md` sections 13.13 and 13.21). An index family's tile key, and a Scan's or Frontier's
`tokenizer`, name its tokenizer by that byte (`STORAGE.md` section 13.1). Tokens SHALL be checked against vectors
written from `STORAGE.md` section 5.3 and this capability's rules.

#### Scenario: Every tokenizer's identifier byte is its own
- **WHEN** the registry lists its tokenizers and each tokenizes a value it is valid for
- **THEN** no two tokenizers share an identifier byte, none is 0 or `0xFF`, and each token begins with its tokenizer's
  byte, or is a hashed token beginning with `0xFF`

### Requirement: Sortable tokens preserve order
A token of a `range` or `interval` index over a declared attribute SHALL be the tokenizer's identifier byte
followed by the comparison form of the value's sortable encoding (`STORAGE.md` sections 5.2 and 5.3), exact, so tokens
compare as bytes as the values compare and a range of values is a range of tokens; the tokenizer an attribute's type
takes is `SCHEMA.md` section 6.10's. In the generic layout a literal's datatype SHALL pick the sortable type of its
range token (`STORAGE.md` section 5.3; T48): `xsd:integer` and the integer types derived from it `Int64`, a value
outside `Int64`'s range taking no range token; `xsd:double` and `xsd:float` `Float64`, the lexical form parsed as a
double, rounded once to the nearest, ties to even, never through binary32; `xsd:decimal` `Int128`, the value × 10^18,
exact, a value with more than 18 fraction digits or outside `Int128` at that scale taking no range token; `xsd:dateTime` and `xsd:date` the 16-byte comparison form, the UTC instant; `xsd:boolean`
`Bool`. Each sortable type SHALL take its own tokenizer and so its own identifier byte, `int` for `Int64`, `float`
for `Float64`, `decimal` for the `Int128`, `datetime` for the instant and `bool` for `Bool`, and the token SHALL be that
tokenizer's identifier byte followed by the comparison form, so values of two sortable types never share an index
family or a token. A `count` index's token SHALL be the
identifier byte, the direction as a u8, 1 a subject's count and 2 an object's, then the count as a big-endian `UInt64`,
a count of 0 taking its token like any other (T48). Any other token SHALL be the identifier byte and the token bytes this capability
gives: year and month tokens big-endian u16 fields in UTC. A token of more than 256 bytes, its identifier byte included,
SHALL be stored as the byte `0xFF` followed by the XXH3-128 of all its bytes as 16 big-endian bytes, 17 bytes in all,
and is lossy: a reader SHALL confirm every match against the value it indexes.

#### Scenario: Integer tokens order like their values
- **WHEN** an `Int64` attribute's values MinInt64, -1, 0, 1 and MaxInt64 are tokenized by the `int` tokenizer
- **THEN** each token is the identifier byte and the value's 8 bytes, big-endian with the sign bit inverted, and the
  tokens' byte order is the values' order

#### Scenario: Float tokens are exact
- **WHEN** a `Float64` attribute's values -2.7, -2.5, -0.0, 0.0, 2.5 and 2.7 are tokenized by the `float` tokenizer
- **THEN** -0.0 and 0.0 give one token, every other pair of values gives two, and the tokens' byte order is the values'
  order

#### Scenario: A generic literal's datatype picks its token
- **WHEN** a tenant with no schema holds `"7"^^xsd:short`, `"2.5"^^xsd:float`, `"0.1"^^xsd:decimal`,
  `"2017-01-01"^^xsd:date` and `"99999999999999999999"^^xsd:integer` under range-indexed predicates
- **THEN** the first is tokenized as `Int64` 7, the second as `Float64` 2.5, the third as `Int128` 10^17, the fourth as
  the instant 2017-01-01T00:00:00Z, and the fifth, outside `Int64`'s range, takes no range token; the first four take
  the identifier bytes of `int`, `float`, `decimal` and `datetime`, each a family of its own

#### Scenario: Values of two sortable types never share a token
- **WHEN** `"0"^^xsd:decimal` and `"0001-01-01T00:00:00Z"^^xsd:dateTime` are tokenized under range-indexed predicates
- **THEN** their tokens begin with the `decimal` and the `datetime` identifier bytes respectively and differ, though
  both comparison forms are 16 bytes

#### Scenario: A float is parsed as a double, and a decimal past 18 fraction digits takes no token
- **WHEN** `"0.1"^^xsd:float`, `"0.1"^^xsd:double` and `"0.0000000000000000001"^^xsd:decimal` are tokenized under
  range-indexed predicates
- **THEN** the first two give one token, the double nearest 0.1, and the third, with 19 fraction digits, takes no range
  token

#### Scenario: A count of zero takes its token
- **WHEN** a subject with no `knows` statement is counted by the `count` index
- **THEN** its token is the identifier byte, then 1, then 8 zero bytes, and it sorts before every nonzero count of the
  same direction

#### Scenario: A count token holds its direction
- **WHEN** a subject's `knows` count of 3 and an object's `knows` count of 3 are tokenized by the `count` index
- **THEN** the tokens are the identifier byte, then 1 and 2 respectively, then 3 as 8 big-endian bytes

#### Scenario: A long token is hashed
- **WHEN** a `String` attribute with `@index(range)` holds a value of 300 bytes
- **THEN** its token is the byte `0xFF` followed by the XXH3-128 of the identifier byte and the value's bytes, 17 bytes in
  all, and a read matching it confirms the match against the value

### Requirement: Text tokenizers follow Dgraph's analysis
The term tokenizer SHALL split on word boundaries as bleve's Unicode 8.0 segmenter does, lowercase, normalize to NFKC,
sort and deduplicate, except for `zh`, `ja`, `th`, `lo`, `my`, `bo`, `km` and `kxm`, whose text it SHALL split on
spaces only. Fulltext SHALL also remove the language's stop words and stem with Dgraph's stemmer for the language, and
SHALL produce bigrams for `zh`, `ja` and `ko`. Trigram SHALL emit three-byte windows, sorted and deduplicated.

#### Scenario: Term tokens are lowercased and deduplicated
- **WHEN** `Tokenizer works works!` is tokenized by the term tokenizer
- **THEN** the tokens are `tokenizer` and `works`, each prefixed with the term tokenizer's identifier byte

#### Scenario: Chinese term tokens split on spaces
- **WHEN** `他是 一个好人` is tokenized by the term tokenizer for `zh`
- **THEN** the tokens are `他是` and `一个好人`

#### Scenario: English fulltext stems and drops stop words
- **WHEN** `Stemming works!` is tokenized by the fulltext tokenizer for `en`
- **THEN** the tokens are `stem` and `work`

#### Scenario: Trigram tokens preserve case
- **WHEN** `Dgraph rocks!` is tokenized by the trigram tokenizer
- **THEN** the result is its 11 three-byte windows, `Dgr` among them, sorted

#### Scenario: Trigrams are byte windows
- **WHEN** `Grüße` is tokenized by the trigram tokenizer
- **THEN** the tokens are its three-byte windows, `Gr\xc3` among them

#### Scenario: Chinese fulltext produces bigrams
- **WHEN** a sentence of Han characters alone is tokenized by the fulltext tokenizer for `zh`
- **THEN** every token is two adjacent characters of the sentence, and every two adjacent characters are a token

#### Scenario: Stemmed languages stem per language
- **WHEN** `running` tagged `en` and `Katzen` tagged `de` are tokenized by fulltext
- **THEN** the tokens are `run` and `katz`

### Requirement: Exact tokens with a language sort by collation
An exact token with a language, of a value no declared attribute holds, SHALL be the language base, the delimiter `0x1F`,
and the collation key Go's `x/text/collate` produces, so that token order equals the language's collation order; a
declared attribute's `exact` token is its comparison form ("Sortable tokens preserve order").

#### Scenario: Collated exact tokens order like the language
- **WHEN** `Bär`, `Äpfel` and `Apfel` are tokenized by the exact tokenizer with language `de`
- **THEN** the tokens order `Apfel`, `Äpfel`, `Bär`, as German collation orders them, though `Äpfel`'s UTF-8 bytes sort
  after `Bär`'s

### Requirement: Geo tokens index S2 cells
A geo value SHALL yield `p/` tokens for its parent cells and `c/` tokens for its cover: for a point, its cells at
levels 5 to 16 and its level-16 cell; for a polygon or multipolygon, an S2 cover of each outer ring at levels 5 to 16
with at most 18 cells, and every ancestor of each cover cell down to level 5. Other geometries SHALL be refused with
`Cannot index geometry of type <T>`.

#### Scenario: A point's tokens name its cells
- **WHEN** a point is tokenized by the geo tokenizer
- **THEN** it yields a `p/` token for its cell at each level from 5 to 16 and one `c/` token, its level-16 cell

#### Scenario: A polygon's cover holds its outer ring
- **WHEN** a polygon is tokenized by the geo tokenizer
- **THEN** it yields at most 18 `c/` tokens whose cells together contain its outer ring, and a `p/` token for each
  ancestor of those cells down to level 5

### Requirement: Tokenizer registry
Tokenizers SHALL be looked up by name and SHALL report whether they are sortable, lossy and valid for a value type. The
registry SHALL hold no vector index factory: `hnsw` leaves with the vector index and returns in stage 8 (vector-index;
T48).

#### Scenario: A tokenizer is refused for the wrong type
- **WHEN** the geo tokenizer is checked against the bool type
- **THEN** the registry reports it as not valid

#### Scenario: The registry holds no vector index factory
- **WHEN** the registry is asked for `hnsw` by name
- **THEN** it finds none
