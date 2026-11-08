/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_terms::{Datatype, EncodedTerm, Hash128, TermError, TermView, Text};
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
