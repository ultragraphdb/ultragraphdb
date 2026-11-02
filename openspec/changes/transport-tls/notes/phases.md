# transport-tls: the five phases

The record `openspec/protocols/tdd-protocol.md` asks for, for `ultragraphdb_transport_tls`.

## Phase 1: the surface (2026-11-02)

The public surface compiles with `unimplemented!()` bodies under `#[expect(clippy::unimplemented, reason = "phase 1
surface")]`, with `unused_variables`, `clippy::needless_pass_by_value` and `dead_code` expected where a body that
uses nothing leaves a parameter or field unused: `TlsError` with its four constructors and `Display`, `TlsTrust`
(`system`, `system_from`, `from_pem_file`, `from_pem`, `len`, `is_empty`, `source`), `TlsConnector` (`new`, `trust`,
`connect`), `TlsStream`, `DEFAULT_CA_BUNDLE`, `CA_BUNDLE_KEY`. Clippy with `-D warnings` over the Ultra lint set:
clean.

One deliberate exception: `TestPki` (`test-utils`) is implemented in phase 1. It is the suite's harness, a CA and a
server certificate rcgen generates; unimplemented, every phase-2 failure would be the harness's panic rather than the
function under test's.

`system_from(Option<&str>)` is on the surface so the choice of bundle is tested with the variable's value passed in:
setting `SSL_CERT_FILE` inside a test needs `unsafe` (`env::set_var`), which the workspace forbids.

## Corner cases, enumerated before the suite

| Case | Test |
|---|---|
| empty source | `tls_trust_tests::test_a_source_without_a_usable_certificate_is_refused` |
| text that is no PEM | same |
| certificate armour around no certificate | same; `test_a_file_without_a_usable_certificate_is_refused_with_its_path`; scenario "A source without a usable certificate is refused" |
| only a private-key block | `test_a_source_without_a_usable_certificate_is_refused` |
| one certificate | `test_one_certificate_is_one_anchor`, `test_pki_tests` |
| a certificate listed twice (coincidence) | `test_a_pem_file_yields_its_certificates_each_once`; scenario "A certificate listed twice is trusted once" |
| unusable blocks mixed with certificates | `test_unusable_blocks_are_skipped_and_the_certificates_kept` |
| missing file | `test_a_missing_file_is_unreadable_with_its_path`, `test_the_system_bundle_is_the_file_ssl_cert_file_names` |
| `SSL_CERT_FILE` unset / empty / set | `test_an_unset_or_empty_ssl_cert_file_means_the_default_bundle`, `test_the_system_bundle_is_the_file_ssl_cert_file_names`, `test_system_reads_ssl_cert_file_from_the_environment`; scenario "The bundle SSL_CERT_FILE names is the system bundle" |
| host as a DNS name / as an IP address | `tls_connector_tests::test_a_trusted_certificate_verifies_by_name_and_by_address` |
| host empty / with a space | `test_a_host_no_certificate_can_name_is_refused_before_the_handshake` |
| untrusted CA | `test_a_certificate_from_an_untrusted_ca_fails_the_handshake`; scenario |
| certificate for another name | `test_a_certificate_for_another_name_fails_the_handshake`; scenario |
| server that never answers (bounded wait) | `test_a_server_that_never_answers_fails_the_handshake_at_the_sockets_timeout` |

Every error variant is constructed through the public API: `TrustStoreUnreadable` (missing file), `TrustStoreEmpty`
(empty sources), `InvalidServerName` (empty and spaced hosts), `HandshakeFailed` (untrusted CA, wrong name, silent
server). Expected rustls reasons are rustls's own `CertificateError` names (`UnknownIssuer`, `NotValidForName`);
expected certificates come from rcgen, independent of the crate.

## Phase 2: the suite fails (2026-11-02)

`bazel test //ultragraphdb_transport/ultragraphdb_transport_tls/...`:

- `ultragraphdb_transport_tls_tests`: 19 tests, 0 passed, 19 failed, every one on `not implemented` inside the crate:
  10 at `TlsTrust::from_pem`, 3 at `from_pem_file`, 2 at `system_from`, 1 at `system`, 3 at the `TlsError`
  constructors. The connector tests and `test_pki_tests` fail at `from_pem`, which they call to build their trust
  before reaching `connect`: an unimplemented body of the surface, not the harness.
- `bdd`: 6 scenarios, 6 failed, each step on `not implemented`.
- `doc_test`: passes; the crate documents no example.

## Phase 3: the audit (2026-11-02)

18 defects planted one at a time in a throwaway implementation, each run through `bazel test` of the unit and `bdd`
targets (`audit.py`, a scratch script, restoring each file after its run):

| Defect | Class | Caught by |
|---|---|---|
| D1 duplicate anchors kept | guard removed | `test_a_pem_file_yields_its_certificates_each_once`; scenario "listed twice" |
| D2 empty store accepted | guard removed | `test_a_source_without_a_usable_certificate_is_refused`, `test_a_file_without_...`; scenario |
| D3 empty `SSL_CERT_FILE` taken as a path | guard removed | `test_an_unset_or_empty_ssl_cert_file_means_the_default_bundle` |
| D4 `SSL_CERT_FILE` ignored | plausible neighbour | `test_the_system_bundle_is_the_file_ssl_cert_file_names`, `test_system_reads_...`; scenario |
| D5 environment not read | early return | `test_system_reads_ssl_cert_file_from_the_environment` (Bazel sets the variable) |
| D6 file read under another source name | plausible neighbour | three unit tests; two scenarios |
| D7 unreadable file loses its path | plausible neighbour | four unit tests |
| D8 `len` off by one | off-by-one | six unit tests; two scenarios |
| D9 `is_empty` inverted | inverted comparison | seven unit tests (through the connector's debug check); three scenarios |
| D10 source lost | plausible neighbour | four unit tests; one scenario |
| D11 certificate checked against a fixed name | constant | `test_a_certificate_for_another_name_fails_the_handshake`; scenario |
| D12 handshake deferred | early return | four unit tests; three scenarios |
| D13 trust ignored | constant | two unit tests; two scenarios |
| D14 invalid name reported as a handshake failure | plausible neighbour | `test_a_host_no_certificate_can_name_is_refused_before_the_handshake` |
| D15 handshake failure loses its host | plausible neighbour | three unit tests |
| D16 TLS 1.3 dropped | constant | **missed** in the first audit |
| D17 constructor builds the wrong variant | plausible neighbour | four unit tests; one scenario |
| D18 `Display` text changed | constant | `test_display_names_the_input_and_the_reason` |

D16 widened the suite: the trusted handshake asserts TLS 1.3, and `TestPki::accept_tls12` with
`test_a_server_speaking_tls_1_2_alone_is_verified_over_tls_1_2` pins TLS 1.2, which D16b (TLS 1.2 dropped) plants.
The repeated audit, 19 defects, caught every one.

Input variety: the two `TestPki` authorities of the untrusted-CA tests first shared one subject name, a coincidence
that made rustls report `BadSignature` (a CA of the issuer's name, the wrong key) instead of `UnknownIssuer`; each
authority is now named `ultragraphdb test CA <n>`.

## Phase 4: the implementation (2026-11-02)

Judged by the audited suite: 20 unit tests and 6 scenarios pass under Bazel. One expectation of phase 2 was corrected,
not weakened: rustls's text for a certificate whose names exclude the host is `certificate not valid for name ...`
(`CertificateError::NotValidForNameContext`'s `Display`), where the test had guessed the variant's name. The suite
runs under Bazel alone, the repository's build of record for ultragraphdb; `cargo test` is not run.

Ultra rules no lint checks, reviewed: no recursion and no `while`/`loop` (rule 1, 2) — the handshake is one
`complete_io` call, which drives it to completion or an error, with a debug check after it; control plane only
(rule 3); every `Arc` and the one atomic carry their reason (rule 9); paired checks (rule 5): an empty store refused
in `from_pem` and checked again in `TlsConnector::new`, the count of parsed certificates checked against rustls's
added and ignored, the handshake checked before and after `complete_io`.

## Phase 5: hardening (2026-11-02)

`SSL_CERT_FILE=/nonexistent/ultragraphdb/from-environment.pem scripts/mutants.sh ultragraphdb_transport_tls`:
25 mutants, 10 caught, 14 unviable, 1 missed: `TlsTrust::is_empty -> false`, equivalent because no constructor
returns an empty store; recorded in `.cargo/mutants.toml` and confirmed by the `comm` check to exclude exactly that
mutant.

Fuzzing: the crate adds no decoder of its own. PEM is decoded by `rustls-pki-types` and TLS records by rustls; the
crate passes their output on, so it has no fuzz target.
