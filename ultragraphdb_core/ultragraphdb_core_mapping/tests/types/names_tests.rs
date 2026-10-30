/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_mapping::{MappingError, Names};

#[test]
fn test_names_map_to_iris_under_the_base_and_back() {
    let names = Names::new("http://ex.org/").unwrap();
    assert_eq!(
        names.iri("age"),
        NamedNode::new_unchecked("http://ex.org/age")
    );
    assert_eq!(names.name(&names.iri("age")), "age");
    let other = NamedNode::new_unchecked("http://other.org/age");
    assert_eq!(
        names.name(&other),
        "http://other.org/age",
        "an IRI elsewhere is its own name"
    );
    assert_eq!(names.iri("http://other.org/age"), other);
    assert_eq!(
        names.name(&NamedNode::new_unchecked("http://ex.org/")),
        "http://ex.org/"
    );
    assert_eq!(
        names.node_iri(7),
        NamedNode::new_unchecked("http://ex.org/node/7")
    );
    assert_eq!(names.base(), "http://ex.org/");
    assert_eq!(
        names.skolem_iri(9),
        NamedNode::new_unchecked("http://ex.org/.well-known/genid/9")
    );
}

#[test]
fn test_an_iri_under_the_base_holding_a_colon_is_named_by_itself() {
    let names = Names::new("http://ex.org/").unwrap();
    // Its name would read back as an absolute IRI, "a:b", not as the IRI it came from.
    let colon = NamedNode::new_unchecked("http://ex.org/a:b");
    assert_eq!(names.name(&colon), "http://ex.org/a:b");
    assert_eq!(names.iri(&names.name(&colon)), colon);
    // A path under the base is a name, and maps back.
    let nested = NamedNode::new_unchecked("http://ex.org/x/y");
    assert_eq!(names.name(&nested), "x/y");
    assert_eq!(names.iri("x/y"), nested);
}

/// The refusal of `base` for `reason`.
fn refused(base: &str, reason: &'static str) -> Result<Names, MappingError> {
    Err(MappingError::InvalidBase(base.to_string(), reason))
}

#[test]
fn test_a_base_that_is_no_absolute_iri_is_refused() {
    // RFC 3987's absolute-IRI begins with a scheme and ":"; none of these does, or the text is no IRI at all.
    for base in [
        "",
        "ex.org/",
        "/ns/",
        "//ex.org/",
        "http//ex.org/",
        "1http://ex.org/",
        // A space is no IRI character (RFC 3987 section 2.2).
        "http://ex.org/a b/",
    ] {
        assert_eq!(
            Names::new(base),
            refused(base, "not an absolute IRI"),
            "{base:?}"
        );
    }
}

#[test]
fn test_a_base_with_a_query_or_a_fragment_is_refused() {
    // A name appended to such a base lands in its query or fragment, and the skolem IRI's `/.well-known/genid/` path
    // (PROTOCOL.md sections 4.4 and 10.2) with it.
    for base in [
        "http://ex.org/ns#",
        "http://ex.org/a#b/",
        "http://ex.org/?q=/",
    ] {
        assert_eq!(
            Names::new(base),
            refused(base, "has a query or a fragment"),
            "{base:?}"
        );
    }
}

#[test]
fn test_a_base_not_ending_in_a_slash_is_refused() {
    // A name appended to it would run into its last segment: "http://ex.org/ns" and "age" give ".../nsage".
    for base in ["http://ex.org", "http://ex.org/ns", "urn:ex:ns", "http:"] {
        assert_eq!(
            Names::new(base),
            refused(base, "does not end in '/'"),
            "{base:?}"
        );
    }
}

#[test]
fn test_an_absolute_base_ending_in_a_slash_names_iris_under_it() {
    for base in [
        "http://ex.org/",
        "urn:ex:ns/",
        "file:///",
        // The design's default base of namespace 0, under the system vocabulary.
        "https://w3id.org/ultragraphdb/ns/namespace/0/",
    ] {
        let names = Names::new(base).unwrap();
        assert_eq!(names.base(), base);
        assert_eq!(names.iri("age").as_str(), format!("{base}age"));
        assert_eq!(names.name(&names.iri("age")), "age");
    }
    // PROTOCOL.md section 10.2's skolem IRI of namespace 0's node 6.
    assert_eq!(
        Names::new("https://w3id.org/ultragraphdb/ns/namespace/0/")
            .unwrap()
            .skolem_iri(6)
            .as_str(),
        "https://w3id.org/ultragraphdb/ns/namespace/0/.well-known/genid/6"
    );
}

#[test]
fn test_a_unicode_base_and_unicode_names_are_iris() {
    // RFC 3987 takes non-ASCII characters (ucschar) unescaped, in the host and the path alike.
    let names = Names::new("http://例え.jp/名前/").unwrap();
    assert_eq!(names.iri("年齢").as_str(), "http://例え.jp/名前/年齢");
    assert_eq!(names.name(&names.iri("年齢")), "年齢");
    assert_eq!(
        names.skolem_iri(6).as_str(),
        "http://例え.jp/名前/.well-known/genid/6"
    );
    // The same rules hold for a unicode base: one without its trailing slash is refused.
    assert_eq!(
        Names::new("http://例え.jp/名前"),
        refused("http://例え.jp/名前", "does not end in '/'")
    );
}
