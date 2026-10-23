## ADDED Requirements

### Requirement: Ngram index tokens use the value's language
The ngram tokenizer chosen for a value with a language SHALL use that language's stop words and stemmer, as the
ngram query tokens do. Dgraph's `GetTokenizerForLang` returns the English ngram tokenizer for every language, so a
query can build tokens the index never stored (ledger 14).

#### Scenario: A German value's ngram index tokens match its query tokens
- **WHEN** `Katzen und Auffassung` tagged `de` is tokenized for the ngram index and for an ngram query
- **THEN** both yield the same stemmed shingles, starting with `katz`

### Requirement: Lowercasing keeps every rune
Lowercasing a term SHALL map every rune by Unicode's simple lowercase mapping and a final `Σ` to `ς`, on every path,
and SHALL keep every rune of the term. Dgraph's lowercase filter leaves stale bytes after a rune whose lowercase form is
shorter, and skips the final-sigma rule once the term grows (ledger 16, 30).

#### Scenario: İstanbul lowercases whole
- **WHEN** `İstanbul` is tokenized by the term tokenizer
- **THEN** the token is the term `istanbul`

#### Scenario: A final sigma after a widening rune becomes final
- **WHEN** `ȺΣ` is lowercased
- **THEN** the result is `ⱥς`
