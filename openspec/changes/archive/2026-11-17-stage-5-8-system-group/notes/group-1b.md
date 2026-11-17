# Stage 5.8, group 1b: the exchange in two halves, `ultragraphdb_protocol_scram`

Base commit `885f5ec9`. This file records task 1b.1, the Ultra Style lift, and tasks 1b.2 to 1b.6: the five phases,
the mapping of every corner case and law to its test, mutation testing, the fuzz targets, and what was found wanting.
Targets: `:ultragraphdb_protocol_scram_tests` (unit), `:property` (Hegel, tagged `property-test`, new) and `:bdd`
(cucumber, tagged `bdd-test`, new) of `//ultragraphdb_protocol/ultragraphdb_protocol_scram`.

## Task 1b.1: the audit against `openspec/protocols/ULTRA_STYLE.md`

Scope: all of `src/` at the base, 1,000 lines in 22 files. Every `file:line` below is as at `885f5ec9`. Measured as
group 1 measured (`notes/group-1.md`, task 1.1): Clippy through the Bazel aspect with the workspace table, and with
`clippy::indexing_slicing`, `cast_possible_truncation`, `must_use_candidate`, `missing_errors_doc`,
`missing_panics_doc`, `return_self_not_must_use` and `string_slice` added by `--@rules_rust//rust/settings:clippy_flag`;
rule 4 and the rest by reading, the crate being small.

| Rule | Result | Evidence |
|---|---|---|
| 1, acyclic control flow | conforms | no recursion, no `dyn`, no stored callback; the lookup of `ScramServer::first` is an `impl FnOnce`, called once in the call |
| 2, bounds | conforms | `utils/message.rs:38`, `unescape`'s `while`, consumes three bytes or more a turn; `utils/crypto.rs:42` runs `iterations` turns, a count `ScramVerifier::derive` and `ScramClient::final_message` check against 10,000,000 before any key is derived (`types/scram_verifier/mod.rs:35`, `types/scram_client/mod.rs:89`) |
| 3, planes | **gap, closed** | no plane was stated. `src/lib.rs` now states the control plane: one exchange per connection's authentication |
| 4, 70 lines | conforms | the longest body is `ScramClient::final_message`, 38 lines (`types/scram_client/mod.rs:68`) |
| 5, invariants | conforms | no run-time `assert!` under `src/`; the crate holds no state whose violation it could detect beyond the exchange's step, which `OutOfStep` refuses |
| 6, smallest scope | conforms | `clippy::shadow_unrelated` clean; each shadowing refines the value (`types/scram_server/mod.rs:87`, the name prepared) |
| 7, results and panics | **gap, closed** | no `unwrap`, `expect`, `panic!` or `unimplemented!`; one slice that could panic for a key of more than one byte, `&part[2..]` (`utils/message.rs:56`), now `chars.as_str()`. Two `if let Ok` drop HMAC's key-length error, which HMAC cannot raise, each with its reason (`utils/crypto.rs:16`, `:36`); `let _probe: ScramClient` (`types/scram_login/mod.rs:28`) is the check of the login's name and password, a typed binding |
| 7, arithmetic on peer input | conforms | `#![deny(clippy::arithmetic_side_effects)]` at `src/lib.rs:22`, clean |
| 8, no macros, `cfg` | conforms | none |
| 9, ownership | conforms | no `unsafe`, `Arc`, `Rc`, cell, lock or atomic; no `Copy` type; one `Box<ScramVerifier>` in `ServerState` (`types/scram_server/mod.rs:22`), gone with task 1b.4 |
| 10, zero warnings | conforms | the aspect is clean with the workspace table; no `#[allow]`, no `#[expect]` |
| Types: casts, widths | conforms | no `as`; `cast_possible_truncation` clean |
| Types: exhaustive matches | conforms | the one `_ =>` arm matches a `&str` (`types/scram_server/mod.rs:182`) |
| Types: `#[must_use]` | **gap, closed** | 13 public functions returning a value that is no `Result` carried none: `types/scram_server/mod.rs:42` (`new`), `:54` (`with_iterations`); `types/scram_verifier/mod.rs:22` (`new`), `:48`, `:53`, `:58`, `:63` (the four getters); `types/scram_authenticator/mod.rs:26` (`new`), `:37` (`with_iterations`); `types/scram_exchange/mod.rs:21` (`new`); `types/scram_login/mod.rs:47` (`name`); `errors/scram_error/mod.rs:97` (`server`), `:110` (`random`). `must_use_candidate` names 7 of them and `return_self_not_must_use` an eighth, `ScramAuthenticator::with_iterations`; the five they pass over (generic, or taking `impl Into`) are of the same kind |
| Comments: `# Errors` | **gap, closed** | 9 public functions returning a `Result` had none: `types/scram_client/mod.rs:40` (`new`), `:54` (`first_message`, no error documented at all), `:68` (`final_message`), `:110` (`verify`); `types/scram_server/mod.rs:76` (`first`), `:128` (`last`); `types/scram_verifier/mod.rs:34` (`derive`); `types/scram_login/mod.rs:27` (`new`); `utils/nonce.rs:14` (`system_nonce`). The refusals each summary listed moved under the heading; three it left out were added as the code raises them: `ScramClient::final_message`'s `Server`, `ScramClient::verify`'s `Malformed`, `ScramServer::last`'s `ChannelBinding` |
| Naming | gap, left open | *Left open*, 1 |
| Secrets | gap, left open | *Left open*, 2 |

### The lift

Small by group 1's measure: 10 files, 67 lines added and 18 removed, none changing what the crate does. So the group
made it and went on, in the same working tree as tasks 1b.2 to 1b.6.

| | Before, at `885f5ec9` | After the lift |
|---|---|---|
| `:ultragraphdb_protocol_scram_tests` | 34 passed | 34 passed |
| Clippy aspect, the workspace table | clean | clean |
| `must_use_candidate`, `missing_errors_doc`, `return_self_not_must_use`, `string_slice` | 7, 9, 2, 0 | 0, 0, 0, 0 |

### Left open

1. **Naming, "units and qualifiers last".** `MIN_ITERATIONS`, `MAX_ITERATIONS`, `DEFAULT_ITERATIONS` and `SALT_SIZE`
   (`utils/constants.rs:10`, `:15`, `:18`, `:21`) put the qualifier first. `ultragraphdb_common_role`, the fuzz targets
   and the frontend's tests name them, so renaming is not this group's (design D2).
2. **Secrets in `Debug` and in memory.** `ScramClient` and `ScramLogin` derive `Debug` over the password
   (`types/scram_client/mod.rs:28`, `types/scram_login/mod.rs:17`), `ScramServer` and `ScramAuthenticator` over the
   secret (`types/scram_server/mod.rs:31`, `types/scram_authenticator/mod.rs:15`), `ScramVerifier` over its two keys
   (`types/scram_verifier/mod.rs:12`); nothing is zeroized on drop. `SECURITY.md`, *Credentials as verifiers, secrets
   out of logs*: "no password, token or authentication message reaches a log or an error"; a `{:?}` of any of these
   types would put one there. The crate neither redacts nor zeroizes, so `ScramRelay` follows it trivially: it holds no
   key, and a test shows its `Debug` holds neither key. Redacting the five changes their `Debug` output, which other
   crates' tests may print; it is the user's to decide.
3. **What follows the nonce in a client-first-message is not read.** `ScramServer` takes `n,,n=user,r=a,c`, whose
   last part is not of RFC 5802 §7's `attr-val` form, as it takes a well-formed extension. The relay reads it alike, as
   the law of task 1b.5 has it; `scram_relay_tests::test_a_client_first_message_is_refused_as_the_server_refuses_it`
   pins both. Refusing it would change `ScramServer`'s behaviour, which task 1b.3 keeps.

## Tasks 1b.2 to 1b.6: the five phases

### Phase 1: the surface

- `ScramRelay` (`src/types/scram_relay/mod.rs`): `new(nonce)`; `first(client_first) -> Result<String, ScramError>`,
  the name a Challenge carries; `challenge(salt, iterations) -> Result<Vec<u8>, ScramError>`, the
  server-first-message; `last(client_final) -> Result<(String, [u8; 32]), ScramError>`, the AuthMessage and the
  ClientProof a Verify carries; `finish(signature: &[u8; 32]) -> Result<Vec<u8>, ScramError>`, the
  server-final-message. The types are the wire's: `VerifyView::auth_message` is a `&str`, `proof` and `signature` are
  `&[u8; 32]`.
- On `ScramVerifier`: `unknown(secret: &[u8; 32], name, iterations) -> ScramVerifier` and `sign(auth_message, proof:
  &[u8; 32]) -> Option<[u8; 32]>`.
- No new `ScramError` variant: each refusal of the halves is one `ScramServer` already gives. Every existing signature
  keeps its form.

Each body a bare `unimplemented!()`. `bazel build` of the crate passed on the surface.

### Phase 2: the suite, observed failing

Run on the phase-1 surface, `bazel test //ultragraphdb_protocol/ultragraphdb_protocol_scram:all`:

| Target | Run | Failed | Passed |
|---|---|---|---|
| unit | 53 tests | 19, each the `not implemented` panic at `scram_relay/mod.rs:46` or `scram_verifier/mod.rs:56`, `:64` | 34, the crate's tests before the group |
| property | 9 tests | 9, each the `not implemented` panic | 0 |
| bdd | 6 scenarios, 6 steps run | 6, each at its `When` step's `not implemented` panic | 0 |

Before the group the unit target ran 34 tests; the crate had no property and no BDD target. Every panic of the run is
at a stub; none is a compile error or a panic elsewhere.

The scenarios, verbatim: `features/system-group/proof_checked.feature` holds the three of "A client's proof is checked
in the system group"; `features/client-endpoint/authentication.feature` three of the five of "Connections authenticate
against the metadata group's principals": "A wrong password is refused without detail", "The frontend never holds a
stored key" and "An unknown name answers as a known one does". "A data request before authentication is refused" and
"A source past its failures is throttled, then banned" need a connection and the throttle, which are group 5's
(tasks.md, 5.1). Both features are bound in `tests/bdd.rs`, with `fail_on_skipped`: the client's exchange runs through
`ScramRelay`, and the system group answers Challenge and Verify, as the wire's `Challenge` and `Verify` bodies, from
the principal's verifier or an unknown name's and `ScramVerifier::sign`. "No other command the endpoint serves returns
them" is bound for what the halves build, the relay's two messages; the session serving the other commands is group
3's.

The BDD target is new to this crate and built as the wire crate's is: `tests/bdd.rs`, `[[test]] bdd` with `harness =
false`, a `rust_test` named `bdd` with `use_libtest_harness = False` and the features as `data`, tagged `bdd-test`. The
property target is built as the wire crate's: `tests/property/`, `[[test]] property`, a `rust_test` named `property`
tagged `property-test` with `HEGEL_DERANDOMIZE` and `HEGEL_DATABASE` set, `hegeltest` with `static-engine`. The unit
target's glob now excludes both. `tests/utils/halves.rs` drives the two halves as the frontend and the group drive
them, with `ScramServer`'s shape of answer; the unit target reaches it through `tests/utils/mod.rs` and the property
target by path. New dev-dependencies: `ultragraphdb_protocol_wire` (the reply bodies the scenarios read), `cucumber`,
`futures`, `hegeltest`.

### The corner cases and their tests

Unit tests in `tests/types/scram_relay/scram_relay_tests.rs` (relay) and `tests/types/scram_verifier/scram_verifier_tests.rs`
(verifier). Each case of the relay runs through the halves and `ScramServer` and asserts the two agree.

| Case | Test |
|---|---|
| C1b.1 | relay: `test_the_relay_reads_the_name_the_server_reads` |
| C1b.2 | relay: `test_an_empty_name_and_a_mapped_name_are_answered_as_names_not_held` (*Found wanting*, 4) |
| C1b.3 | relay: `test_a_client_first_message_is_refused_as_the_server_refuses_it` |
| C1b.4 | relay: `test_the_server_first_message_of_a_salt_is_the_servers` |
| C1b.5 | relay: `test_the_least_count_and_one_below_it_are_built_as_the_server_builds_them` (*Found wanting*, 2) |
| C1b.6 | relay: `test_a_nonce_other_than_the_exchanges_is_refused` |
| C1b.7 | relay: `test_a_channel_binding_other_than_the_first_messages_is_refused` |
| C1b.8 | relay: `test_a_proof_not_in_its_form_is_refused`, `test_the_nonce_is_checked_before_the_proofs_length` |
| C1b.9 | verifier: `test_a_proof_with_one_bit_changed_is_signed_with_nothing` |
| C1b.10 | verifier: `test_a_proof_that_holds_is_signed_with_the_servers_signature` |
| C1b.11 | verifier: `test_the_verifier_of_a_name_not_held_signs_nothing` |
| C1b.12 | verifier: `test_an_unknown_names_verifier_is_a_function_of_the_secret_and_the_name`, `test_an_unknown_names_salt_and_count_are_the_servers` |
| C1b.13 | relay: `test_rfc_7677s_exchange_runs_through_both_halves`: RFC 7677 §3's name, server-first-message, ClientProof, ServerSignature and server-final-message, and RFC 5802 §3's AuthMessage of its messages (`tests/utils/rfc7677.rs`) |
| C1b.14 | relay: `test_an_empty_password_and_a_mapped_password_are_answered_as_the_server_answers_them` |

All 14 have a test. Beside them: `test_messages_out_of_step_are_refused` (every method out of step, a refused message
leaving the relay where it stood, a second client-final-message read as `ScramServer` reads one after a refused proof),
`test_an_extension_after_the_nonce_is_part_of_the_auth_message`, and `test_the_relay_holds_no_key` (neither key, in
any of its forms, in the relay's messages or its `Debug`; the verifier's `Debug` holding it shows the check finds a key
where one is).

### The laws and their property tests

`tests/property/types/<source>_property_tests.rs`; inputs drawn in `tests/property/utils/draw.rs`: a name and a
password of printable ASCII with `,` and `=`, empty included, two nonces of 1 to 24 characters, a salt of 1 to 64 bytes,
a count of 4,096 to 4,103 where a key is derived and any `u32` where none is.

| Law | Property test |
|---|---|
| L1b.1 | `scram_relay`: `test_a_client_completes_its_exchange_through_the_two_halves` |
| L1b.2 | `scram_relay`: `test_the_two_halves_accept_exactly_what_the_server_accepts`: 1 to 4 drawn steps, each the client's first or final message right or changed in one place, the name held or not, the password right or wrong, sent to the halves and to `ScramServer`, every answer compared |
| L1b.3 | `scram_relay`: `test_the_server_first_message_is_the_servers`, `test_the_server_final_message_is_the_servers` (any signature gives RFC 5802 §7's `v=` and its base64; the group's gives `ScramServer`'s message) |
| L1b.4 | `scram_verifier`: `test_a_changed_proof_and_a_name_not_held_are_refused_alike` |
| L1b.5 | `scram_relay`: `test_no_message_the_relay_builds_holds_a_key` |
| L1b.6 | `scram_verifier`: `test_an_unknown_names_verifier_is_the_servers` |
| L1b.7 | `scram_relay`: `test_the_relay_reads_the_client_first_message_as_the_server_does` (*Found wanting*, 1) |
| L1b.8 | `scram_relay`: `test_the_relay_refuses_the_client_final_message_as_the_server_does` |

All 8 laws have a property test, 9 tests in all. L1b.2 is a drawn sequence of the exchange's steps against
`ScramServer` as the model, not Hegel's stateful API: the exchange has two commands, and the sequence draws their order,
their repetition and their alteration.

**What the equivalence laws rest on after task 1b.4.** `ScramServer` now runs over `ScramRelay` and
`ScramVerifier::sign`, so L1b.2, L1b.3, L1b.7 and L1b.8 afterwards compare the halves with code built from them. They
were run against `ScramServer` as it stood, before the rewrite, at the first green run of phase 4 (below), and passed
there. After the rewrite they hold the halves to `ScramServer`'s 34 tests, unchanged and green, and to RFC 7677's vector.

### Every error variant reachable through these halves

No variant was added. The halves raise `Malformed` (`"channel binding"`, `"authorization identity"`, `"name"`,
`"nonce"`, `"proof"`), `ChannelBinding`, `Prepare`, `Nonce`, `Proof` and `OutOfStep`, each asserted by variant and part
in the relay's unit tests. `Iterations` is the client's refusal of a count below the least (C1b.5).

### Phase 3: the suite's inputs against the enumeration

Reviewed for coincidence, case by case. What the review added, one test:

- `test_the_channel_binding_follows_the_first_messages_flag`: every client-final-message of the suite followed an `n`
  flag, so the binding was always `biws` and a relay comparing against `biws` itself would have passed. Under the `y`
  flag the binding is `eSws`, and `biws` is refused, as `ScramServer` has them. Observed failing on the stubs: 54 unit
  tests, 20 failing.

What the review found already apart: the salts of C1b.4 differ in length and content and their messages are asserted
different; C1b.5's two counts and C1b.12's two secrets, two names and two counts differ; the stored key `[1; 32]` and
the server key `[2; 32]` of the verifiers built by hand differ; the bits C1b.9 changes are at either end; the client's
and the server's nonces differ in every unit test, and RFC 7677's vector fixes their order in the combined nonce.

### Phase 4: the implementation

In two steps, so the equivalence laws met `ScramServer` as it stood before it changed.

1. The two halves, `ScramServer` untouched. The suite's first run: unit 53 of 54, property 9 of 9, BDD 6 scenarios of
   6. The one failure was a literal of the phase-2 suite, not the code: `relay_reads(b"n,,n=user,r=a,c")` expected
   `Malformed` `"nonce"`, and the unchanged `ScramServer`, the oracle the same test asserts first, takes the message
   (*Left open*, 3). The literal was corrected to `ScramServer`'s answer; no other test changed.
2. Task 1b.4: `ScramServer` over the halves. `first` reads through `ScramRelay::first`, takes an unknown name's
   verifier from `ScramVerifier::unknown` and builds through `ScramRelay::challenge`; `last` reads through
   `ScramRelay::last`, checks through `ScramVerifier::sign` and builds through `ScramRelay::finish`; `name_in` reads
   through the relay module's `read_name`. Gone: `ScramServer`'s `header`, its private `unknown`, its reading of both
   messages and its proof check, the second copy of each. `ScramServer`'s tests unchanged, all green.

| Target | Before the group | After |
|---|---|---|
| unit | 34 tests | 54 tests |
| property | none | 9 tests |
| bdd | none | 6 scenarios, 12 steps |

The Clippy aspect is clean over the library, the unit tests, the property tests and the BDD target.

### Phase 5: fuzz targets and mutation testing

Two targets in `fuzz/ultragraphdb_protocol_scram/fuzz_targets/`, registered as `[[bin]]`s of the crate's package
beside `scram_server` and `scram_client`:

- `scram_relay_first`: any bytes as a client-first-message read by `ScramRelay::first`. Nothing panics; the relay
  reads what `ScramServer::first` reads, the name it looks up or the same refusal; `ScramServer::name_in` reads the
  relay's name, refuses with the relay's refusal what it refuses, and differs only by `Malformed` `"nonce"`; an
  accepted message is answered with `ScramServer`'s server-first-message. Seeds: `testdata/fuzz_seeds/scram_relay_first/`,
  RFC 7677's client-first-message and one with an escaped name.
- `scram_relay_last`: any bytes as a client-final-message read by `ScramRelay::last` after RFC 7677's first two
  messages. Nothing panics; an accepted message gives an AuthMessage opening with the exchange's first two messages;
  the relay refuses exactly what `ScramServer` holding RFC 7677's user refuses, with the same refusal, and what it reads
  `ScramServer` answers as `ScramVerifier::sign` decides. Seed: `testdata/fuzz_seeds/scram_relay_last/rfc7677`, RFC
  7677's client-final-message.

`cargo +nightly fuzz build --fuzz-dir fuzz/ultragraphdb_protocol_scram` builds all four targets of the package. The
runs of `scripts/fuzz.sh` are the stage's final verification's. The build rewrote `fuzz/Cargo.lock`, which was stale
against the root workspace's path crates (it gained `ultragraphdb_core_cell`, `ultragraphdb_core_geo` and ICU's crates,
and lost `ultragraphdb_common_utils`, `ultragraphdb_error_json`, `ultragraphdb_error_num` and their dependencies); no
manifest of the fuzz workspace changed but this package's.

Mutation testing, `scripts/mutants.sh ultragraphdb_protocol_scram --diff 885f5ec9`:

| Run | Mutants | Caught | Unviable | Missed | Timeout |
|---|---|---|---|---|---|
| 1, the diff | 51 | 0 | 0 | 0 | 0: the unmutated baseline did not compile under Cargo, so no mutant was tested |
| 2, the diff | 51 | 49 | 2 | 0 | 0 |

Run 1's baseline: `base64::DecodeError` implements `std::error::Error` only under base64's `std` feature, which the
workspace pins `default-features = false` and this crate takes with `alloc` alone. Bazel builds every crate with the
features of the whole workspace unified, where something turns `std` on; Cargo building this package alone does not,
so every `STANDARD.decode(..)?` in a test failed to convert to `TestError`. Two of the seven were in the unit tests the
crate had before the group (`tests/utils/nonce_tests.rs:15`, `:33`): `cargo test -p ultragraphdb_protocol_scram` did not
compile at the base either. Fixed by failing those decodes through `.context(..)?`, the repository's own way, which
needs `Display` alone; no manifest changed. Run 2 is the run over the diff.

The two unviable mutants do not compile: `ScramVerifier::unknown` replaced by `Default::default()` (`ScramVerifier` has
no `Default`), and the deletion of `read_name`'s one binding match arm (its names unbound). No survivor; no third run,
and no entry in `.cargo/mutants.toml`.

### The gate

`make build` passes. `bazel test //... --test_tag_filters=-integration_test`: 148 targets, 148 passed, answered from
Bazel's cache against the tree holding this group's and group 2's work. Clippy's aspect is clean over the crate's four
targets, and `cargo fmt --check` over the crate and the fuzz package.

## Found wanting

None is decided here; each is routed to the user, with the reading the code holds until then.

1. **L1b.7 compares the relay's reader with `ScramServer::name_in`, which reads no nonce.** `notes/laws.md`, L1b.7:
   "the relay's reader of the name gives what `ScramServer::name_in` gives, the name or the same error". `name_in`
   reads the header and the name (`types/scram_server/mod.rs:67-71` at the base); `ScramServer::first` refuses a
   missing or malformed nonce before it looks the name up (`:88-90`). A message whose nonce alone is broken is read by
   the one and refused by the other, and the relay cannot agree with both. Implemented: the relay reads as
   `ScramServer::first` reads, so the frontend refuses a malformed message before it asks the group anything. The law's
   test asserts that, `name_in`'s name wherever the relay reads one, `name_in`'s refusal wherever it refuses, and
   `Malformed` `"nonce"` as the one case between.
2. **C1b.5 presumes a least count `ScramServer` takes.** `notes/corner-cases.md`, C1b.5: "the least iteration count
   `ScramServer` takes, and one below it: built, and refused as `ScramServer` refuses it". `ScramServer` refuses no
   count: `with_iterations` takes any `u32` (`types/scram_server/mod.rs:54`), and `first` builds the
   server-first-message of whatever count the verifier holds (`:96-100`, `ScramVerifier::new` checking none). The
   counts refused are `ScramVerifier::derive`'s and `ScramClient`'s, outside 4,096 to 10,000,000. Implemented: the
   relay builds any count it is handed, byte for byte as `ScramServer` does, as design D8 and task 1b.5 require; the
   count reaching it has passed the wire's Challenge decoder (`PROTOCOL.md` §13.8: `FieldValue` `"iterations"` outside
   4,096 to 10,000,000), and a count below the least is refused by the client, `Iterations`, for both.
3. **"A salt and a count derived from a secret".** `PROTOCOL.md` §13.8: "A name no principal holds is answered with a
   salt and a count derived from a secret the metadata group draws". `ScramServer` derives the salt and the two keys
   from the secret and the name, and takes the count it derives verifiers with (`with_iterations`, 600,000 by default),
   so that an unknown name costs a client what a known one does (`SECURITY.md`, *Failures*: "takes as long for an
   unknown principal as for a known one"). Implemented: `ScramVerifier::unknown(secret, name, iterations)`, the count
   the caller's, as `ScramServer` has it. The group must pass the count it derives verifiers with; that is group 3's to
   wire. §13.8 would read truly as "a salt derived from a secret … and the count the group derives verifiers with".
   *Decided 2026-11-16:* §13.8 and system-group, "A client's proof is checked in the system group", now say so.
4. **C1b.2, "a name SASLprep changes … the group answers it as a name no principal holds".** The relay hands the group
   the prepared name, as `ScramServer::first` looks up the prepared name, so a name SASLprep maps onto a held one is
   answered as that principal: `us\u{00AD}er` is asked for, and answered, as `user`. `PROTOCOL.md` §13.8 and §13.9:
   "a name that is empty or not prepared with SASLprep is a name no principal holds", a rule of the body's section 0,
   which the relay always fills with a prepared name. Implemented: the prepared name, as `ScramServer` has it; the test
   asserts the relay reads `user`, and that an empty name and a mapped name no principal holds are answered as
   `ScramServer` answers them.
