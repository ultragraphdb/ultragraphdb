/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/encoding/json/v2_stream_test.go (TestDecoder, TestDecoderBuffered, TestBlocking, the
 * Decode cases of TestDecodeInStream) and v2_decode_test.go (TestUnmarshalErrorAfterMultipleJSON, TestUnmarshal's
 * Decoder errors). Changed for ultragraphdb: translated to Rust; numbers decode as with UseNumber; reads of one
 * byte at a time added.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::io::Read;

use ultragraphdb_common_utils::{JsonDecoder, JsonValue};
use ultragraphdb_error_json::{JsonError, JsonErrorEnum};

use crate::utils::go_json_tests::Want::Syntax;
use crate::utils::go_json_tests::{VECTORS, Want, matches};

/// A reader handing out one byte per read, as Go's `iotest.OneByteReader`.
struct OneByte<'a>(&'a [u8]);

impl Read for OneByte<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match (self.0.split_first(), buf.first_mut()) {
            (Some((&b, rest)), Some(slot)) => {
                *slot = b;
                self.0 = rest;
                Ok(1)
            }
            _ => Ok(0),
        }
    }
}

/// A reader holding its data once and failing after it, where Go's pipe would block.
struct Once(Option<Vec<u8>>);

impl Read for Once {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self.0.take() {
            Some(data) => {
                buf[..data.len()].copy_from_slice(&data);
                Ok(data.len())
            }
            None => Err(std::io::Error::other("read past the value")),
        }
    }
}

#[test]
fn decode_matches_go() {
    for &(input, _, first, offset, second) in VECTORS {
        let mut dec = JsonDecoder::new(input);
        let got = dec.decode();
        let name = String::from_utf8_lossy(input);
        assert!(matches(&got, first), "{name:?}: {got:?}, want {first:?}");
        assert_eq!(dec.input_offset(), offset, "{name:?}");
        let got = dec.decode();
        assert!(
            matches(&got, second),
            "{name:?}: second {got:?}, want {second:?}"
        );
    }
}

/// Recorded from Go 1.27 over `iotest.OneByteReader`: the inputs whose results differ from those over a
/// `bytes.Buffer`. A truncated `\u` escape is judged by the bytes buffered so far, and a character by its bytes
/// buffered, so where the input is cut shows in the error.
#[rustfmt::skip]
const ONE_BYTE: &[(&[u8], Want, i64, Want)] = &[
    (b"\"\\u12x4\"", Syntax("invalid escape sequence `\\u12x` in string", 6), 0, Syntax("invalid escape sequence `\\u12x` in string", 6)),
    (b"\xef\xbb\xbf{}", Syntax("invalid character '\\xef' looking for beginning of value", 1), 0, Syntax("invalid character '\\xef' looking for beginning of value", 1)),
    (b"\xc2\xa0{}", Syntax("invalid character '\\xc2' looking for beginning of value", 1), 0, Syntax("invalid character '\\xc2' looking for beginning of value", 1)),
    (b"{\"a\":1}\xc3\xa9", Want::Ok("{\"a\":1}"), 7, Syntax("invalid character '\\xc3' looking for beginning of value", 8)),
    (b"{\"a\":\"\\u12 4\"}", Syntax("invalid escape sequence \"\\\\u12 \" in string", 11), 0, Syntax("invalid escape sequence \"\\\\u12 \" in string", 11)),
    (b"{\"a\":\"\\u1\x01\"}", Syntax("invalid escape sequence \"\\\\u1\\x01\" in string", 10), 0, Syntax("invalid escape sequence \"\\\\u1\\x01\" in string", 10)),
];

#[test]
fn decode_one_byte_at_a_time_matches_go() {
    for &(input, _, first, offset, second) in VECTORS {
        let (first, offset, second) = ONE_BYTE
            .iter()
            .find(|o| o.0 == input)
            .map_or((first, offset, second), |o| (o.1, o.2, o.3));
        let mut dec = JsonDecoder::new(OneByte(input));
        let name = String::from_utf8_lossy(input);
        let got = dec.decode();
        assert!(matches(&got, first), "{name:?}: {got:?}, want {first:?}");
        assert_eq!(dec.input_offset(), offset, "{name:?}");
        let got = dec.decode();
        assert!(
            matches(&got, second),
            "{name:?}: second {got:?}, want {second:?}"
        );
    }
}

#[test]
fn test_decoder() {
    let stream = [
        "0.1",
        r#""hello""#,
        "null",
        "true",
        "false",
        r#"["a","b","c"]"#,
        r#"{"ß":"long s","K":"Kelvin"}"#,
        "3.14",
    ];
    let want = [
        "0.1",
        r#""hello""#,
        "null",
        "true",
        "false",
        r#"["a","b","c"]"#,
        r#"{"K":"Kelvin","ß":"long s"}"#,
        "3.14",
    ];
    for i in 0..=stream.len() {
        // The stream without newlines: the test input does not hold back-to-back numbers.
        let input = stream[..i].concat();
        let mut dec = JsonDecoder::new(input.as_bytes());
        for (j, w) in want[..i].iter().enumerate() {
            let v = dec
                .decode()
                .unwrap_or_else(|e| panic!("decode #{j}/{i}: {e}"));
            assert_eq!(v.marshal(), *w, "#{j}/{i}");
        }
    }
}

#[test]
fn test_decoder_buffered() {
    let mut dec = JsonDecoder::new(r#"{"Name": "Gopher"} extra "#.as_bytes());
    let v = dec.decode().unwrap();
    assert_eq!(v.marshal(), r#"{"Name":"Gopher"}"#);
    assert_eq!(dec.buffered(), b" extra ");
}

#[test]
fn test_blocking() {
    for input in [r#"{"x": 1}"#, "[1, 2, 3]"] {
        // A read past what was written would block Go's pipe; here it fails.
        let mut dec = JsonDecoder::new(Once(Some(input.as_bytes().to_vec())));
        assert!(dec.decode().is_ok(), "{input}");
    }
}

#[test]
fn test_decode_in_stream_errors() {
    for (input, msg, offset) in [
        (
            r#"{ "\a" }"#,
            "invalid escape sequence `\\a` in string",
            r#"{ "\a"#.len(),
        ),
        (
            r#" \a"#,
            "invalid character '\\\\' looking for beginning of value",
            r#" \"#.len(),
        ),
        (
            ",",
            "invalid character ',' looking for beginning of value",
            1,
        ),
    ] {
        let mut dec = JsonDecoder::new(input.as_bytes());
        assert_eq!(
            dec.decode().unwrap_err(),
            JsonError::Syntax(msg.to_string(), offset as i64),
            "{input}"
        );
    }
}

#[test]
fn test_unmarshal_error_after_multiple_json() {
    for (input, msg, upto) in [
        (
            "1 false null :",
            "invalid character ':' looking for beginning of value",
            "1 false null :",
        ),
        (
            "1 [] [,]",
            "invalid character ',' looking for beginning of value",
            "1 [] [,",
        ),
        (
            "1 [] [true:]",
            "invalid character ':' after array element",
            "1 [] [true:",
        ),
        (
            r#"1  {}    {"x"=}"#,
            "invalid character '=' after object key",
            r#"1  {}    {"x"="#,
        ),
        (
            "falsetruenul#",
            "invalid character '#' in literal null (expecting 'l')",
            "falsetruenul#",
        ),
    ] {
        let mut dec = JsonDecoder::new(input.as_bytes());
        let err = loop {
            if let Err(e) = dec.decode() {
                break e;
            }
        };
        assert_eq!(
            err,
            JsonError::Syntax(msg.to_string(), upto.len() as i64),
            "{input}"
        );
    }
}

/// `TestUnmarshal` reads every syntax error through a Decoder as well; truncated input is `io.ErrUnexpectedEOF`
/// there, and input of nothing but whitespace `io.EOF`.
#[test]
fn test_unmarshal_decoder_errors() {
    for (input, want) in [
        ("", Want::Eof),
        (" \n\r\t", Want::Eof),
        ("[2, 3", Want::UnexpectedEof),
        (
            r#"{"F3": -}"#,
            Want::Syntax("invalid character '}' in numeric literal", 9),
        ),
        (
            r#"{"X":12x}"#,
            Want::Syntax("invalid character 'x' after object key:value pair", 8),
        ),
    ] {
        let got = JsonDecoder::new(input.as_bytes()).decode();
        assert!(matches(&got, want), "{input:?}: {got:?}, want {want:?}");
    }
}

#[test]
fn a_syntax_error_is_returned_again() {
    let mut dec = JsonDecoder::new("1 x 2".as_bytes());
    assert_eq!(dec.decode().unwrap(), JsonValue::Number("1".to_string()));
    let err = dec.decode().unwrap_err();
    assert_eq!(dec.decode().unwrap_err(), err);
}

#[test]
fn a_failing_reader_is_its_error() {
    let mut dec = JsonDecoder::new(Once(Some(b"[1,".to_vec())));
    assert_eq!(
        dec.decode().unwrap_err().0,
        JsonErrorEnum::Read("read past the value".to_string())
    );
}

#[test]
fn check_eof_refuses_anything_but_whitespace_after_the_value() {
    let mut dec = JsonDecoder::new("{} \n".as_bytes());
    dec.decode().unwrap();
    assert_eq!(dec.check_eof(), Ok(()));
    for (input, msg, offset) in [
        ("{} x", "invalid character 'x' after top-level value", 4),
        ("[1] [2]", "invalid character '[' after top-level value", 5),
        ("1 é", "invalid character 'é' after top-level value", 4),
        (
            "null\u{0}",
            "invalid character '\\x00' after top-level value",
            5,
        ),
    ] {
        let mut dec = JsonDecoder::new(input.as_bytes());
        dec.decode().unwrap();
        assert_eq!(
            dec.check_eof(),
            Err(JsonError::Syntax(msg.to_string(), offset)),
            "{input}"
        );
    }
}

#[test]
fn check_eof_names_a_character_the_buffer_cut_whole() {
    // The first read takes 64 bytes: the value ends at byte 63, and é's second byte is not read yet.
    let input = format!(r#"{{"a":"{}"}}é"#, "x".repeat(55));
    assert_eq!(input.find('é'), Some(63));
    for input in [input.as_bytes(), b"{} \xc3"] {
        let mut dec = JsonDecoder::new(input);
        dec.decode().unwrap();
        let want = if input.len() > 4 {
            JsonError::Syntax(
                "invalid character 'é' after top-level value".to_string(),
                65,
            )
        } else {
            // A byte no continuation follows is named alone.
            JsonError::Syntax(
                "invalid character '\\xc3' after top-level value".to_string(),
                4,
            )
        };
        assert_eq!(dec.check_eof(), Err(want));
    }
}

#[test]
fn a_long_value_spans_several_reads() {
    let long = format!(r#"{{"{}":[{}1]}}"#, "k".repeat(5000), "1,".repeat(3000));
    let mut dec = JsonDecoder::new(OneByte(long.as_bytes()));
    let v = dec.decode().unwrap();
    assert_eq!(v.marshal(), long);
    assert_eq!(dec.input_offset(), long.len() as i64);
    assert_eq!(dec.decode().unwrap_err(), JsonError::Eof());
}
