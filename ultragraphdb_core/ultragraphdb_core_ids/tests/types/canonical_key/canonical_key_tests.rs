/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{CanonicalKey, IdError, IdErrorEnum};
use ultragraphdb_tool_bdd::{Context, TestResult};

// Expected bytes by hand: the IRI's length as 4 big-endian bytes, then its UTF-8 bytes.

#[test]
fn test_the_canonical_encoding_of_an_iri() -> TestResult {
    let key = CanonicalKey::iri("http://ex.org/a")?;
    let mut out = Vec::new();
    key.encode_into(&mut out);
    let mut expected = vec![0x00, 0x00, 0x00, 0x0F];
    expected.extend_from_slice(b"http://ex.org/a");
    assert_eq!(out, expected);
    assert_eq!(key.encoded_len(), 19);
    assert_eq!(key.as_str(), "http://ex.org/a");
    Ok(())
}

#[test]
fn test_the_empty_iri_encodes_as_a_zero_length() -> TestResult {
    let key = CanonicalKey::iri("")?;
    let mut out = Vec::new();
    key.encode_into(&mut out);
    assert_eq!(out, [0x00, 0x00, 0x00, 0x00]);
    assert_eq!(key.encoded_len(), 4);
    Ok(())
}

#[test]
fn test_the_length_counts_utf8_bytes_not_characters() -> TestResult {
    // "http://ex.org/" is 14 bytes, ü 2, 日 3 and 本 3: 22 = 0x16 bytes for 17 characters.
    let iri = "http://ex.org/\u{fc}\u{65e5}\u{672c}";
    let key = CanonicalKey::iri(iri)?;
    let mut out = Vec::new();
    key.encode_into(&mut out);
    let (length, bytes) = out.split_first_chunk::<4>().context("a length")?;
    assert_eq!(length, &[0x00, 0x00, 0x00, 0x16]);
    assert_eq!(bytes, iri.as_bytes());
    assert_eq!(key.encoded_len(), 26);
    Ok(())
}

#[test]
fn test_a_length_above_255_takes_two_bytes() -> TestResult {
    // 300 = 0x012C.
    let iri = format!("http://ex.org/{}", "c".repeat(286));
    let mut out = Vec::new();
    CanonicalKey::iri(&iri)?.encode_into(&mut out);
    assert_eq!(out.get(..4), Some(&[0x00, 0x00, 0x01, 0x2C][..]));
    assert_eq!(out.len(), 304);
    Ok(())
}

#[test]
fn test_encoding_appends_to_the_buffer() -> TestResult {
    let mut out = vec![0xAA];
    CanonicalKey::iri("urn:x")?.encode_into(&mut out);
    assert_eq!(
        out,
        [0xAA, 0x00, 0x00, 0x00, 0x05, b'u', b'r', b'n', b':', b'x']
    );
    Ok(())
}

#[test]
fn test_the_largest_length_a_4_byte_count_holds_is_accepted() -> TestResult {
    // An IRI of u32::MAX bytes is the longest a 4-byte length counts. The bound is tested on the length alone, which
    // `CanonicalKey::iri` checks the same way, so no test holds a 4 GiB IRI in memory.
    let len = usize::try_from(u32::MAX).context("a 64-bit target")?;
    assert_eq!(CanonicalKey::length(len), Ok(u32::MAX));
    Ok(())
}

#[test]
fn test_a_4_gib_iri_is_refused() -> TestResult {
    // 2^32 bytes, one more than a 4-byte length counts.
    let len = usize::try_from(u64::from(u32::MAX) + 1).context("a 64-bit target")?;
    assert_eq!(
        CanonicalKey::length(len),
        Err(IdError(IdErrorEnum::IriTooLong { len: 4_294_967_296 }))
    );
    Ok(())
}
