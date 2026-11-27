//! Seam use for the secret store's set-up (specs/004 research.md R5): the
//! only module where the CLI reaches Infisical, over its loopback port,
//! and only to bootstrap it. It creates the instance admin, project `sesh`
//! and machine identity `sesh-gateway` (universal auth, project member),
//! and never reads or writes a credential.

use crate::host::http::Http;
use crate::secrets::{self, Secrets};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub const ADMIN_EMAIL: &str = "admin@sesh.local";

pub fn base_url() -> String {
    let port = std::env::var("SESH_INFISICAL_PORT").unwrap_or_else(|_| "8222".into());
    format!("http://127.0.0.1:{port}")
}

fn ok(r: crate::host::http::Reply, what: &str) -> Result<Value> {
    if r.status != 200 {
        bail!(
            "Infisical refused to {what} ({}): {}",
            r.status,
            r.body.to_string().chars().take(200).collect::<String>()
        );
    }
    Ok(r.body)
}

fn field(v: &Value, path: &str, what: &str) -> Result<String> {
    v.pointer(path)
        .and_then(Value::as_str)
        .map(str::to_string)
        .with_context(|| format!("Infisical's {what} reply had no {path}"))
}

/// Bootstraps Infisical once. Returns true if it did (the gateway must
/// then restart to pick up its credentials).
pub fn ensure(http: &dyn Http, secrets: &mut Secrets) -> Result<bool> {
    if secrets
        .get(secrets::INFISICAL_GATEWAY_CLIENT_SECRET)
        .is_some()
    {
        return Ok(false);
    }
    let url = base_url();
    fn get(secrets: &Secrets, k: &str) -> Result<String> {
        secrets
            .get(k)
            .map(str::to_string)
            .with_context(|| format!("{k} missing; run `sesh init`"))
    }
    let admin = match secrets.get(secrets::INFISICAL_ADMIN_TOKEN) {
        Some(t) => t.to_string(),
        None => {
            let body = json!({"email": ADMIN_EMAIL, "password": get(secrets, "INFISICAL_ADMIN_PASSWORD")?,
                "organization": "sesh", "setupToken": get(secrets, "INFISICAL_SETUP_TOKEN")?});
            let b = ok(
                http.post(&format!("{url}/api/v1/admin/bootstrap"), None, &body)?,
                "bootstrap",
            )?;
            let token = field(&b, "/identity/credentials/token", "bootstrap")?;
            let org = field(&b, "/organization/id", "bootstrap")?;
            secrets.set(secrets::INFISICAL_ADMIN_TOKEN, &token);
            secrets.set("INFISICAL_ORG_ID", &org);
            token
        }
    };
    let org = get(secrets, "INFISICAL_ORG_ID")?;
    let auth = Some(admin.as_str());
    let project = match secrets.get(secrets::INFISICAL_PROJECT_ID) {
        Some(p) => p.to_string(),
        None => {
            let b = ok(
                http.post(
                    &format!("{url}/api/v1/projects"),
                    auth,
                    &json!({"projectName": "sesh", "type": "secret-manager"}),
                )?,
                "create the project",
            )?;
            let id = field(&b, "/project/id", "project")?;
            secrets.set(secrets::INFISICAL_PROJECT_ID, &id);
            id
        }
    };
    let b = ok(
        http.post(
            &format!("{url}/api/v1/identities"),
            auth,
            &json!({"name": "sesh-gateway", "organizationId": org, "role": "no-access"}),
        )?,
        "create the gateway identity",
    )?;
    let identity = field(&b, "/identity/id", "identity")?;
    let b = ok(
        http.post(
            &format!("{url}/api/v1/auth/universal-auth/identities/{identity}"),
            auth,
            &json!({}),
        )?,
        "enable universal auth",
    )?;
    let client_id = field(&b, "/identityUniversalAuth/clientId", "universal auth")?;
    let b = ok(
        http.post(
            &format!("{url}/api/v1/auth/universal-auth/identities/{identity}/client-secrets"),
            auth,
            &json!({"description": "sesh gateway"}),
        )?,
        "create the gateway's client secret",
    )?;
    let client_secret = field(&b, "/clientSecret", "client secret")?;
    ok(
        http.post(
            &format!("{url}/api/v1/projects/{project}/memberships/identities/{identity}"),
            auth,
            &json!({"role": "member"}),
        )?,
        "give the gateway access to the project",
    )?;
    secrets.set(secrets::INFISICAL_GATEWAY_CLIENT_ID, &client_id);
    secrets.set(secrets::INFISICAL_GATEWAY_CLIENT_SECRET, &client_secret);
    Ok(true)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::host::fake::FakeHttp;

    fn fake() -> FakeHttp {
        FakeHttp::new(|_, url, _| reply(url).unwrap_or((404, json!({}))))
    }

    /// Infisical's answers to the bootstrap calls, for other modules' fakes.
    pub fn reply(url: &str) -> Option<(u16, Value)> {
        {
            let path = url.split_once("8222").map(|(_, p)| p)?;
            let body = match path {
                "/api/v1/admin/bootstrap" => {
                    json!({"identity": {"credentials": {"token": "admin-tok"}}, "organization": {"id": "org1"}})
                }
                "/api/v1/projects" => json!({"project": {"id": "proj1"}}),
                "/api/v1/identities" => json!({"identity": {"id": "ident1"}}),
                "/api/v1/auth/universal-auth/identities/ident1" => {
                    json!({"identityUniversalAuth": {"clientId": "cid"}})
                }
                "/api/v1/auth/universal-auth/identities/ident1/client-secrets" => {
                    json!({"clientSecret": "csec"})
                }
                "/api/v1/projects/proj1/memberships/identities/ident1" => {
                    json!({"identityMembership": {}})
                }
                _ => return None,
            };
            Some((200, body))
        }
    }

    #[test]
    fn bootstraps_once_and_records_only_what_the_gateway_needs() {
        let mut s = Secrets::default();
        s.fill_missing().unwrap();
        let http = fake();
        assert!(ensure(&http, &mut s).unwrap());
        assert_eq!(s.get(secrets::INFISICAL_PROJECT_ID), Some("proj1"));
        assert_eq!(s.get(secrets::INFISICAL_GATEWAY_CLIENT_ID), Some("cid"));
        assert_eq!(
            s.get(secrets::INFISICAL_GATEWAY_CLIENT_SECRET),
            Some("csec")
        );
        assert_eq!(s.get(secrets::INFISICAL_ADMIN_TOKEN), Some("admin-tok"));
        let calls = http.calls.borrow().len();
        assert!(!ensure(&http, &mut s).unwrap(), "already done");
        assert_eq!(http.calls.borrow().len(), calls, "no calls the second time");
    }

    #[test]
    fn a_refusal_names_the_step() {
        let mut s = Secrets::default();
        s.fill_missing().unwrap();
        let http = FakeHttp::new(|_, _, _| (403, json!({"message": "nope"})));
        let e = ensure(&http, &mut s).unwrap_err().to_string();
        assert!(e.contains("bootstrap") && e.contains("403"), "{e}");
    }
}
