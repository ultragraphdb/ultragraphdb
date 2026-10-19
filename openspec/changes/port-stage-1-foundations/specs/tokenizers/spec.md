## ADDED Requirements

### Requirement: Tokenizer identifiers
Each index token SHALL start with its tokenizer's identifier byte, equal to Dgraph's: term `0x01`, exact `0x02`,
exact with language `0x03`, year `0x04`, geo `0x05`, int `0x06`, float `0x07`, fulltext `0x08`, bool `0x09`,
trigram `0x0A`, hash `0x0B`, sha256 `0x0C`, bigfloat `0x0D`, float32vector `0x0E`, ngram `0x0F`, month `0x41`,
day `0x42`, hour `0x43`.

#### Scenario: Every tokenizer's tokens match the golden vectors
- **WHEN** each tokenizer tokenizes its golden inputs
- **THEN** every token, identifier byte included, equals Dgraph's

### Requirement: Sortable tokens preserve order
Int, float, bigfloat and bool tokens SHALL be a sign byte (0 for negative, 1 otherwise) followed by the integer
value as u64 big-endian; year, month, day and hour tokens SHALL be big-endian u16 fields in UTC; so that byte
order equals value order.

#### Scenario: Integer tokens order like their values
- **WHEN** the ints MinInt64, -1, 0, 1 and MaxInt64 are tokenized
- **THEN** the tokens' byte order is the values' order

#### Scenario: Hour tokens hold UTC fields
- **WHEN** `2017-01-01T12:12:12+01:00` is tokenized by the hour tokenizer
- **THEN** the token encodes year 2017, month 1, day 1 and hour 11

### Requirement: Text tokenizers follow Dgraph's analysis
The term tokenizer SHALL split on Unicode word boundaries, lowercase, normalize to NFKC, sort and deduplicate;
fulltext SHALL also remove the language's stop words and stem with Dgraph's stemmer for the language; trigram
SHALL emit case-preserving three-character windows; CJK languages SHALL produce bigrams.

#### Scenario: Term tokens are lowercased and deduplicated
- **WHEN** `Tokenizer works works!` is tokenized by the term tokenizer
- **THEN** the tokens are `tokenizer` and `works`, each prefixed with `0x01`

#### Scenario: English fulltext stems and drops stop words
- **WHEN** `Stemming works!` is tokenized by the fulltext tokenizer for `en`
- **THEN** the tokens are `stem` and `work`

#### Scenario: Trigram tokens preserve case
- **WHEN** `Dgraph rocks!` is tokenized by the trigram tokenizer
- **THEN** the result is Dgraph's 11 trigrams, sorted

#### Scenario: Chinese fulltext produces bigrams
- **WHEN** the golden Chinese sentence is tokenized by the fulltext tokenizer for `zh`
- **THEN** the tokens equal Dgraph's bigrams

#### Scenario: Stemmed languages match Dgraph per language
- **WHEN** the golden sentences for every language with a stemmer are tokenized by fulltext
- **THEN** each language's tokens equal Dgraph's

### Requirement: Hash and ngram tokens
The hash tokenizer SHALL emit the BLAKE2b-256 digest of the value; the ngram tokenizer SHALL emit shingles of up
to four words after stop-word removal and stemming, replacing any shingle of 30 bytes or more with its
BLAKE2b-256 digest.

#### Scenario: A long shingle is hashed
- **WHEN** a sentence yielding a shingle of 30 or more bytes is tokenized by ngram
- **THEN** that shingle appears as its BLAKE2b-256 digest and shorter shingles appear as text

### Requirement: Language tags reduce to their base
A language tag SHALL reduce to its base language, and an unparsable tag SHALL reduce to `en`.

#### Scenario: Language tags reduce
- **WHEN** `en-US`, `zh-Hant` and `x!` are reduced
- **THEN** the results are `en`, `zh` and `en`

### Requirement: Exact tokens with a language sort by collation
An exact token with a language SHALL be the language base, the delimiter `0x1F`, and a collation key, so that
token order equals the language's collation order.

#### Scenario: Collated exact tokens order like the language
- **WHEN** the golden German strings are tokenized by the exact tokenizer with language `de`
- **THEN** the tokens' byte order equals Dgraph's collation order of the strings

### Requirement: Geo tokens index S2 cells
A geo token SHALL be `p/` with the S2 token of each parent cell from level 5 to 16, or `c/` with the token of
each cell of the value's cover of at most 18 cells.

#### Scenario: A point's geo tokens match Dgraph
- **WHEN** the golden points and polygons are tokenized by the geo tokenizer
- **THEN** the tokens equal Dgraph's

### Requirement: Tokenizer registry
Tokenizers SHALL be looked up by name, SHALL report whether they are sortable, lossy and valid for a value
type, and the registry SHALL hold the vector index factory `hnsw` with its options (`metric` one of
`euclidean`, `cosine`, `dotproduct`; `exponent`, `maxLevels`, `efConstruction`, `efSearch` as integers).

#### Scenario: A tokenizer is refused for the wrong type
- **WHEN** the geo tokenizer is checked against the bool type
- **THEN** the registry reports it as not valid

#### Scenario: hnsw options are validated
- **WHEN** the hnsw factory is given `metric: "manhattan"`
- **THEN** the options are refused and the allowed metrics are named
