/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The settings-toml scenarios, bound to `features/settings_toml/`.

#[path = "utils/document.rs"]
mod document;
#[path = "utils/toml_test_suite.rs"]
mod toml_test_suite;

use cucumber::{World, then, when};
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};
use ultragraphdb_tool_toml::{TomlDocument, TomlValue};
use ultragraphdb_tool_toml::{
    TomlError, TomlErrorEnum, TomlFeature, TomlInvalid, TomlKind, TomlLimit,
};

use crate::document::{reread_alike, row, rows};
use crate::toml_test_suite::{
    INVALID_DOCUMENTS, VALID_DOCUMENTS, check_invalid, check_valid, invalid_cases, valid_cases,
};

/// Inputs the hermetic stand-in for the fuzzer feeds the parser: arbitrary bytes and mutated valid documents.
const ARBITRARY_INPUTS: usize = 20_000;

#[derive(Debug, Default, World)]
struct Steps {
    parsed: Option<Result<TomlDocument, TomlError>>,
    read: Option<TomlError>,
    finished: Option<TomlError>,
    checked: usize,
}

/// The parse the When step made.
fn parsed(w: &mut Steps) -> TestResult<Result<TomlDocument, TomlError>> {
    w.parsed.take().context("a parse")
}

/// The refusal the When step's parse returned.
fn refusal(w: &mut Steps) -> TestResult<TomlErrorEnum> {
    match parsed(w)? {
        Ok(document) => Err(TestError::new(format!("parsed as {:?}", rows(&document)))),
        Err(error) => Ok(error.0),
    }
}

#[when(
    "the document `name = \"dev\"`, `[storage]`, `partition_count = 4  # one per core`, `sync = true` is parsed"
)]
fn a_settings_file(w: &mut Steps) {
    let text = "name = \"dev\"\n[storage]\npartition_count = 4  # one per core\nsync = true\n";
    w.parsed = Some(TomlDocument::parse(text.as_bytes()));
}

#[then(
    "the root table holds `name` = `\"dev\"`, and the table `storage` holds `partition_count` = 4 and `sync` = true"
)]
fn the_settings_file_holds(w: &mut Steps) -> TestResult {
    let document = parsed(w)??;
    assert_eq!(
        rows(&document),
        [
            row("", "name", TomlValue::String("dev".to_string())),
            row("storage", "partition_count", TomlValue::Integer(4)),
            row("storage", "sync", TomlValue::Boolean(true)),
        ]
    );
    Ok(())
}

#[when(regex = r#"^the values `"a\\tbé"`, `-1_000` and `\+7` are parsed$"#)]
fn escapes_and_integers(w: &mut Steps) {
    let text = "s = \"a\\tbé\"\nn = -1_000\np = +7\n";
    w.parsed = Some(TomlDocument::parse(text.as_bytes()));
}

#[then("they are the string `a`, tab, `b`, `é`, and the integers −1000 and 7")]
fn escapes_and_integers_follow_toml(w: &mut Steps) -> TestResult {
    let document = parsed(w)??;
    assert_eq!(
        rows(&document),
        [
            row("", "s", TomlValue::String("a\u{9}b\u{e9}".to_string())),
            row("", "n", TomlValue::Integer(-1_000)),
            row("", "p", TomlValue::Integer(7)),
        ]
    );
    Ok(())
}

#[when("every document of the toml-test suite's valid cases that uses only the subset is parsed")]
fn the_valid_suite(w: &mut Steps) -> TestResult {
    for (path, bytes, expected) in &valid_cases()? {
        check_valid(path, bytes, expected)?;
        w.checked += 1;
    }
    Ok(())
}

#[then(
    "each parses to the values the suite's expected JSON states, the documents vendored under `testdata/` with their source and version"
)]
fn the_valid_suite_agrees(w: &mut Steps) -> TestResult {
    assert_eq!(w.checked, VALID_DOCUMENTS);
    let source = std::fs::read_to_string(
        ultragraphdb_tool_bdd::testdata_dir()?
            .join("toml-test")
            .join("SOURCE.txt"),
    )
    .context("the suite's source and version")?;
    assert!(source.contains("https://github.com/toml-lang/toml-test"));
    assert!(source.contains("v1.6.0"));
    assert!(source.contains("369cc3ae275fd76b6e21780ebf87609e5b2a951a"));
    Ok(())
}

#[when("a document holds `peers = [\"a\", \"b\"]` on line 3")]
fn an_array_on_line_3(w: &mut Steps) {
    let text = "name = \"dev\"\n[transport]\npeers = [\"a\", \"b\"]\n";
    w.parsed = Some(TomlDocument::parse(text.as_bytes()));
}

#[then("parsing fails with an unsupported-feature error naming arrays and line 3")]
fn arrays_are_unsupported(w: &mut Steps) -> TestResult {
    let expected = TomlErrorEnum::Unsupported {
        feature: TomlFeature::Arrays,
        line: 3,
    };
    assert_eq!(refusal(w)?, expected);
    Ok(())
}

#[when("a document holds `ratio = 0.5`")]
fn a_float(w: &mut Steps) {
    w.parsed = Some(TomlDocument::parse(b"[cache]\nratio = 0.5\n"));
}

#[then("parsing fails with an unsupported-feature error naming floats and the line")]
fn floats_are_unsupported(w: &mut Steps) -> TestResult {
    let expected = TomlErrorEnum::Unsupported {
        feature: TomlFeature::Floats,
        line: 2,
    };
    assert_eq!(refusal(w)?, expected);
    Ok(())
}

#[when("a table holds `sync = true` twice")]
fn a_duplicate_key(w: &mut Steps) {
    w.parsed = Some(TomlDocument::parse(
        b"[storage]\nsync = true\nsync = true\n",
    ));
}

#[then("parsing fails with an invalid-document error naming `sync` and the second line")]
fn the_duplicate_is_invalid(w: &mut Steps) -> TestResult {
    let expected = TomlErrorEnum::Invalid {
        reason: TomlInvalid::DuplicateKey("sync".to_string()),
        line: 3,
    };
    assert_eq!(refusal(w)?, expected);
    Ok(())
}

#[when("every document of the toml-test suite's invalid cases is parsed")]
fn the_invalid_suite(w: &mut Steps) -> TestResult {
    for (path, bytes) in &invalid_cases()? {
        check_invalid(path, bytes)?;
        w.checked += 1;
    }
    Ok(())
}

#[then("each is refused, as unsupported or as invalid, and none parses")]
fn the_invalid_suite_is_refused(w: &mut Steps) {
    assert_eq!(w.checked, INVALID_DOCUMENTS);
}

#[when(
    "a role reads `[storage] partition_count` from a document holding `[storage] partiton_count = 4`"
)]
fn a_misspelled_key(w: &mut Steps) -> TestResult {
    let mut document = TomlDocument::parse(b"[storage]\npartiton_count = 4\n")?;
    w.read = document.integer("storage", "partition_count").err();
    w.finished = document.finish().err();
    Ok(())
}

#[then(
    "the read of `partition_count` reports it missing, and the final check refuses `storage.partiton_count`"
)]
fn the_misspelling_is_refused(w: &mut Steps) -> TestResult {
    let read = w.read.take().context("a refused read")?;
    assert_eq!(
        read.0,
        TomlErrorEnum::Missing {
            table: "storage".to_string(),
            key: "partition_count".to_string(),
            expected: TomlKind::Integer,
        }
    );
    let finished = w.finished.take().context("a refused final check")?;
    assert_eq!(
        finished.0,
        TomlErrorEnum::UnreadKey {
            table: "storage".to_string(),
            key: "partiton_count".to_string(),
        }
    );
    assert!(finished.to_string().contains("storage.partiton_count"));
    Ok(())
}

#[when("a role reads `[storage] partition_count` as an integer from `partition_count = \"4\"`")]
fn a_value_of_the_wrong_kind(w: &mut Steps) -> TestResult {
    let mut document = TomlDocument::parse(b"[storage]\npartition_count = \"4\"\n")?;
    w.read = document.integer("storage", "partition_count").err();
    Ok(())
}

#[then(
    "the read fails with an error naming `storage.partition_count`, an integer expected and a string found"
)]
fn the_wrong_kind_is_refused(w: &mut Steps) -> TestResult {
    let read = w.read.take().context("a refused read")?;
    assert_eq!(
        read.0,
        TomlErrorEnum::WrongKind {
            table: "storage".to_string(),
            key: "partition_count".to_string(),
            expected: TomlKind::Integer,
            found: TomlKind::String,
        }
    );
    let text = read.to_string();
    assert!(text.contains("storage.partition_count"), "{text}");
    assert!(text.contains("an integer"), "{text}");
    assert!(text.contains("a string"), "{text}");
    Ok(())
}

#[when("a document of 64 KiB plus one byte is parsed")]
fn an_oversized_document(w: &mut Steps) {
    // 65,537 bytes of comment lines, each within the line limit: only the document's size is wrong.
    let line = format!("#{}\n", "x".repeat(4_094));
    let mut text = line.repeat(16);
    text.push('\n');
    assert_eq!(text.len(), 65_537);
    w.parsed = Some(TomlDocument::parse(text.as_bytes()));
}

#[then("parsing fails with an error naming the 64 KiB limit")]
fn the_document_limit_is_named(w: &mut Steps) -> TestResult {
    let expected = TomlErrorEnum::Limit {
        limit: TomlLimit::DocumentBytes,
        max: 65_536,
    };
    assert_eq!(refusal(w)?, expected);
    Ok(())
}

/// A xorshift64 generator: the stand-in fuzzer's inputs, the same on every run.
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

/// Input `index`: odd ones arbitrary bytes, most from TOML's own characters; even ones a valid document with bytes
/// replaced, inserted and cut.
fn arbitrary_input(index: usize, state: &mut u64, seeds: &[Vec<u8>]) -> Vec<u8> {
    const ALPHABET: &[u8] = b"[]=\"'#.\\ \t\r\nabtuxU0123456789+-_:eE{},";
    let roll = |generator: &mut u64, bound: usize| (next(generator) % bound as u64) as usize;
    if index % 2 == 1 || seeds.is_empty() {
        let len = roll(state, 256);
        return (0..len)
            .map(|_| match roll(state, 4) {
                0 => next(state).to_le_bytes()[0],
                _ => ALPHABET[roll(state, ALPHABET.len())],
            })
            .collect();
    }
    let mut input = seeds[roll(state, seeds.len())].clone();
    for _ in 0..1 + roll(state, 4) {
        let at = roll(state, input.len() + 1);
        let byte = ALPHABET[roll(state, ALPHABET.len())];
        match roll(state, 3) {
            0 if at < input.len() => input[at] = byte,
            1 => input.insert(at, byte),
            _ => input.truncate(at),
        }
    }
    input
}

#[when("the fuzzer feeds the parser arbitrary bytes")]
fn arbitrary_bytes(w: &mut Steps) -> TestResult {
    let seeds: Vec<Vec<u8>> = valid_cases()?
        .into_iter()
        .map(|(_, bytes, _)| bytes)
        .collect();
    let mut state = 0x9E37_79B9_7F4A_7C15;
    for index in 0..ARBITRARY_INPUTS {
        let input = arbitrary_input(index, &mut state, &seeds);
        // A panic would end the step: reaching the match is the property.
        match TomlDocument::parse(&input) {
            Ok(document) => reread_alike(document)?,
            Err(error) => assert!(!error.to_string().is_empty()),
        }
        w.checked += 1;
    }
    Ok(())
}

#[then("every input is accepted or refused with a typed error, and nothing panics")]
fn nothing_panics(w: &mut Steps) {
    assert_eq!(w.checked, ARBITRARY_INPUTS);
}

fn main() -> TestResult {
    futures::executor::block_on(
        Steps::cucumber()
            .fail_on_skipped()
            .run_and_exit(ultragraphdb_tool_bdd::features_dir()?),
    );
    Ok(())
}
