/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! What the client endpoint answers a real client: TLS, SCRAM, the tenant gate, and what the frontend answers itself. The
//! compute and storage behind it are real and take no part where the frontend answers alone.

use cucumber::{then, when};
use ultragraphdb_client_compio::{ClientError, ClientErrorEnum};
use ultragraphdb_protocol_wire::{
    Begin, ColumnType, ConditionalWrite, Declared, DeclaredKind, DeclaredPredicate, DirectWrite,
    DropCommand, ErrorCode, Language, PatternSlot, Prepare, QuadPattern, TermTable, Write,
    WriteChange,
};
use ultragraphdb_tests::SECRET;
use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

use crate::Steps;
use crate::client_reads::{
    Pattern, TENANT, closing_code, direct_of, iri, read_whole, refusal_of, refused_with,
};
use crate::cluster::{client, shared_client};
use crate::shells::{ADMIN, ClientAt, Knobs, TestClient, run};
use crate::transaction_steps::in_shared;

/// The tenant no scenario's cluster holds.
const UNKNOWN_TENANT: u64 = 9;

/// A login's outcome at the endpoint at `at`.
async fn logging_in(at: &ClientAt, name: &str, password: &str) -> TestResult<Option<ClientError>> {
    let mut client = TestClient::new(at, &Knobs::logging_in(name, password))?;
    Ok(client.connect().await.err())
}

/// A declaration of `predicate` as a one-change Write.
pub(crate) fn declaring(declared: DeclaredPredicate) -> Write {
    Write::new(
        TermTable::new(),
        vec![WriteChange::Declare(Declared::Predicate(declared))],
    )
}

/// Commits a declaration of `declared` in a transaction of its own, which goes through no session.
pub(crate) async fn declare(client: &mut TestClient, declared: DeclaredPredicate) -> TestResult {
    let transaction = client.begin(TENANT, false).await?;
    client.write(&transaction, &declaring(declared)).await?;
    let _version = client.commit(transaction, None).await?;
    Ok(())
}

#[when(
    "the frontend holds the test CA's certificate for `localhost` and a client trusting the CA connects and authenticates"
)]
fn tls_client(w: &mut Steps) -> TestResult {
    let [a, b, knows] = ["a", "b", "knows"].map(|name| w.iri(name));
    w.seen.answers = vec![in_shared(w, async |client| {
        let _written = client
            .direct_write(direct_of(&a, &knows, &iri(&b))?, None)
            .await?;
        let reader = client.begin(TENANT, false).await?;
        let answer = read_whole(client, &reader, &Pattern::of(Some(&a), Some(&knows))).await?;
        client.abort(reader).await?;
        Ok(answer)
    })?];
    Ok(())
}

#[then("the TLS handshake, the wire handshake and SCRAM complete, and a write and a read succeed")]
fn tls_served(w: &mut Steps) -> TestResult {
    let answer = w.seen.answers.first().context("the read's answer")?;
    assert_eq!(answer.column(0), vec![w.iri("b")], "{}", w.over());
    Ok(())
}

#[when(
    "a client without TLS sends the wire protocol's first frame to an endpoint configured for TLS"
)]
fn clear_text_at_tls(w: &mut Steps) -> TestResult {
    let tls_endpoint = shared_client(w.carrier)?;
    let at = ClientAt::clear_text(&tls_endpoint.address);
    w.seen.refusals = run(async {
        let refused = logging_in(&at, ADMIN, crate::shells::PASSWORD).await?;
        refused
            .map(|error| vec![error])
            .context("a clear-text client refused")
    })?;
    Ok(())
}

#[then(
    "the connection fails as a TLS handshake failure, and no frame of the wire protocol is read"
)]
fn clear_text_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    // No frame came back: a server speaking the wire protocol answers a Hello, and an answer would have been a closing Error.
    let framed = matches!(
        &refused.0,
        ClientErrorEnum::Closed { .. }
            | ClientErrorEnum::Refused { .. }
            | ClientErrorEnum::Protocol { .. }
    );
    assert!(!framed, "{refused:?} {}", w.over());
    Ok(())
}

#[when("a client authenticates as `ann` with a wrong password, and as an unknown name")]
fn wrong_password_and_name(w: &mut Steps) -> TestResult {
    let at = shared_client(w.carrier)?;
    w.seen.refusals = run(async {
        Ok(vec![
            logging_in(&at, ADMIN, "not the password")
                .await?
                .context("a wrong password refused")?,
            logging_in(&at, "nobody-of-this-name", "not the password")
                .await?
                .context("an unknown name refused")?,
        ])
    })?;
    Ok(())
}

/// The closing code and message of a refused login, the wrong password's and the unknown name's, which must be one.
fn one_authentication_error(w: &Steps) -> TestResult {
    let [wrong, unknown] = w.seen.refusals.as_slice() else {
        return Err(TestError::new("two refusals"));
    };
    assert_eq!(wrong, unknown, "the two refusals differ {}", w.over());
    let found = closing_code(wrong).context("the Error closing the connection")?;
    assert_eq!(found, ErrorCode::AuthenticationFailed, "{}", w.over());
    Ok(())
}

#[then("both are refused with the same authentication error")]
fn same_authentication_error(w: &mut Steps) -> TestResult {
    one_authentication_error(w)
}

#[when(
    "a client's SCRAM proof does not verify, once for a principal that exists and once for a name no principal holds"
)]
fn proof_not_verified(w: &mut Steps) -> TestResult {
    wrong_password_and_name(w)
}

#[then(
    "each is refused with code 19 on stream 0 and the connection closed, and the two Error frames are the same bytes"
)]
fn same_frames(w: &mut Steps) -> TestResult {
    one_authentication_error(w)
}

#[when("a client authenticates on the client endpoint with the role secret")]
fn role_secret_login(w: &mut Steps) -> TestResult {
    let at = shared_client(w.carrier)?;
    w.seen.refusals = run(async {
        Ok(vec![
            logging_in(&at, ADMIN, "not the password")
                .await?
                .context("a wrong password refused")?,
            logging_in(&at, "compute", SECRET)
                .await?
                .context("the role secret refused")?,
        ])
    })?;
    Ok(())
}

#[then("authentication is refused with the same error as a wrong password")]
fn role_secret_refused(w: &mut Steps) -> TestResult {
    one_authentication_error(w)
}

#[when(
    "a server requiring authentication lists `SCRAM-SHA-256` and a client completes the exchange with the principal's password"
)]
fn scram_exchange(w: &mut Steps) -> TestResult {
    let at = shared_client(w.carrier)?;
    let (mechanisms, principal, begun) = run(async {
        let mut client = client(&at).await?;
        let welcome = client.welcome().context("the Welcome")?;
        let mechanisms = welcome.mechanisms().to_vec();
        let principal = client.principal().context("the principal")?;
        let begun = client.begin(TENANT, true).await?;
        client.abort(begun).await?;
        Ok((mechanisms, principal, begun.tenant()))
    })?;
    w.seen.facts = mechanisms;
    w.seen.facts.push(principal.to_string());
    w.seen.facts.push(begun.to_string());
    Ok(())
}

#[then("the server's final Authenticate names the principal, and the client's requests are served")]
fn scram_done(w: &mut Steps) -> TestResult {
    let facts = &w.seen.facts;
    assert!(
        facts.iter().any(|fact| fact == "SCRAM-SHA-256"),
        "{facts:?}"
    );
    let [.., principal, tenant] = facts.as_slice() else {
        return Err(TestError::new("a principal and a tenant"));
    };
    assert_ne!(principal, "0", "the principal the exchange named");
    assert_eq!(tenant, &TENANT.to_string());
    Ok(())
}

#[when("an authenticated principal sends a Begin naming tenant 0")]
fn gate_allows(w: &mut Steps) -> TestResult {
    let tenant = in_shared(w, async |client| {
        let begun = client.begin(TENANT, false).await?;
        client.abort(begun).await?;
        Ok(begun.tenant())
    })?;
    w.seen.facts = vec![tenant.to_string()];
    Ok(())
}

#[then("the gate passes it, and the Begin is answered")]
fn gate_passed(w: &mut Steps) {
    assert_eq!(w.seen.facts, [TENANT.to_string()], "{}", w.over());
}

/// A Prepare of `tenant` that the frontend answers by the pattern's own rules.
fn preparing(tenant: u64) -> Prepare {
    let pattern = QuadPattern::new(
        PatternSlot::DefaultGraph,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
        PatternSlot::Unbound,
    );
    Prepare::new(
        tenant,
        0,
        0,
        TermTable::new(),
        pattern,
        Vec::<ColumnType>::new(),
        Vec::<ColumnType>::new(),
    )
}

#[when(
    "an authenticated principal sends a Begin, a Prepare, a Describe, an Install, a Drop, a DirectWrite and a ConditionalWrite, each naming tenant 9, which the metadata group does not hold"
)]
fn unknown_tenant(w: &mut Steps) -> TestResult {
    let [a, b, knows, unique] = ["a", "b", "knows", "unique"].map(|name| w.iri(name));
    let answers = in_shared(w, async |client| {
        let session = client.register().await?;
        let mut terms = TermTable::new();
        let change = WriteChange::Insert {
            graph: None,
            subject: terms.iri(&a),
            predicate: terms.iri(&knows),
            object: terms.iri(&b),
        };
        let direct = DirectWrite::new(UNKNOWN_TENANT, terms, vec![change]);
        let mut op_terms = TermTable::new();
        let operation = ultragraphdb_protocol_wire::CommitOperation::CounterAdd {
            graph: None,
            subject: op_terms.iri(&a),
            predicate: op_terms.iri(&unique),
            delta: 1,
        };
        let conditional =
            ConditionalWrite::new(UNKNOWN_TENANT, op_terms, Vec::new(), vec![operation]);
        Ok(vec![
            client
                .begin_with(Begin::new(UNKNOWN_TENANT, 0, false), None)
                .await
                .map(|_| ())
                .err(),
            client
                .prepare(&preparing(UNKNOWN_TENANT))
                .await
                .map(|_| ())
                .err(),
            client.describe(UNKNOWN_TENANT).await.map(|_| ()).err(),
            client
                .drop_command(DropCommand::graph(UNKNOWN_TENANT, &a), session)
                .await
                .map(|_| ())
                .err(),
            client.direct_write(direct, None).await.map(|_| ()).err(),
            client
                .conditional_write(conditional, None)
                .await
                .map(|_| ())
                .err(),
        ])
    })?;
    w.seen.refusals = answers
        .into_iter()
        .map(|refused| refused.context("a request of tenant 9 refused"))
        .collect::<TestResult<_>>()?;
    Ok(())
}

#[then(
    "each is refused with code 7, `detail` 9, the message naming tenant 9, and no other role sees it"
)]
fn tenant_refusals(w: &mut Steps) -> TestResult {
    assert!(!w.seen.refusals.is_empty());
    for refused in &w.seen.refusals {
        let (code, detail, message) = refusal_of(refused)?;
        assert_eq!(code, ErrorCode::Refused, "{refused:?}");
        assert_eq!(detail, UNKNOWN_TENANT, "{refused:?}");
        assert!(
            message.contains(&UNKNOWN_TENANT.to_string()),
            "{message} {}",
            w.over()
        );
    }
    Ok(())
}

#[when(
    "a client begins a read-write transaction and sends Commit through its session with nothing else"
)]
fn empty_commit(w: &mut Steps) -> TestResult {
    let committed = in_shared(w, async |client| {
        let session = client.register().await?;
        let transaction = client.begin(TENANT, false).await?;
        Ok(client.commit(transaction, session).await?)
    })?;
    w.seen.commits = vec![committed];
    Ok(())
}

#[then(
    "the frontend answers `commit_version` 0, the token zero and no results, and no partition sees it"
)]
fn empty_answered(w: &mut Steps) -> TestResult {
    let committed = w.seen.commits.first().context("the commit's reply")?;
    assert_eq!(committed.version(), 0, "{}", w.over());
    assert!(committed.token().is_none(), "{committed:?}");
    assert!(committed.results().is_empty(), "{committed:?}");
    Ok(())
}

#[when("a client declares a vector index on `embedding`")]
fn vector_declared(w: &mut Steps) -> TestResult {
    let embedding = w.iri("embedding");
    let (refused, described) = in_shared(w, async |client| {
        let declared = DeclaredPredicate::new(&embedding)
            .with_kind(DeclaredKind::Property)
            .with_vector_index("hnsw", Vec::new());
        let transaction = client.begin(TENANT, false).await?;
        let refused = match client.write(&transaction, &declaring(declared)).await {
            Ok(()) => client.commit(transaction, None).await.err(),
            Err(error) => Some(error),
        };
        Ok((refused, client.describe(TENANT).await?))
    })?;
    w.seen.refusals = refused.into_iter().collect();
    w.seen.facts = described
        .predicates()
        .iter()
        .map(|declared| declared.predicate().to_string())
        .collect();
    Ok(())
}

#[then("the declaration is refused with an error naming `embedding`, and the catalog is unchanged")]
fn vector_refused(w: &mut Steps) -> TestResult {
    let refused = w.seen.refusals.first().context("the refusal")?;
    let (code, _, message) = refusal_of(refused)?;
    assert_eq!(code, ErrorCode::Refused, "{refused:?}");
    let embedding = w.iri("embedding");
    assert!(message.contains(&embedding), "{message}");
    assert!(!w.seen.facts.contains(&embedding), "{:?}", w.seen.facts);
    Ok(())
}

/// A Prepare of a language a later stage serves.
fn later_stage_prepare() -> Prepare {
    Prepare::of_text(
        TENANT,
        Language::Gql,
        "MATCH (n) RETURN n",
        TermTable::new(),
        Vec::new(),
        Vec::new(),
    )
}

#[when("an authenticated principal prepares a text of a later stage's language")]
fn later_stage(w: &mut Steps) -> TestResult {
    let (detail, message) = in_shared(w, async |client| {
        refused_with(
            client.prepare(&later_stage_prepare()).await,
            ErrorCode::Refused,
        )
    })?;
    w.seen.facts = vec![detail.to_string(), message];
    Ok(())
}

#[then("it is refused with code 7 on its stream, the message naming the command and its stage")]
fn later_stage_refused(w: &mut Steps) -> TestResult {
    let [_, message] = w.seen.facts.as_slice() else {
        return Err(TestError::new("a refusal's detail and message"));
    };
    assert!(message.contains("Prepare"), "{message}");
    assert!(message.contains("stage"), "{message}");
    Ok(())
}
