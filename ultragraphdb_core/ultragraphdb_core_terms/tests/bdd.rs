/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The term-dictionary scenarios of literal encoding.

use std::collections::BTreeMap;

use cucumber::{World, then, when};
use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNode, NamedNodeRef};
use ultragraphdb_core_terms::{
    EncodedLiteral, EncodedTerm, FLOAT32_VECTOR, Hash128, TermError, TermView, Text,
};

#[derive(Debug, Default, World)]
struct Steps {
    encoded: Vec<EncodedTerm>,
    dictionary: BTreeMap<Hash128, String>,
    literals: Vec<Literal>,
}

fn no_uid(_: NamedNodeRef<'_>) -> Result<u64, TermError> {
    panic!("a known datatype needs no UID")
}

impl Steps {
    fn encode(&mut self, literal: Literal) {
        let encoded: EncodedLiteral<'_> =
            EncodedTerm::literal(literal.as_ref(), no_uid).expect("encodes");
        for (hash, string) in encoded.strings() {
            self.dictionary.insert(hash, string.to_string());
        }
        self.encoded.push(encoded.into_term());
        self.literals.push(literal);
    }

    fn decode(&self, i: usize) -> Literal {
        self.encoded[i]
            .view()
            .expect("views")
            .to_literal(
                |_| -> Result<NamedNode, TermError> { panic!("a known datatype needs no IRI") },
                |h| Ok(self.dictionary[&h].clone()),
            )
            .expect("decodes")
    }
}

fn vector(lexical: &str) -> Literal {
    Literal::new_typed_literal(lexical, NamedNode::new_unchecked(FLOAT32_VECTOR))
}

#[when(expr = "{string}^^xsd:integer is encoded")]
fn encode_integer(w: &mut Steps, lexical: String) {
    w.encode(Literal::new_typed_literal(lexical, xsd::INTEGER));
}

#[then(expr = "its encoding holds the value {int} and decodes to {string}^^xsd:integer")]
fn holds_integer(w: &mut Steps, value: i64, lexical: String) {
    assert!(matches!(w.encoded[0].view(), Ok(TermView::Integer(v)) if i64::from(v) == value));
    assert_eq!(
        w.decode(0),
        Literal::new_typed_literal(lexical, xsd::INTEGER)
    );
}

#[when(expr = "{string}^^xsd:integer and {string}^^xsd:integer are encoded")]
fn encode_two_integers(w: &mut Steps, first: String, second: String) {
    w.encode(Literal::new_typed_literal(first, xsd::INTEGER));
    w.encode(Literal::new_typed_literal(second, xsd::INTEGER));
}

#[then(expr = "their encodings differ, and the first decodes to {string}^^xsd:integer")]
fn first_kept(w: &mut Steps, lexical: String) {
    assert_ne!(w.encoded[0], w.encoded[1]);
    assert_eq!(
        w.decode(0),
        Literal::new_typed_literal(lexical, xsd::INTEGER)
    );
}

#[when("a 40-byte string literal is encoded")]
fn encode_long_string(w: &mut Steps) {
    w.encode(Literal::new_simple_literal("a".repeat(40)));
}

#[then("its encoding names a 128-bit hash, and decoding through the dictionary returns the string")]
fn long_string_hashed(w: &mut Steps) {
    let hash = Hash128::of("a".repeat(40).as_bytes());
    assert_eq!(
        w.encoded[0].view(),
        Ok(TermView::String(Text::Hashed(hash)))
    );
    assert_eq!(w.encoded[0].as_bytes().len(), 17);
    assert_eq!(w.decode(0), w.literals[0]);
}

#[when(expr = "{string}@fr is encoded")]
fn encode_french(w: &mut Steps, lexical: String) {
    w.encode(Literal::new_language_tagged_literal(lexical, "fr").expect("a language tag"));
}

#[then(expr = "its encoding holds {word} and {word} inline, and decodes to {string}@fr")]
fn french_inline(w: &mut Steps, lexical: String, language: String, decoded: String) {
    assert_eq!(
        w.encoded[0].view(),
        Ok(TermView::LangString {
            lexical: Text::Inline(&lexical),
            language: Text::Inline(&language),
            direction: None,
        })
    );
    assert!(w.dictionary.is_empty());
    assert_eq!(
        w.decode(0),
        Literal::new_language_tagged_literal(decoded, "fr").unwrap()
    );
}

#[when("a 768-element ugdb:float32vector literal is written")]
fn encode_large_vector(w: &mut Steps) {
    let elements: Vec<String> = (0..768).map(|i| format!("{}", i as f32 / 8.0)).collect();
    w.encode(vector(&format!("[{}]", elements.join(", "))));
}

#[then(
    "its encoding is the tag byte, the dimension 768 and 3,072 bytes of elements, and the dictionary gains no entry"
)]
fn large_vector_by_value(w: &mut Steps) {
    let bytes = w.encoded[0].as_bytes();
    assert_eq!(bytes.len(), 1 + 2 + 3072);
    assert_eq!(&bytes[1..3], &768u16.to_le_bytes());
    assert_eq!(&bytes[3 + 4 * 5..3 + 4 * 6], &(5.0f32 / 8.0).to_le_bytes());
    assert!(w.dictionary.is_empty());
    assert_eq!(w.decode(0), w.literals[0]);
}

#[when(expr = "{string}^^ugdb:float32vector is written and read back")]
fn round_trip_vector(w: &mut Steps, lexical: String) {
    w.encode(vector(&lexical));
}

#[then(expr = "the read returns {string}^^ugdb:float32vector")]
fn vector_read_back(w: &mut Steps, lexical: String) {
    assert_eq!(w.decode(0), vector(&lexical));
}

fn main() {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()),
    );
}
