/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_role::RoleError;
use ultragraphdb_transport_http_client::{HttpClient, HttpUrl, read_key};
use ultragraphdb_transport_tls::TlsTrust;

use crate::types::role_kind::RoleKind;

const URL: &str = "ULTRAGRAPHDB_CONFIG_URL";
const TARGET: &str = "ULTRAGRAPHDB_CONFIG_TARGET";
const INSTANCE: &str = "ULTRAGRAPHDB_CONFIG_INSTANCE";
const KEY_FILE: &str = "ULTRAGRAPHDB_CONFIG_KEY_FILE";
const CA_FILE: &str = "ULTRAGRAPHDB_CONFIG_CA_FILE";

/// The revision a role loading one starts at, which it sets on itself as it executes itself again. A crash loses it, so a
/// role restarted by its supervisor returns to the active revision.
pub(crate) const REVISION: &str = "ULTRAGRAPHDB_CONFIG_REVISION";

/// What a role's environment says: which service to ask, as what. Nothing else about a role comes from its environment.
#[derive(Debug, Clone)]
pub struct Bootstrap {
    client: HttpClient,
    target: String,
    instance: String,
    revision: Option<u64>,
}

impl Bootstrap {
    /// The bootstrap of a role of `kind` from the environment `variables`. Variables not beginning `ULTRAGRAPHDB_` are someone
    /// else's.
    ///
    /// # Errors
    ///
    /// [`RoleError::configuration`] naming a variable this role does not take, one it requires and lacks, an instance of
    /// another role, a URL it cannot read, or a key or CA file it cannot use.
    pub fn read(
        kind: RoleKind,
        variables: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, RoleError> {
        let (mut url, mut target, mut instance, mut key_file, mut ca_file) =
            (None, None, None, None, None);
        let mut revision = None;
        for (name, value) in variables {
            match name.as_str() {
                URL => url = Some(value),
                TARGET => target = Some(value),
                INSTANCE => instance = Some(value),
                KEY_FILE => key_file = Some(value),
                CA_FILE => ca_file = Some(value),
                REVISION => revision = Some(value),
                other if other.starts_with("ULTRAGRAPHDB_") => {
                    return Err(RoleError::configuration(format!(
                        "the variable {other} is not one a role takes: a role takes {URL}, {TARGET}, {INSTANCE}, {KEY_FILE} and {CA_FILE}, and every setting comes from the config service"
                    )));
                }
                _ => {}
            }
        }
        let required = |value: Option<String>, name: &str| {
            value
                .ok_or_else(|| RoleError::configuration(format!("the variable {name} is required")))
        };
        let revision = revision
            .map(|number| {
                number.parse().map_err(|_| {
                    RoleError::configuration(format!(
                        "the variable {REVISION} holds {number}, which is not a revision's number"
                    ))
                })
            })
            .transpose()?;
        let url = required(url, URL)?;
        let target = required(target, TARGET)?;
        let instance = required(instance, INSTANCE)?;
        let key_file = required(key_file, KEY_FILE)?;
        let role = kind.to_string();
        let own = instance == role
            || instance
                .strip_prefix(&role)
                .is_some_and(|label| label.len() > 1 && label.starts_with('_'));
        if !own {
            return Err(RoleError::configuration(format!(
                "the variable {INSTANCE} holds {instance}, which is not an instance of this binary's role, {role}: it must be {role}, or {role} followed by _ and a label"
            )));
        }
        let refused = |name: &str, reason: &dyn std::fmt::Display| {
            RoleError::configuration(format!("the variable {name}: {reason}"))
        };
        let parsed = HttpUrl::parse(&url).map_err(|error| refused(URL, &error))?;
        let trust = match (parsed.is_secure(), ca_file) {
            (true, None) => {
                return Err(RoleError::configuration(format!(
                    "the variable {CA_FILE} is required, since {URL} holds {url}, which asks for TLS"
                )));
            }
            (true, Some(file)) => {
                Some(TlsTrust::from_pem_file(&file).map_err(|error| refused(CA_FILE, &error))?)
            }
            (false, _) => None,
        };
        let key = read_key(&key_file).map_err(|error| refused(KEY_FILE, &error))?;
        let client = HttpClient::new(parsed, trust, key).map_err(|error| refused(URL, &error))?;
        Ok(Self {
            client,
            target,
            instance,
            revision,
        })
    }

    /// The revision a load named, which a start takes in place of the active one.
    #[must_use]
    pub fn revision(&self) -> Option<u64> {
        self.revision
    }

    /// The client asking the service.
    #[must_use]
    pub fn client(&self) -> &HttpClient {
        &self.client
    }

    /// The deployment target.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// This instance's name.
    #[must_use]
    pub fn instance(&self) -> &str {
        &self.instance
    }
}
