# tokenizers Specification

## Purpose
The index tokenizers: their identifier bytes and sortable encodings, language handling, stemming, stop words and
the tokenizer registry, producing the tokens Dgraph's indexes hold.
## Requirements
### Requirement: Tokenizer identifiers
Each index token SHALL start with its tokenizer's identifier byte, equal to Dgraph's: term `0x01`, exact `0x02`, exact
with a language `0x03`, year `0x04`, geo `0x05`, int `0x06`, float `0x07`, fulltext `0x08`, bool `0x09`, trigram `0x0A`,
hash `0x0B`, sha256 `0x0C`, bigfloat `0x0D`, ngram `0x0F`, month `0x41`, day `0x42`, hour `0x43`. The identifier
`0x0E` SHALL name the vector index factory, whose one token for any value is `float`.

#### Scenario: Every tokenizer's tokens match the golden vectors
- **WHEN** each tokenizer tokenizes its golden inputs
- **THEN** every token, identifier byte included, equals Dgraph's

### Requirement: Sortable tokens preserve order
Int tokens, and float and bigfloat tokens truncated toward zero to int64, SHALL be a sign byte (0 for negative, 1
otherwise) followed by the int64 as u64 big-endian, so that byte order is non-decreasing in value; bool tokens SHALL
be the int token of 0 or 1; year, month, day and hour tokens SHALL be big-endian u16 fields in UTC.

#### Scenario: Integer tokens order like their values
- **WHEN** the ints MinInt64, -1, 0, 1 and MaxInt64 are tokenized
- **THEN** the tokens' byte order is the values' order

#### Scenario: Float tokens truncate toward zero
- **WHEN** the floats -2.7 and 2.7 are tokenized
- **THEN** their tokens equal the int tokens of -2 and 2

#### Scenario: Hour tokens hold UTC fields
- **WHEN** `2017-01-01T12:12:12+01:00` is tokenized by the hour tokenizer
- **THEN** the token encodes year 2017, month 1, day 1 and hour 11

### Requirement: Text tokenizers follow Dgraph's analysis
The term tokenizer SHALL split on word boundaries as bleve's Unicode 8.0 segmenter does, lowercase, normalize to NFKC,
sort and deduplicate, except for `zh`, `ja`, `th`, `lo`, `my`, `bo`, `km` and `kxm`, whose text it SHALL split on
spaces only. Fulltext SHALL also remove the language's stop words and stem with Dgraph's stemmer for the language, and
SHALL produce bigrams for `zh`, `ja` and `ko`. Trigram SHALL emit three-byte windows, sorted and deduplicated.

#### Scenario: Term tokens are lowercased and deduplicated
- **WHEN** `Tokenizer works works!` is tokenized by the term tokenizer
- **THEN** the tokens are `tokenizer` and `works`, each prefixed with `0x01`

#### Scenario: Chinese term tokens split on spaces
- **WHEN** `他是 一个好人` is tokenized by the term tokenizer for `zh`
- **THEN** the tokens are `他是` and `一个好人`

#### Scenario: English fulltext stems and drops stop words
- **WHEN** `Stemming works!` is tokenized by the fulltext tokenizer for `en`
- **THEN** the tokens are `stem` and `work`

#### Scenario: Trigram tokens preserve case
- **WHEN** `Dgraph rocks!` is tokenized by the trigram tokenizer
- **THEN** the result is Dgraph's 11 trigrams, sorted

#### Scenario: Trigrams are byte windows
- **WHEN** `Grüße` is tokenized by the trigram tokenizer
- **THEN** the tokens are its three-byte windows, `Gr\xc3` among them

#### Scenario: Chinese fulltext produces bigrams
- **WHEN** the golden Chinese sentence is tokenized by the fulltext tokenizer for `zh`
- **THEN** the tokens equal Dgraph's bigrams

#### Scenario: Stemmed languages match Dgraph per language
- **WHEN** the golden sentences for every language with a stemmer are tokenized by fulltext
- **THEN** each language's tokens equal Dgraph's

### Requirement: Hash and ngram tokens
The hash tokenizer SHALL emit the BLAKE2b-256 digest of the value. The ngram tokenizer SHALL emit shingles of up to four
words after stop-word removal and stemming, replacing any shingle of 30 bytes or more with its BLAKE2b-256 digest; its
query tokens SHALL be the shingles of three words, or of all words when there are fewer than three, hashed the same way.
Ngram token order SHALL be unspecified, and comparisons SHALL sort them.

#### Scenario: A long shingle is hashed
- **WHEN** a sentence yielding a shingle of 30 or more bytes is tokenized by ngram
- **THEN** that shingle appears as its BLAKE2b-256 digest and shorter shingles appear as text

#### Scenario: Short queries take one shingle
- **WHEN** the two-word query `quick fox` is tokenized for ngram queries
- **THEN** the query tokens are the one shingle of both words

### Requirement: Language tags reduce to their base
A language tag SHALL reduce to the base `golang.org/x/text/language` infers for it, and an unparsable, undefined or
no-confidence tag SHALL reduce to `en`.

#### Scenario: Language tags reduce
- **WHEN** `en-US`, `zh-Hant`, `sh`, `und` and `x!` are reduced
- **THEN** the results are `en`, `zh`, `sr`, `en` and `en`

### Requirement: Exact tokens with a language sort by collation
An exact token with a language SHALL be the language base, the delimiter `0x1F`, and the collation key Go's
`x/text/collate` produces, so that token order equals the language's collation order.

#### Scenario: Collated exact tokens order like the language
- **WHEN** the golden German strings are tokenized by the exact tokenizer with language `de`
- **THEN** the tokens equal Dgraph's, byte for byte

### Requirement: Geo tokens index S2 cells
A geo value SHALL yield `p/` tokens for its parent cells and `c/` tokens for its cover: for a point, its cells at
levels 5 to 16 and its level-16 cell; for a polygon or multipolygon, an S2 cover of each outer ring at levels 5 to 16
with at most 18 cells, and every ancestor of each cover cell down to level 5. Other geometries SHALL be refused with
`Cannot index geometry of type <T>`.

#### Scenario: Geo tokens match Dgraph
- **WHEN** the golden points and polygons are tokenized by the geo tokenizer
- **THEN** the tokens, sorted, equal Dgraph's

### Requirement: Tokenizer registry
Tokenizers SHALL be looked up by name and SHALL report whether they are sortable, lossy and valid for a value type. The
registry SHALL hold the vector index factory `hnsw` with Dgraph's option names (`metric`, `exponent`, `maxLevels`,
`efConstruction`, `efSearch`) and metrics (`euclidean`, `cosine`, `dotproduct`).

#### Scenario: A tokenizer is refused for the wrong type
- **WHEN** the geo tokenizer is checked against the bool type
- **THEN** the registry reports it as not valid

#### Scenario: hnsw options are validated
- **WHEN** the hnsw factory is given `metric: "manhattan"`
- **THEN** the options are refused with `Can't create a vector index for manhattan`

