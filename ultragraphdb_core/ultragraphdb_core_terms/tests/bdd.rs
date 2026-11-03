/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The term-dictionary scenarios of literal encoding.

use std::collections::BTreeMap;

use cucumber::{World, then, when};
use oxrdf::vocab::xsd;
use oxrdf::{Literal, NamedNode, NamedNodeRef};
use ultragraphdb_core_ids::SystemId;
use ultragraphdb_core_terms::{
    EncodedLiteral, EncodedTerm, FLOAT32_VECTOR, Hash128, TermError, TermView, Text,
};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[derive(Debug, Default, World)]
struct Steps {
    encoded: Vec<EncodedTerm>,
    dictionary: BTreeMap<Hash128, String>,
    literals: Vec<Literal>,
}

fn no_uid(_: NamedNodeRef<'_>) -> Result<SystemId, TermError> {
    Err(TermError::malformed(
        Vec::new(),
        "a known datatype needs no UID",
    ))
}

impl Steps {
    fn encode(&mut self, literal: Literal) -> TestResult {
        let encoded: EncodedLiteral<'_> =
            EncodedTerm::literal(literal.as_ref(), no_uid).context("encodes")?;
        for (hash, string) in encoded.strings() {
            let _entered: Option<String> = self.dictionary.insert(hash, string.to_string());
        }
        self.encoded.push(encoded.into_term());
        self.literals.push(literal);
        Ok(())
    }

    fn decode(&self, i: usize) -> TestResult<Literal> {
        self.encoded[i]
            .view()
            .context("views")?
            .to_literal(
                |_| {
                    Err(TermError::malformed(
                        Vec::new(),
                        "a known datatype needs no IRI",
                    ))
                },
                |h| Ok(self.dictionary[&h].clone()),
            )
            .context("decodes")
    }
}

fn vector(lexical: impl Into<String>) -> Literal {
    Literal::new_typed_literal(lexical, NamedNode::new_unchecked(FLOAT32_VECTOR))
}

#[when(expr = "{string}^^xsd:integer is encoded")]
fn encode_integer(w: &mut Steps, lexical: String) -> TestResult {
    w.encode(Literal::new_typed_literal(lexical, xsd::INTEGER))?;
    Ok(())
}

#[then(expr = "its encoding holds the value {int} and decodes to {string}^^xsd:integer")]
fn holds_integer(w: &mut Steps, value: i64, lexical: String) -> TestResult {
    assert!(matches!(w.encoded[0].view(), Ok(TermView::Integer(v)) if i64::from(v) == value));
    assert_eq!(
        w.decode(0)?,
        Literal::new_typed_literal(lexical, xsd::INTEGER)
    );
    Ok(())
}

#[when(expr = "{string}^^xsd:integer and {string}^^xsd:integer are encoded")]
fn encode_two_integers(w: &mut Steps, first: String, second: String) -> TestResult {
    w.encode(Literal::new_typed_literal(first, xsd::INTEGER))?;
    w.encode(Literal::new_typed_literal(second, xsd::INTEGER))?;
    Ok(())
}

#[then(expr = "their encodings differ, and the first decodes to {string}^^xsd:integer")]
fn first_kept(w: &mut Steps, lexical: String) -> TestResult {
    assert_ne!(w.encoded[0], w.encoded[1]);
    assert_eq!(
        w.decode(0)?,
        Literal::new_typed_literal(lexical, xsd::INTEGER)
    );
    Ok(())
}

#[when("a 40-byte string literal is encoded")]
fn encode_long_string(w: &mut Steps) -> TestResult {
    w.encode(Literal::new_simple_literal("a".repeat(40)))?;
    Ok(())
}

#[then("its encoding names a 128-bit hash, and decoding through the dictionary returns the string")]
fn long_string_hashed(w: &mut Steps) -> TestResult {
    let hash = Hash128::of("a".repeat(40).as_bytes());
    assert_eq!(
        w.encoded[0].view(),
        Ok(TermView::String(Text::Hashed(hash)))
    );
    assert_eq!(w.encoded[0].as_bytes().len(), 17);
    assert_eq!(w.decode(0)?, w.literals[0]);
    Ok(())
}

#[when(expr = "{string}@fr is encoded")]
fn encode_french(w: &mut Steps, lexical: String) -> TestResult {
    w.encode(Literal::new_language_tagged_literal(lexical, "fr").context("a language tag")?)?;
    Ok(())
}

#[then(expr = "its encoding holds {word} and {word} inline, and decodes to {string}@fr")]
fn french_inline(w: &mut Steps, parts: &[String]) -> TestResult {
    let (lexical, language, decoded) = (&parts[0], &parts[1], &parts[2]);
    assert_eq!(
        w.encoded[0].view(),
        Ok(TermView::LangString {
            lexical: Text::Inline(lexical),
            language: Text::Inline(language),
            direction: None,
        })
    );
    assert!(w.dictionary.is_empty());
    assert_eq!(
        w.decode(0)?,
        Literal::new_language_tagged_literal(decoded.as_str(), "fr")?
    );
    Ok(())
}

#[when("a 768-element ugdb:float32vector literal is written")]
fn encode_large_vector(w: &mut Steps) -> TestResult {
    let elements: Vec<String> = (0..768).map(|i| format!("{}", i as f32 / 8.0)).collect();
    w.encode(vector(format!("[{}]", elements.join(", "))))?;
    Ok(())
}

#[then(
    "its encoding is the tag byte, the dimension 768 and 3,072 bytes of elements, and the dictionary gains no entry"
)]
fn large_vector_by_value(w: &mut Steps) -> TestResult {
    let bytes = w.encoded[0].as_bytes();
    assert_eq!(bytes.len(), 1 + 2 + 3072);
    assert_eq!(&bytes[1..3], &768u16.to_le_bytes());
    assert_eq!(&bytes[3 + 4 * 5..3 + 4 * 6], &(5.0f32 / 8.0).to_le_bytes());
    assert!(w.dictionary.is_empty());
    assert_eq!(w.decode(0)?, w.literals[0]);
    Ok(())
}

#[when(expr = "{string}^^ugdb:float32vector is written and read back")]
fn round_trip_vector(w: &mut Steps, lexical: String) -> TestResult {
    w.encode(vector(lexical))?;
    Ok(())
}

#[then(expr = "the read returns {string}^^ugdb:float32vector")]
fn vector_read_back(w: &mut Steps, lexical: String) -> TestResult {
    assert_eq!(w.decode(0)?, vector(lexical));
    Ok(())
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
