/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{Literal, NamedNode};
use ultragraphdb_core_ids::{Bucket, Sequence, SystemId, TypePrefix};
use ultragraphdb_core_terms::{
    Datatype, EncodedTerm, Hash128, TaggedDatatype, TermError, TermView, Text,
};
use ultragraphdb_tool_bdd::TestResult;

fn id(sequence: u64) -> TestResult<SystemId> {
    Ok(SystemId::new(
        TypePrefix::RESOURCE,
        Bucket::new(7)?,
        Sequence::new(sequence)?,
    ))
}

#[test]
fn test_an_id_is_no_literal() -> TestResult {
    let refused = TermView::Id(id(5)?).to_literal(
        |_| Ok::<_, TermError>(NamedNode::new_unchecked("http://e/x")),
        |_| Ok(String::new()),
    );
    assert_eq!(refused, Err(TermError::not_a_literal(id(5)?)));
    Ok(())
}

#[test]
fn test_a_resolver_failure_is_returned() -> TestResult {
    #[derive(Debug, PartialEq)]
    struct Missing(u64);
    impl From<TermError> for Missing {
        fn from(_: TermError) -> Self {
            Missing(0)
        }
    }
    let view = TermView::Typed {
        datatype: Datatype::Id(id(9)?),
        lexical: Text::Inline("x"),
    };
    assert_eq!(
        view.to_literal(
            |id| Err(Missing(id.sequence().get())),
            |_| Ok(String::new())
        ),
        Err(Missing(9))
    );
    let hashed = TermView::String(Text::Hashed(Hash128::of(b"gone")));
    assert_eq!(
        hashed.to_literal(|_| Err(Missing(1)), |_| Err(Missing(2))),
        Err(Missing(2))
    );
    Ok(())
}

/// A datatype a Result names by its IRI's hash reads as the IRI that hash names, refused when that is no IRI.
#[test]
fn test_a_datatype_by_hash_is_the_iri_its_hash_names() -> TestResult {
    let iri = "http://e/celsius";
    let view = TermView::Typed {
        datatype: Datatype::Hashed(Hash128::of(iri.as_bytes())),
        lexical: Text::Inline("21.5"),
    };
    assert_eq!(
        view.to_literal(
            |id| Err(TermError::not_a_literal(id)),
            |_| Ok(iri.to_string())
        )?,
        Literal::new_typed_literal("21.5", NamedNode::new(iri)?)
    );
    let no_iri = "no IRI";
    assert_eq!(
        view.to_literal(
            |id| Err(TermError::not_a_literal(id)),
            |_| Ok(no_iri.to_string())
        ),
        Err(TermError::malformed(
            no_iri.as_bytes().to_vec(),
            "a datatype that is no IRI"
        ))
    );
    Ok(())
}

/// A lexical form of codes 34 to 36 is a literal of that datatype's IRI (`PROTOCOL.md` §10.4).
#[test]
fn test_a_lexical_form_of_codes_34_to_36_is_a_literal_of_its_datatype() -> TestResult {
    let view = TermView::Typed {
        datatype: Datatype::Tagged(TaggedDatatype::Measure),
        lexical: Text::Inline("1.5 m"),
    };
    let literal = view.to_literal(
        |_| Ok::<_, TermError>(NamedNode::new_unchecked("http://e/x")),
        |_| Ok(String::new()),
    )?;
    assert_eq!(
        literal,
        Literal::new_typed_literal(
            "1.5 m",
            NamedNode::new_unchecked("https://w3id.org/ultragraphdb/ns/measure")
        )
    );
    Ok(())
}

/// A value of codes 34 to 36 held by value, under tags `0x41`, `0x50` and `0x51`, is refused as a literal, naming its
/// code: the mapping writes no lexical form of it.
#[test]
fn test_a_value_of_codes_34_to_36_by_value_names_its_code() -> TestResult {
    let encodings: [(Vec<u8>, u8); 3] = [
        (vec![0x41, 1, 0, 0x80, 0x3F], 34),
        (
            [&[0x50][..], &1.5f64.to_be_bytes(), &[1], b"m"].concat(),
            35,
        ),
        (vec![0x51, 9], 36),
    ];
    for (bytes, code) in encodings {
        let term = EncodedTerm::from_bytes(&bytes)?;
        let refused = term.result_view()?.to_literal(
            |_| Ok::<_, TermError>(NamedNode::new_unchecked("http://e/x")),
            |_| Ok(String::new()),
        );
        assert_eq!(
            refused,
            Err(TermError::unknown_datatype(code)),
            "{bytes:02x?}"
        );
    }
    Ok(())
}
