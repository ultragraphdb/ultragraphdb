## ADDED Requirements

### Requirement: Index tokens follow the value
Setting an indexed value SHALL add the subject to the index key of each of its tokens, and changing or deleting the
value SHALL remove the subject from the old value's tokens. A language-tagged value SHALL be tokenized by the
language's tokenizer.

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
A delete of `<s> <p> *` SHALL remove every posting of `p` for `s`, its index tokens or reverse edges, and the subject
from its count key.

#### Scenario: A star delete clears reverse and count entries
- **WHEN** `<0x1> <friend> * .` is deleted on `friend: [uid] @reverse @count .`
- **THEN** `0x1` has no `friend` postings, appears in no reverse key of its former targets, is listed under no count
  key, and each former target's reverse count drops by one

#### Scenario: A star delete clears index entries
- **WHEN** `<0x1> <name> * .` is deleted on an indexed `name`
- **THEN** no index key of `name` contains `0x1`
