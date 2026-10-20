/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, x/reserved_namespace_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::panic::{self, UnwindSafe};

use ultragraphdb_core_keys::{
    ROOT_NAMESPACE, ReservedNamespace, TrustMarker, is_registered_reserved_predicate,
    is_registered_reserved_type, namespace_attr, register_reserved_namespace,
    reserved_predicate_value_lock,
};

const TEST_TRUST: TrustMarker = TrustMarker::new("testTrust");

// ns namespace-attributes a bare name, the way the alter validator and
// no-schema mutation guard see predicate/type names at the call sites.
fn ns(name: &str) -> String {
    namespace_attr(ROOT_NAMESPACE, name)
}

/// The panic message of `f`, or `None` when it returns.
fn panic_message(f: impl FnOnce() + UnwindSafe) -> Option<String> {
    panic::catch_unwind(f)
        .err()
        .map(|payload| match payload.downcast::<String>() {
            Ok(s) => *s,
            Err(payload) => payload
                .downcast::<&str>()
                .map(|s| s.to_string())
                .unwrap_or_default(),
        })
}

#[test]
fn test_reserved_namespace_registry() {
    register_reserved_namespace(
        ReservedNamespace::new()
            .with_predicate_prefix("dgraph.testns.rel.")
            .with_predicates(["dgraph.testns.xid", "dgraph.testns.cfg"])
            .with_types(["dgraph.testns.node"])
            .with_value_locked(["dgraph.testns.cfg"])
            .with_trust_marker(TEST_TRUST),
    );

    // Dynamic prefix members.
    assert!(is_registered_reserved_predicate(&ns(
        "dgraph.testns.rel.owner"
    )));
    // Exact predicate members.
    assert!(is_registered_reserved_predicate(&ns("dgraph.testns.xid")));
    assert!(is_registered_reserved_predicate(&ns("dgraph.testns.cfg")));
    // Not owned: a sibling under the same prefix root, a different reserved
    // namespace, and an ordinary predicate.
    assert!(!is_registered_reserved_predicate(&ns(
        "dgraph.testns.other"
    )));
    assert!(!is_registered_reserved_predicate(&ns(
        "dgraph.graphql.schema"
    )));
    assert!(!is_registered_reserved_predicate(&ns("person.name")));

    // Types are tracked separately from predicates.
    assert!(is_registered_reserved_type(&ns("dgraph.testns.node")));
    assert!(!is_registered_reserved_type(&ns("dgraph.testns.xid"))); // a predicate, not a type
    assert!(!is_registered_reserved_type(&ns("dgraph.graphql")));

    // Value lock matches the bare predicate, like is_other_reserved_predicate.
    // cfg is locked to the registered marker; xid is owned but not locked.
    assert_eq!(
        reserved_predicate_value_lock("dgraph.testns.cfg"),
        Some(TEST_TRUST)
    );
    assert_eq!(reserved_predicate_value_lock("dgraph.testns.xid"), None);
}

// Confirms names no namespace claims are never members, so a stock build with no
// registration keeps the pristine reserved-namespace behavior (only pre-defined
// names exist under `dgraph.`).
#[test]
fn test_reserved_namespace_rejects_unregistered() {
    assert!(!is_registered_reserved_predicate(&ns(
        "dgraph.unregistered.pred"
    )));
    assert!(!is_registered_reserved_type(&ns(
        "dgraph.unregistered.type"
    )));
    assert_eq!(
        reserved_predicate_value_lock("dgraph.unregistered.pred"),
        None
    );
}

// Guards against bypassing a value lock by changing the case of an owned name:
// ownership is matched case-insensitively, so the value lock must be too.
#[test]
fn test_reserved_predicate_value_lock_case_insensitive() {
    register_reserved_namespace(
        ReservedNamespace::new()
            .with_predicates(["dgraph.casetest.Secret"])
            .with_value_locked(["dgraph.casetest.Secret"])
            .with_trust_marker(TEST_TRUST),
    );

    for p in [
        "dgraph.casetest.Secret",
        "dgraph.casetest.secret",
        "dgraph.casetest.SECRET",
    ] {
        assert_eq!(
            reserved_predicate_value_lock(p),
            Some(TEST_TRUST),
            "value lock must hold regardless of case: {p:?}"
        );
    }
    assert!(is_registered_reserved_predicate(&ns(
        "DGRAPH.CASETEST.SECRET"
    )));
}

// Confirms the invariant is enforced at registration (init time): ValueLocked
// without a TrustMarker would make the predicate unwritable by everyone, so it
// panics rather than failing silently at mutation time.
#[test]
fn test_register_reserved_namespace_requires_trust_marker() {
    let message = panic_message(|| {
        register_reserved_namespace(
            ReservedNamespace::new()
                .with_predicates(["dgraph.nomarker.cfg"])
                .with_value_locked(["dgraph.nomarker.cfg"]), // TrustMarker intentionally left unset.
        );
    });
    assert_eq!(
        message.as_deref(),
        Some(
            "x.RegisterReservedNamespace: ValueLocked is set but TrustMarker is nil; a value-locked predicate with no \
             TrustMarker is unwritable by everyone, including its owner"
        )
    );
}

// Confirms a namespace-qualified name is rejected at registration. The value-lock
// guard matches the bare predicate, so a qualified entry would never match and the
// predicate would stay publicly writable — it must fail closed at startup instead.
#[test]
fn test_register_reserved_namespace_rejects_qualified_name() {
    let message = panic_message(|| {
        register_reserved_namespace(
            ReservedNamespace::new()
                .with_predicates(["dgraph.qualtest.secret"])
                .with_value_locked([namespace_attr(ROOT_NAMESPACE, "dgraph.qualtest.secret")])
                .with_trust_marker(TEST_TRUST),
        );
    });
    assert_eq!(
        message.as_deref(),
        Some(
            "x.RegisterReservedNamespace: name \"0-dgraph.qualtest.secret\" must be bare, without the \"-\" namespace \
             separator"
        )
    );
}

// Confirms a name claimed twice panics rather than silently overwriting (for value
// locks, last-writer-wins would let import order pick the TrustMarker).
#[test]
fn test_register_reserved_namespace_rejects_duplicate() {
    let message = panic_message(|| {
        register_reserved_namespace(ReservedNamespace::new().with_predicates(["dgraph.duptest.x"]));
        register_reserved_namespace(ReservedNamespace::new().with_predicates(["dgraph.duptest.x"]));
    });
    assert_eq!(
        message.as_deref(),
        Some("x.RegisterReservedNamespace: predicate \"dgraph.duptest.x\" already registered")
    );
}

#[test]
fn test_register_reserved_namespace_rejects_duplicate_type_and_value_lock() {
    let message = panic_message(|| {
        register_reserved_namespace(ReservedNamespace::new().with_types(["dgraph.duptype.T"]));
        register_reserved_namespace(ReservedNamespace::new().with_types(["dgraph.duptype.t"]));
    });
    assert_eq!(
        message.as_deref(),
        Some("x.RegisterReservedNamespace: type \"dgraph.duptype.t\" already registered")
    );

    let lock = || {
        ReservedNamespace::new()
            .with_value_locked(["dgraph.duplock.x"])
            .with_trust_marker(TEST_TRUST)
    };
    let message = panic_message(|| {
        register_reserved_namespace(lock());
        register_reserved_namespace(lock());
    });
    assert_eq!(
        message.as_deref(),
        Some(
            "x.RegisterReservedNamespace: value-locked predicate \"dgraph.duplock.x\" already registered"
        )
    );
}

// Covers locking dynamically-named predicates by prefix. A namespace whose
// predicates are created one per (namespace, relation) at runtime cannot enumerate
// them in ValueLocked, so without prefix locking they would be creatable via Alter
// yet writable by anyone through /mutate.
#[test]
fn test_value_locked_prefixes() {
    register_reserved_namespace(
        ReservedNamespace::new()
            .with_predicate_prefix("dgraph.prefixlock.rel.")
            .with_predicates(["dgraph.prefixlock.xid"])
            .with_value_locked_prefixes(["dgraph.prefixlock.rel."])
            .with_trust_marker(TEST_TRUST),
    );

    // Any predicate under the locked prefix, including ones that do not exist
    // yet — that is the point of locking by prefix.
    for p in [
        "dgraph.prefixlock.rel.document.owner",
        "dgraph.prefixlock.rel.group.member",
        "dgraph.prefixlock.rel.",
        "dgraph.prefixlock.REL.Document.Owner", // case-insensitive, like exact names
    ] {
        assert_eq!(
            reserved_predicate_value_lock(p),
            Some(TEST_TRUST),
            "prefix lock must cover {p:?}"
        );
    }

    // Owned but deliberately not locked: xid stays writable so admin tooling and
    // migrations can create nodes.
    assert_eq!(reserved_predicate_value_lock("dgraph.prefixlock.xid"), None);

    // A near miss outside the prefix is not locked.
    assert_eq!(
        reserved_predicate_value_lock("dgraph.prefixlock.relative"),
        None
    );
}

// Pins that an exact value lock takes precedence over a prefix that also matches.
//
// It registers the prefix and the exact name in SEPARATE namespaces with distinct
// markers, which is both the only way the precedence is observable and the only way
// the split is expressible: a ReservedNamespace carries one TrustMarker.
#[test]
fn test_value_locked_exact_wins_over_prefix() {
    let prefix_trust = TrustMarker::new("prefixTrust");
    let exact_trust = TrustMarker::new("exactTrust");

    // The namespace that owns the whole sub-namespace by prefix.
    register_reserved_namespace(
        ReservedNamespace::new()
            .with_predicate_prefix("dgraph.precedence.rel.")
            .with_value_locked_prefixes(["dgraph.precedence.rel."])
            .with_trust_marker(prefix_trust),
    );
    // A second namespace pinning one predicate inside it to its own marker. Note
    // there is no cross-kind conflict check, so this registration is accepted — the
    // precedence rule is what decides the overlap.
    register_reserved_namespace(
        ReservedNamespace::new()
            .with_predicates(["dgraph.precedence.rel.special"])
            .with_value_locked(["dgraph.precedence.rel.special"])
            .with_trust_marker(exact_trust),
    );

    assert_eq!(
        reserved_predicate_value_lock("dgraph.precedence.rel.special"),
        Some(exact_trust),
        "the exact entry must win over the prefix that also matches"
    );
    assert_eq!(
        reserved_predicate_value_lock("dgraph.precedence.rel.ordinary"),
        Some(prefix_trust),
        "a predicate with no exact entry falls to the prefix owner"
    );
}

// Covers the guard that PredicatePrefix has and value locks did not. An empty
// prefix matches every predicate in the cluster.
#[test]
fn test_value_locked_prefix_rejects_empty() {
    let message = panic_message(|| {
        register_reserved_namespace(
            ReservedNamespace::new()
                .with_predicate_prefix("dgraph.emptylock.")
                .with_value_locked_prefixes([""])
                .with_trust_marker(TrustMarker::new("emptyTrust")),
        );
    });
    assert_eq!(
        message.as_deref(),
        Some("x.RegisterReservedNamespace: value-locked prefix must not be empty")
    );
}

// Pins that when two namespaces lock overlapping prefixes with different markers,
// the longest (most specific) prefix governs a predicate whatever the registration
// order. Otherwise a holder of the broader marker could write under the narrower
// prefix its owner meant to guard (CWE-863), with import order silently deciding
// which marker wins.
#[test]
fn test_value_locked_longest_prefix_wins() {
    let broad_trust = TrustMarker::new("broadTrust");
    let narrow_trust = TrustMarker::new("narrowTrust");

    // The broad prefix registers FIRST, so a first-match lookup would return it.
    register_reserved_namespace(
        ReservedNamespace::new()
            .with_predicate_prefix("dgraph.longestpfx.rel.")
            .with_value_locked_prefixes(["dgraph.longestpfx.rel."])
            .with_trust_marker(broad_trust),
    );
    // A second namespace locks a narrower sub-prefix to its own marker.
    register_reserved_namespace(
        ReservedNamespace::new()
            .with_value_locked_prefixes(["dgraph.longestpfx.rel.secret."])
            .with_trust_marker(narrow_trust),
    );

    // Under both prefixes, the more specific one governs.
    assert_eq!(
        reserved_predicate_value_lock("dgraph.longestpfx.rel.secret.key"),
        Some(narrow_trust),
        "the longest matching prefix must win, not the first registered"
    );

    // Under the broad prefix only, the broad owner governs.
    assert_eq!(
        reserved_predicate_value_lock("dgraph.longestpfx.rel.owner"),
        Some(broad_trust)
    );
}

// Mirrors the ValueLocked invariant: a locked prefix with no marker would be
// unwritable by everyone, including its owner, so it must panic at registration.
#[test]
fn test_value_locked_prefixes_require_trust_marker() {
    assert!(
        panic_message(|| {
            register_reserved_namespace(
                ReservedNamespace::new()
                    .with_predicate_prefix("dgraph.prefixnomarker.rel.")
                    .with_value_locked_prefixes(["dgraph.prefixnomarker.rel."]), // TrustMarker intentionally left unset.
            );
        })
        .is_some()
    );
}

// Confirms a namespace-qualified prefix is rejected, for the same reason a
// qualified exact name is: the guard matches the bare predicate, so it would never
// fire and the predicates would stay writable.
#[test]
fn test_value_locked_prefixes_reject_qualified() {
    assert!(
        panic_message(|| {
            register_reserved_namespace(
                ReservedNamespace::new()
                    .with_value_locked_prefixes([namespace_attr(
                        ROOT_NAMESPACE,
                        "dgraph.qualprefix.rel.",
                    )])
                    .with_trust_marker(TEST_TRUST),
            );
        })
        .is_some()
    );
}

// Confirms two namespaces cannot claim the same locked prefix, since import order
// would silently pick the TrustMarker.
#[test]
fn test_value_locked_prefixes_reject_duplicate() {
    let message = panic_message(|| {
        let plugin = || {
            ReservedNamespace::new()
                .with_value_locked_prefixes(["dgraph.dupprefix.rel."])
                .with_trust_marker(TEST_TRUST)
        };
        register_reserved_namespace(plugin());
        register_reserved_namespace(plugin());
    });
    assert_eq!(
        message.as_deref(),
        Some(
            "x.RegisterReservedNamespace: value-locked prefix \"dgraph.dupprefix.rel.\" already registered"
        )
    );
}

// A panic while the registry is held leaves it usable for later registrations and lookups.
#[test]
fn test_registry_survives_a_panicking_registration() {
    assert!(
        panic_message(|| {
            register_reserved_namespace(
                ReservedNamespace::new().with_predicates(["dgraph.poison.a", "dgraph.poison.a"]),
            );
        })
        .is_some()
    );
    // The first claim landed before the duplicate panicked, as in Dgraph.
    assert!(is_registered_reserved_predicate(&ns("dgraph.poison.a")));
    register_reserved_namespace(ReservedNamespace::new().with_types(["dgraph.poison.T"]));
    assert!(is_registered_reserved_type(&ns("dgraph.poison.t")));
}
