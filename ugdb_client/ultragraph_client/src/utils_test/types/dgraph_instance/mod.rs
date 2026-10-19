//! A Dgraph the caller may connect to.

use crate::utils_test::errors::fixture_error::FixtureError;
use crate::utils_test::traits::instance_provider::InstanceProvider;
use crate::utils_test::types::container_provider::ContainerProvider;
use crate::utils_test::types::endpoint::Endpoint;
use crate::utils_test::types::exclusivity::Exclusivity;
use crate::utils_test::types::existing_provider::ExistingProvider;
use crate::utils_test::types::run_id::RunId;
use crate::utils_test::types::strategy::Strategy;
use crate::utils_test::{
    ADMIN_PASSWORD_ENV, CA_BUNDLE_URL_ENV, DEFAULT_GROOT_PASSWORD, STRATEGY_ENV, env_var,
};

/// A running Dgraph, and everything a caller needs to know about it.
///
/// Every field private: an instance that did not come from [`Self::acquire`] could claim an
/// exclusivity it does not have, and exclusivity is what callers use to decide whether they may
/// destroy data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DgraphInstance {
    endpoint: Endpoint,
    strategy: Strategy,
    port: u16,
    connection_string: String,
    container_id: Option<String>,
    run_id: RunId,
    ca_bundle_url: Option<String>,
}

impl DgraphInstance {
    /// Read the environment, obtain a Dgraph however it requires, and return only once it
    /// reports healthy.
    ///
    /// Blocking, not async, on purpose: `docker_utils` is synchronous and so is the health check,
    /// so there is nothing to await. That also makes this callable from a plain `#[test]` and
    /// from `#[tokio::test]` alike, with no ambient runtime required either way.
    pub fn acquire() -> Result<Self, FixtureError> {
        Self::acquire_with(Strategy::from_env()?)
    }

    /// [`Self::acquire`] for a strategy already decided, which is what a caller driving both
    /// paths explicitly needs.
    pub fn acquire_with(strategy: Strategy) -> Result<Self, FixtureError> {
        let endpoint = Endpoint::from_env()?;
        let ca_bundle_url = env_var(CA_BUNDLE_URL_ENV);

        // Static dispatch: each arm monomorphises, nothing is boxed.
        let (port, container_id) = match strategy {
            Strategy::Container => ContainerProvider.acquire(&endpoint)?,
            Strategy::Existing => ExistingProvider.acquire(&endpoint)?,
        };

        Ok(Self {
            connection_string: endpoint.connection_string_at(port),
            endpoint,
            strategy,
            port,
            container_id,
            run_id: RunId::resolve(),
            ca_bundle_url,
        })
    }

    /// `dgraph://host:port[?sslmode=...]`, ready to hand to a client.
    pub fn connection_string(&self) -> &str {
        &self.connection_string
    }

    pub fn host(&self) -> &str {
        self.endpoint.host()
    }

    /// Where the instance actually is, which on the container path is what `docker_utils`
    /// reported rather than what was configured.
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub fn strategy(&self) -> Strategy {
        self.strategy
    }

    /// Whether this process owns the instance.
    ///
    /// Callers that wipe data MUST branch on this. A container this run started is
    /// [`Exclusivity::Exclusive`]; a Dgraph it merely found is shared with whoever else is using
    /// it and is [`Exclusivity::Shared`].
    pub fn exclusivity(&self) -> Exclusivity {
        self.strategy.exclusivity()
    }

    /// This run's correlation id.
    ///
    /// The reason a caller wants it is data scoping. Dgraph namespaces do NOT isolate without
    /// ACL -- a write inside namespace N is visible from namespace 0 -- so a suite sharing the
    /// fixture must keep out of other runs' way by naming its own data.
    pub fn run_id(&self) -> &RunId {
        &self.run_id
    }

    /// Where the CA that verifies this Dgraph is published, when [`CA_BUNDLE_URL_ENV`] names one.
    ///
    /// The URL only. This module never reads certificate material for itself: its health check
    /// runs with verification disabled because liveness is not an identity question.
    /// [`crate::utils_test::fixtures::fetch_ca_bundle`] is what a caller uses to fetch it.
    pub fn ca_bundle_url(&self) -> Option<&str> {
        self.ca_bundle_url.as_deref()
    }

    /// The ACL password for `groot` on this instance.
    ///
    /// The credential differs by strategy and nothing else does. A container this run started has
    /// a groot on Dgraph's own published default, which was never anybody's to choose and is not
    /// a secret. A Dgraph this run merely found has a password somebody chose, so it comes from
    /// [`ADMIN_PASSWORD_ENV`] and there is deliberately nothing to fall back to when it is unset.
    pub fn admin_password(&self) -> Result<String, FixtureError> {
        match self.strategy {
            Strategy::Container => Ok(DEFAULT_GROOT_PASSWORD.to_string()),
            Strategy::Existing => {
                env_var(ADMIN_PASSWORD_ENV).ok_or_else(FixtureError::missing_admin_password)
            }
        }
    }

    /// The connection string with extra parameters appended, for a caller that needs to add a
    /// CA path or select a namespace.
    pub fn connection_string_with(&self, params: &[(&str, &str)]) -> String {
        let mut out = self.connection_string.clone();
        for (key, value) in params {
            out.push(if out.contains('?') { '&' } else { '?' });
            out.push_str(key);
            out.push('=');
            out.push_str(value);
        }
        out
    }

    /// The connection string with ACL credentials and extra parameters.
    ///
    /// Pure string assembly -- this module neither logs in nor holds a session. The caller owns
    /// the credential and decides which identity to use; all that happens here is
    /// percent-encoding it into the userinfo the client parses.
    pub fn connection_string_as(
        &self,
        username: &str,
        password: &str,
        params: &[(&str, &str)],
    ) -> String {
        // The password can contain anything a generator produced, and ':' or '@' inside it
        // would otherwise re-split the authority.
        let user = encode(username);
        let pass = encode(password);
        let rest = self
            .connection_string
            .strip_prefix("dgraph://")
            .unwrap_or(&self.connection_string);

        let mut out = format!("dgraph://{user}:{pass}@{rest}");
        for (key, value) in params {
            out.push(if out.contains('?') { '&' } else { '?' });
            out.push_str(key);
            out.push('=');
            out.push_str(value);
        }
        out
    }

    /// Container id, when one was created. `None` on the pre-existing path.
    pub fn container_id(&self) -> Option<&str> {
        self.container_id.as_deref()
    }

    /// One printable line naming what was obtained and how.
    pub fn describe(&self) -> String {
        format!(
            "{}:{} via {} ({:?}, {}={}), run={}{}",
            self.host(),
            self.port,
            self.strategy.as_str(),
            self.exclusivity(),
            STRATEGY_ENV,
            self.strategy.as_str(),
            self.run_id.as_str(),
            if self.run_id.is_supplied() {
                ""
            } else {
                " (local)"
            }
        )
    }
}

/// Percent-encode the characters that would otherwise re-split a `dgraph://` authority.
fn encode(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            ':' => "%3A".to_string(),
            '@' => "%40".to_string(),
            '/' => "%2F".to_string(),
            '?' => "%3F".to_string(),
            '#' => "%23".to_string(),
            other => other.to_string(),
        })
        .collect()
}
