## MODIFIED Requirements

### Requirement: Index tokens follow the value
Writing a literal to a predicate the catalog indexes SHALL add the subject to the value-index key of each token of the
literal, and deleting or overwriting the literal SHALL remove the subject from each token the old literal produces and
no remaining literal of the subject's predicate produces. A language-tagged literal SHALL be tokenized by its language's
tokenizer.

#### Scenario: Writing an indexed literal adds its tokens
- **WHEN** `name` is declared with a `term` index and `<a> <name> "big apple"` is written
- **THEN** the value-index keys of `name` for `big` and `apple` each hold `a`

#### Scenario: Overwriting a single-valued literal moves its tokens
- **WHEN** `name` is declared single-valued with a `term` index, holds `"big apple"` for `a`, and `"red apple"` is
  written
- **THEN** the key for `big` no longer holds `a`, and the keys for `red` and `apple` do

#### Scenario: Deleting a literal removes its index entries
- **WHEN** `<a> <name> "big apple"` is deleted and `a` holds no other `name`
- **THEN** no value-index key of `name` holds `a`

#### Scenario: A language-tagged literal uses its language's tokenizer
- **WHEN** `body` is declared with a `fulltext` index and `<a> <body> "Häuser"@de` is written
- **THEN** the index holds the German stem's token for `a`

#### Scenario: Deleting one literal keeps shared tokens
- **WHEN** `tags` is declared with a `term` index, `a` holds `"big apple"` and `"red apple"`, and `"big apple"` is
  deleted
- **THEN** the key for `big` no longer holds `a`, and the keys for `red` and `apple` still do

### Requirement: Reverse edges
Every statement SHALL keep its reverse entry, for every predicate and every object kind: writing `a p o` SHALL add `a`
to the reverse list of `p` for `o`, deleting it SHALL remove `a`, and overwriting a single-valued predicate SHALL move
`a` from the old object's reverse list to the new one's.

#### Scenario: A reverse entry follows its statement
- **WHEN** `friend` is declared single-valued and `<a> <friend>` is written `<b>` and then `<c>`
- **THEN** the reverse list of `friend` for `c` holds `a`, and the one for `b` does not

#### Scenario: Undeclared predicates keep reverse entries
- **WHEN** `<a> <likes> <b>` is written and `likes` is not declared
- **THEN** the reverse list of `likes` for `b` holds `a`

### Requirement: Count indexes
For a predicate the catalog declares an exact count index on, a subject SHALL be listed under the forward count key of
its current number of objects when that number is above zero and under no count key otherwise, a single-valued
predicate counting as one, and an object SHALL be listed likewise under the reverse count key of its number of
subjects. A write that adds no object SHALL leave the counts unchanged.

#### Scenario: Adding a third object moves the count
- **WHEN** `friend` has an exact count index, `a` holds two objects of it, and a third is written
- **THEN** `a` is listed under forward count 3 and not under 2

#### Scenario: Re-writing an existing object leaves the count
- **WHEN** an object `a` already holds is written again
- **THEN** the count keys of `a` do not change

#### Scenario: Reverse counts are kept
- **WHEN** `friend` has an exact count index and gains `<a> <friend> <b>`
- **THEN** the reverse count keys of `b` reflect one more subject

### Requirement: Deleting all of a predicate for a subject
A delete by the pattern of a subject and predicate SHALL remove every object of the predicate for the subject in its
graph, with each object's reverse entry, the predicate from the subject's predicates and from each object's object
predicates where nothing else keeps it there, the subject from every token its literals produce, each literal tokenized
by its language, and the subject and objects from the count keys they are listed under.

#### Scenario: A delete by pattern clears reverse and count entries
- **WHEN** `friend` has an exact count index, `a` holds `b` and `c`, and the pattern `a`, `friend` is deleted
- **THEN** `a` holds no `friend`, the reverse lists for `b` and `c` no longer hold `a`, `a` is listed under no count
  key, and the reverse counts of `b` and `c` drop by one

#### Scenario: A delete by pattern clears index entries
- **WHEN** `name` has a `fulltext` index, `a` holds an untagged literal and one tagged `de`, and the pattern `a`, `name`
  is deleted
- **THEN** no value-index key of `name` holds `a`

## ADDED Requirements

### Requirement: Each tokenizer indexes the datatypes it takes
A tokenizer SHALL index the literals of the datatypes it takes, computing its tokens from the literal's value, so that
literals with equal values and different lexical forms produce equal tokens. It SHALL leave a literal of any other
datatype unindexed, which is still held:

| Tokenizers | Datatypes |
|---|---|
| `int` | `xsd:integer` and the types derived from it, within 64 bits |
| `float` | `xsd:double`, `xsd:float` |
| `bigfloat` | `xsd:decimal` |
| `bool` | `xsd:boolean` |
| `year`, `month`, `day`, `hour` | `xsd:dateTime`, `xsd:dateTimeStamp`; `xsd:date` for `year`, `month` and `day`. A value without a time zone is indexed as UTC |
| `exact`, `hash`, `term`, `fulltext`, `trigram`, `ngram`, `sha256` | `xsd:string`, `rdf:langString`, `rdf:dirLangString` |
| `geo` | `geo:geoJSONLiteral` (GeoSPARQL 1.1, Requirements 25–27): a GeoJSON Geometry object in CRS84 |
| the vector index | `ugdb:float32vector` |

#### Scenario: Non-canonical integers index alike
- **WHEN** `periods` is declared with an `int` index, `<a> <periods> "042"^^xsd:integer` and `<b> <periods> 42` are
  written
- **THEN** the value-index key of `periods` for 42 holds `a` and `b`

#### Scenario: Every value of a multi-valued predicate is indexed
- **WHEN** `periods` is multi-valued with an `int` index and `a` holds 41, 42 and 43
- **THEN** a range read of the `int` index over 42 to 43 returns `a`

#### Scenario: A literal of another datatype is held unindexed
- **WHEN** `periods` is declared with an `int` index and no datatype constraint, and `<a> <periods> "soon"` is written
- **THEN** `a` holds `"soon"`, and no value-index key of `periods` holds `a`

#### Scenario: A GeoJSON geometry is indexed by its cells
- **WHEN** `where` is declared with a `geo` index and
  `<a> <where> "{\"type\": \"Point\", \"coordinates\": [8.63, 50.12]}"^^geo:geoJSONLiteral` is written
- **THEN** the value-index keys of `where` for the point's S2 cell tokens hold `a`

### Requirement: Writes read only what they need
Inserting a statement SHALL read the subject's forward list of its predicate, so that a statement already held is
neither counted again in the statistics nor written over its statement IDs, and SHALL write its reverse,
predicate-list, value-index and statistics entries as deltas without reading them. A subject the write created holds
no list, so its inserts read none. A write SHALL read another list only to check a unique predicate or to maintain an
exact count index.

#### Scenario: An insert reads only the subject's list
- **WHEN** `<a> <name> "Ann"` is held, `tag` is multi-valued with a `term` index and no count index, and
  `<a> <tag> "x"` is written
- **THEN** the write reads the forward list of `tag` for `a` and no other list, and writes the forward, reverse,
  predicate-list, index and statistics deltas

#### Scenario: An insert of a new subject reads nothing
- **WHEN** `<a> <tag> "x"` is written and `a` is new to the store
- **THEN** the write reads no list

#### Scenario: A delete reads the subject's list
- **WHEN** `<a> <tag> "x"` is deleted
- **THEN** the write reads the forward list of `tag` for `a`, to keep the tokens `a`'s other tags produce

### Requirement: Terms are kept as written
A write SHALL store each object as the term it names, its lexical form, datatype, language and direction unchanged, and
SHALL convert a literal only to compute its index tokens.

#### Scenario: An integer keeps its lexical form under an index
- **WHEN** `n` has an `int` index and `<a> <n> "007"^^xsd:integer` is written
- **THEN** `a` holds `"007"^^xsd:integer`, and the int index lists `a` under 7

## REMOVED Requirements

### Requirement: Values are converted to the schema type
**Reason**: RDF keeps every term as written; the catalog constrains datatypes instead of converting values.
**Migration**: "Terms are kept as written", and the catalog's datatype constraint ("Writes are checked against the
catalog").
