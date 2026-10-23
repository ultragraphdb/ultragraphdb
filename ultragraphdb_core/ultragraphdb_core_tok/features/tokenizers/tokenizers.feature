# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.

Feature: Tokenizers
  Dgraph's index tokenizers, their registry, text analysis and the hnsw factory entry
  (openspec/specs/tokenizers/spec.md).

  Scenario: Every tokenizer's tokens match the golden vectors
    When each tokenizer tokenizes its golden inputs
    Then every token, identifier byte included, equals Dgraph's

  Scenario: Integer tokens order like their values
    When the ints MinInt64, -1, 0, 1 and MaxInt64 are tokenized
    Then the tokens' byte order is the values' order

  Scenario: Float tokens truncate toward zero
    When the floats -2.7 and 2.7 are tokenized
    Then their tokens equal the int tokens of -2 and 2

  Scenario: Hour tokens hold UTC fields
    When "2017-01-01T12:12:12+01:00" is tokenized by the hour tokenizer
    Then the token encodes year 2017, month 1, day 1 and hour 11

  Scenario: Term tokens are lowercased and deduplicated
    When "Tokenizer works works!" is tokenized by the term tokenizer
    Then the tokens are "tokenizer" and "works", each prefixed with 0x01

  Scenario: Chinese term tokens split on spaces
    When "他是 一个好人" is tokenized by the term tokenizer for "zh"
    Then the tokens are "他是" and "一个好人"

  Scenario: English fulltext stems and drops stop words
    When "Stemming works!" is tokenized by the fulltext tokenizer for "en"
    Then the tokens are "stem" and "work"

  Scenario: Trigram tokens preserve case
    When "Dgraph rocks!" is tokenized by the trigram tokenizer
    Then the result is Dgraph's 11 trigrams, sorted

  Scenario: Trigrams are byte windows
    When "Grüße" is tokenized by the trigram tokenizer
    Then the tokens are its three-byte windows, "Gr\xc3" among them

  Scenario: Chinese fulltext produces bigrams
    When the golden Chinese sentence is tokenized by the fulltext tokenizer for "zh"
    Then the tokens equal Dgraph's bigrams

  Scenario: Stemmed languages match Dgraph per language
    When the golden sentences for every language with a stemmer are tokenized by fulltext
    Then each language's tokens equal Dgraph's

  Scenario: A long shingle is hashed
    When a sentence yielding a shingle of 30 or more bytes is tokenized by ngram
    Then that shingle appears as its BLAKE2b-256 digest and shorter shingles appear as text

  Scenario: Short queries take one shingle
    When the two-word query "quick fox" is tokenized for ngram queries
    Then the query tokens are the one shingle of both words

  Scenario: Language tags reduce
    When "en-US", "zh-Hant", "sh", "und" and "x!" are reduced
    Then the results are "en", "zh", "sr", "en" and "en"

  Scenario: Collated exact tokens order like the language
    When the golden German strings are tokenized by the exact tokenizer with language "de"
    Then the tokens equal Dgraph's, byte for byte

  Scenario: Geo tokens match Dgraph
    When the golden points and polygons are tokenized by the geo tokenizer
    Then the tokens, sorted, equal Dgraph's

  Scenario: A tokenizer is refused for the wrong type
    When the geo tokenizer is checked against the bool type
    Then the registry reports it as not valid

  Scenario: hnsw options are validated
    When the hnsw factory is given metric "manhattan"
    Then the options are refused with "Can't create a vector index for manhattan"
