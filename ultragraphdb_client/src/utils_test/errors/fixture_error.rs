//! Why a Dgraph could not be obtained.

use crate::utils_test::types::strategy::Strategy;
use crate::utils_test::{ADMIN_PASSWORD_ENV, STRATEGY_ENV};
use std::fmt;

/// The error of every fallible operation in this module.
///
/// The classification is wrapped rather than public so variants can be added without breaking
/// callers; branch on [`FixtureError::kind`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureError(FixtureErrorEnum);

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FixtureErrorEnum {
    /// An environment variable does not describe a usable endpoint.
    Configuration(String),
    /// [`STRATEGY_ENV`] names something that is not a strategy.
    UnsupportedStrategy { value: String },
    /// A credential the chosen strategy needs is not set.
    MissingCredential { variable: String },
    /// `docker_utils` could not provide a container. Its error is a newtype over `String`
    /// upstream, so this is where the text survives; do not branch on its contents.
    Docker(String),
    /// The endpoint never became healthy inside its budget.
    NotReady {
        host: String,
        port: u16,
        attempts: u32,
        last: String,
    },
    /// The health endpoint answered, but not with something this module can read.
    Health { url: String, detail: String },
}

impl FixtureError {
    pub fn new(kind: FixtureErrorEnum) -> Self {
        Self(kind)
    }

    pub fn kind(&self) -> &FixtureErrorEnum {
        &self.0
    }

    pub fn is_not_ready(&self) -> bool {
        matches!(self.0, FixtureErrorEnum::NotReady { .. })
    }

    pub fn is_unsupported_strategy(&self) -> bool {
        matches!(self.0, FixtureErrorEnum::UnsupportedStrategy { .. })
    }

    pub fn is_missing_credential(&self) -> bool {
        matches!(self.0, FixtureErrorEnum::MissingCredential { .. })
    }
}

impl fmt::Display for FixtureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            FixtureErrorEnum::Configuration(detail) => {
                write!(f, "the dgraph endpoint could not be resolved: {detail}")
            }
            // Names the whole rule rather than just the refusal: the reader's next question is
            // always "then which ones DO work", and the answer is short enough to just say.
            FixtureErrorEnum::UnsupportedStrategy { value } => write!(
                f,
                "{STRATEGY_ENV}={value:?} is not a strategy. Use 'container' (provisions a \
                 dgraph/standalone container, the default) or 'existing' (verifies a Dgraph \
                 this process did not create)."
            ),
            FixtureErrorEnum::MissingCredential { variable } => write!(
                f,
                "{variable} is unset. A Dgraph this process did not create has an ACL password \
                 chosen by whoever deployed it, so there is nothing sensible to default to; \
                 export it, or use {STRATEGY_ENV}=container, whose groot is Dgraph's own \
                 published default."
            ),
            FixtureErrorEnum::Docker(detail) => {
                write!(f, "docker could not provide Dgraph: {detail}")
            }
            FixtureErrorEnum::NotReady {
                host,
                port,
                attempts,
                last,
            } => write!(
                f,
                "{host}:{port} did not become healthy after {attempts} attempt(s); last: {last}"
            ),
            FixtureErrorEnum::Health { url, detail } => {
                write!(
                    f,
                    "{url} did not answer with a usable health report: {detail}"
                )
            }
        }
    }
}

impl std::error::Error for FixtureError {}

impl FixtureError {
    pub(crate) fn configuration(detail: impl Into<String>) -> Self {
        Self(FixtureErrorEnum::Configuration(detail.into()))
    }

    pub(crate) fn unsupported_strategy(value: impl Into<String>) -> Self {
        Self(FixtureErrorEnum::UnsupportedStrategy {
            value: value.into(),
        })
    }

    /// The one credential this module can need, named so the message can say which.
    pub(crate) fn missing_admin_password() -> Self {
        Self(FixtureErrorEnum::MissingCredential {
            variable: ADMIN_PASSWORD_ENV.to_string(),
        })
    }

    pub(crate) fn docker(detail: impl Into<String>) -> Self {
        Self(FixtureErrorEnum::Docker(detail.into()))
    }

    pub(crate) fn not_ready(host: &str, port: u16, attempts: u32, last: impl Into<String>) -> Self {
        Self(FixtureErrorEnum::NotReady {
            host: host.to_string(),
            port,
            attempts,
            last: last.into(),
        })
    }

    pub(crate) fn health(url: impl Into<String>, detail: impl Into<String>) -> Self {
        Self(FixtureErrorEnum::Health {
            url: url.into(),
            detail: detail.into(),
        })
    }
}

/// Strategy is carried in errors often enough to warrant the conversion being obvious.
impl From<Strategy> for &'static str {
    fn from(strategy: Strategy) -> Self {
        strategy.as_str()
    }
}
