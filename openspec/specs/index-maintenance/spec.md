# index-maintenance Specification

## Purpose
How a write keeps the indexes in step with the data: index tokens that follow the value, reverse edges, count
indexes, deleting all of a predicate for a subject, and the conversion of values to the schema type.
## Requirements
### Requirement: Index tokens follow the value
Setting an indexed value SHALL add the subject to the index key of each of its tokens, and changing or deleting the
value SHALL remove the subject from each token the old value produces and no remaining value of the subject produces;
Dgraph removes it from every token of the old value. A language-tagged value SHALL be tokenized by the language's
tokenizer.

#### Scenario: Setting an indexed value adds its tokens
- **WHEN** `name: string @index(term) .` holds no value for `0x1` and `name` is set to `big apple`
- **THEN** the index keys for `big` and `apple` each contain `0x1`

#### Scenario: Changing a value moves its tokens
- **WHEN** `name` of `0x1` changes from `big apple` to `red apple`
- **THEN** the index key for `big` no longer contains `0x1`, and `red` and `apple` do

#### Scenario: Deleting a value removes its index entries
- **WHEN** the indexed value of `0x1` is deleted
- **THEN** no index key of that predicate contains `0x1`

#### Scenario: A language value uses its language's tokenizer
- **WHEN** a fulltext-indexed predicate is set with language `de` to `Häuser`
- **THEN** the index holds the German stem's token

#### Scenario: Deleting one value of a list keeps shared tokens
- **WHEN** `tags: [string] @index(term) .` holds `big apple` and `red apple` for `0x1`, and `big apple` is deleted
- **THEN** the index key for `big` no longer contains `0x1`, and the keys for `red` and `apple` still do

### Requirement: Reverse edges
For a predicate with `@reverse`, setting `a → b` SHALL add `a` to the reverse key of `b`, deleting it SHALL remove
`a`, and overwriting a single uid predicate SHALL move `a` to the new target's reverse key.

#### Scenario: A reverse edge follows its forward edge
- **WHEN** `friend: uid @reverse .` is set from `0x1` to `0x2` and then to `0x3`
- **THEN** the reverse key of `0x3` contains `0x1` and the reverse key of `0x2` does not

### Requirement: Count indexes
For a predicate with `@count`, the subject SHALL be listed under the count key of its current number of postings when
that number is above zero and under no count key otherwise, a non-list predicate counting as one, and a set that adds
no posting SHALL leave the count unchanged; with `@reverse` as well, the reverse count keys SHALL be kept likewise.

#### Scenario: Adding a third uid moves the count
- **WHEN** `friend: [uid] @count .` holds two uids for `0x1` and a third is added
- **THEN** `0x1` is listed under count 3 and not under count 2

#### Scenario: Re-setting an existing uid leaves the count
- **WHEN** a uid already in the list is set again
- **THEN** the count key of the subject does not change

#### Scenario: Reverse counts are kept
- **WHEN** `friend: [uid] @count @reverse .` gains the edge `0x1 → 0x2`
- **THEN** the reverse count keys of `0x2` reflect one more incoming edge

### Requirement: Deleting all of a predicate for a subject
A delete of `<s> <p> *` SHALL remove every posting of `p` for `s`; for an indexed `p`, the tokens of each posting, a
language-tagged value's computed by its language's tokenizer; for a `p` with `@reverse`, its reverse edges; and the
subject from the count key it is listed under. Dgraph tokenizes every posting without its language, and removes the
subject from the count of all its postings, where a scalar with language values stays listed under count 1.

#### Scenario: A star delete clears reverse and count entries
- **WHEN** `<0x1> <friend> * .` is deleted on `friend: [uid] @reverse @count .`
- **THEN** `0x1` has no `friend` postings, appears in no reverse key of its former targets, is listed under no count
  key, and each former target's reverse count drops by one

#### Scenario: A star delete clears index entries
- **WHEN** `name: string @index(fulltext) @lang .` holds an untagged value and a value tagged `de` for `0x1`, and
  `<0x1> <name> * .` is deleted
- **THEN** no index key of `name` contains `0x1`

#### Scenario: A star delete on a scalar with language values clears its count
- **WHEN** `name: string @count @lang .` holds an untagged value and a value tagged `en` for `0x1`, and
  `<0x1> <name> * .` is deleted
- **THEN** `0x1` is listed under no count key of `name`

### Requirement: Values are converted to the schema type
Applying an edge SHALL convert its value to the predicate's schema type, and SHALL refuse the edge, writing nothing of
its request, with Dgraph's text when the value cannot be converted.

#### Scenario: A value is converted to the schema type
- **WHEN** `age: int .` is set for `0x1` with the untyped value `"12"`
- **THEN** `age` of `0x1` holds the int 12

#### Scenario: A value that cannot be converted is refused
- **WHEN** `age: int .` is set for `0x1` with the untyped value `"twelve"`
- **THEN** the write fails with `strconv.ParseInt: parsing "twelve": invalid syntax`, and `age` of `0x1` holds nothing

