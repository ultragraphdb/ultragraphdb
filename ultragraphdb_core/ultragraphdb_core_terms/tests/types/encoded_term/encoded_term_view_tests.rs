/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_terms::{
    Datatype, EncodedTerm, Hash128, TaggedDatatype, TermError, TermView, Text,
};
use ultragraphdb_tool_bdd::TestResult;

/// A datatype no known-datatype code names.
const DATATYPE: &str = "http://ex.org/celsius";
/// A lexical form short enough to be held inline.
const SHORT: &str = "21.5";
/// A lexical form held by its hash.
const LONG: &str = "a lexical form longer than fifteen bytes";

/// A literal of `DATATYPE` as tags `0x32` and `0x33` hold it, the datatype named by `datatype`'s 16 bytes: the short
/// form inline, the long one by its hash.
fn typed(datatype: &[u8; 16]) -> TestResult<[EncodedTerm; 2]> {
    let short = [
        &[0x32][..],
        datatype,
        &[SHORT.len() as u8],
        SHORT.as_bytes(),
    ]
    .concat();
    let long = [
        &[0x33][..],
        datatype,
        Hash128::of(LONG.as_bytes()).as_bytes(),
    ]
    .concat();
    Ok([
        EncodedTerm::from_bytes(&short)?,
        EncodedTerm::from_bytes(&long)?,
    ])
}

/// A Result names a datatype no code names by its IRI's hash (`PROTOCOL.md` §10.1): its reader takes the 16 bytes as
/// that hash, where the store's reader takes them as a system ID and refuses a hash that is no valid one.
#[test]
fn test_a_results_reader_takes_a_datatype_by_its_hash() -> TestResult {
    let hash = Hash128::of(DATATYPE.as_bytes());
    assert!(SystemId::decode(hash.as_bytes()).is_err());
    let [short, long] = typed(hash.as_bytes())?;
    for (term, lexical) in [
        (&short, Text::Inline(SHORT)),
        (&long, Text::Hashed(Hash128::of(LONG.as_bytes()))),
    ] {
        assert_eq!(
            term.result_view()?,
            TermView::Typed {
                datatype: Datatype::Hashed(hash),
                lexical,
            }
        );
        assert_eq!(
            term.view(),
            Err(TermError::malformed(
                term.as_bytes().to_vec(),
                "an ID that is no valid system ID"
            ))
        );
    }
    Ok(())
}

/// The store names a datatype no code names by its IRI's system ID: its reader takes the 16 bytes as that ID, and a
/// Result's reader, given the same bytes, as a hash.
#[test]
fn test_the_stores_reader_takes_a_datatype_by_its_id() -> TestResult {
    let id = SystemId::new(TypePrefix::RESOURCE, Bucket::new(7)?, Sequence::new(9)?);
    let [short, _] = typed(&id.to_be_bytes())?;
    assert_eq!(
        short.view()?,
        TermView::Typed {
            datatype: Datatype::Id(id),
            lexical: Text::Inline(SHORT),
        }
    );
    assert_eq!(
        short.result_view()?,
        TermView::Typed {
            datatype: Datatype::Hashed(Hash128::from_bytes(id.to_be_bytes())),
            lexical: Text::Inline(SHORT),
        }
    );
    Ok(())
}

/// A Result names an IRI or a blank node by its hash, which no view reads; every other encoding a Result's reader reads
/// as the store's does.
#[test]
fn test_a_results_reader_refuses_an_iri_and_reads_other_literals_as_the_store() -> TestResult {
    let id = EncodedTerm::id(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(7)?,
        Sequence::new(9)?,
    ));
    assert_eq!(
        id.result_view(),
        Err(TermError::malformed(
            id.as_bytes().to_vec(),
            "an IRI or a blank node, which a Result names by its hash"
        ))
    );
    let string = EncodedTerm::from_bytes(
        &[[0x20, SHORT.len() as u8].as_slice(), SHORT.as_bytes()].concat(),
    )?;
    assert_eq!(string.result_view()?, string.view()?);
    Ok(())
}

/// Every encoding the check accepts is read (`PROTOCOL.md` §10.1, §10.5): tag `0x50`, a measure, its binary64 number
/// big-endian (1.5 is `3F F8 00 00 00 00 00 00`), then its unit's code inline; tag `0x51`, a precision, one byte of 1
/// (millennium) to 12 (nanosecond). A Result's reader and the store's read them alike.
#[test]
fn test_a_measure_and_a_precision_are_read_by_value() -> TestResult {
    let measure =
        EncodedTerm::from_bytes(&[&[0x50][..], &1.5f64.to_be_bytes(), &[3], b"m/s"].concat())?;
    let expected = TermView::Measure {
        number: 1.5,
        unit: "m/s",
    };
    assert_eq!(measure.result_view()?, expected);
    assert_eq!(measure.view()?, expected);
    for precision in [1, 9, 12] {
        let term = EncodedTerm::from_bytes(&[0x51, precision])?;
        assert_eq!(term.result_view()?, TermView::Precision(precision));
        assert_eq!(term.view()?, TermView::Precision(precision));
    }
    Ok(())
}

/// Tags `0x30` and `0x31` naming codes 34 to 36 (§10.4) hold the lexical form of `ugdb:bfloat16vector`,
/// `ugdb:measure` or `ugdb:precision`, inline or by its hash: read as a typed literal of that datatype.
#[test]
fn test_codes_34_to_36_are_read_as_typed_literals() -> TestResult {
    let long = "[1, 2, 3, 4, 5, 6, 7, 8]";
    let cases = [
        (
            [&[0x30, 35, 5][..], b"1.5 m"].concat(),
            TaggedDatatype::Measure,
            Text::Inline("1.5 m"),
        ),
        (
            [&[0x30, 36, 6][..], b"second"].concat(),
            TaggedDatatype::Precision,
            Text::Inline("second"),
        ),
        (
            [&[0x31, 34][..], Hash128::of(long.as_bytes()).as_bytes()].concat(),
            TaggedDatatype::Bfloat16Vector,
            Text::Hashed(Hash128::of(long.as_bytes())),
        ),
    ];
    for (bytes, datatype, lexical) in cases {
        let term = EncodedTerm::from_bytes(&bytes)?;
        let expected = TermView::Typed {
            datatype: Datatype::Tagged(datatype),
            lexical,
        };
        assert_eq!(term.result_view()?, expected, "{bytes:02x?}");
        assert_eq!(term.view()?, expected, "{bytes:02x?}");
    }
    Ok(())
}
