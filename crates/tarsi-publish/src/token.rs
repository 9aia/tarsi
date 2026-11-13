//! Publish-token resolution — the seam between config's secret
//! *names* (`token_env`) / refs (`token_ref`) and the values the
//! forges consume. Values are secret: they are never logged, never
//! placed in error messages, and never written anywhere but the
//! child process environment / `Authorization` header.
//!
//! Resolution order (T-041):
//!
//! 1. `token_ref` — an explicit `cuora://<scope>/<name>` secret
//!    ref, fetched through `hypha-secret-cuora` (audit events and
//!    vault policy apply). Nested scopes work (`cuora://a/b/name`
//!    → scope `a/b`, name `name` — the ref tail is the secret
//!    name).
//! 2. `token_env` — the environment variable of that name.
//!
//! A `cuora://` miss falls through to the env var (the org's
//! Secrets-chain semantics); an unsupported ref scheme is a hard
//! config error. With no `token_ref`, behavior is exactly the
//! pre-T-041 env lookup.
//!
//! Env knobs follow the `<PRODUCT>_CUORA_*` convention:
//! `TARSI_CUORA_BIN` overrides the `cuora` binary (tests stub it),
//! `TARSI_CUORA_TRANSPORT` picks `cli`/`serve`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use hypha_secret::Secrets as _;
use hypha_secret_cuora::CuoraSecrets;

use crate::PublishError;

/// Env var naming the `cuora` binary override.
pub const CUORA_BIN_ENV: &str = "TARSI_CUORA_BIN";
/// Env var picking the cuora transport (`cli`/`serve`).
pub const CUORA_TRANSPORT_ENV: &str = "TARSI_CUORA_TRANSPORT";

/// Resolves a publish token from config's `(token_env, token_ref)`
/// pair. Implementations must treat the returned value as secret.
pub trait TokenResolver: Send + Sync {
    /// Resolve the token. `token_ref` is an optional
    /// `cuora://<scope>/<name>` ref taking precedence over the env
    /// var named `token_env`. `Ok(None)` means no token is
    /// configured or found — the caller decides whether that's an
    /// error (publish) or fine (dry-run).
    ///
    /// # Errors
    /// `PublishError::Msg` on an unparseable/unsupported `token_ref`
    /// or a resolver-side failure. The secret *value* must never
    /// appear in the error.
    fn resolve(
        &self,
        token_env: &str,
        token_ref: Option<&str>,
    ) -> Result<Option<String>, PublishError>;
}

/// Split a `cuora://<scope>/<name>` ref into `(scope, name)` —
/// scope may itself contain `/` (e.g. `peregrin/push`), so the
/// split is at the *last* `/`. Rejects missing/empty parts.
fn parse_cuora_ref(r: &str) -> Result<(&str, &str), PublishError> {
    let rest = r
        .strip_prefix("cuora://")
        .ok_or_else(|| PublishError::Msg(format!("unsupported token_ref scheme: {r}")))?;
    let (scope, name) = rest.rsplit_once('/').ok_or_else(|| {
        PublishError::Msg(format!(
            "token_ref {r:?} has no secret name (want cuora://<scope>/<name>)"
        ))
    })?;
    if scope.is_empty() || name.is_empty() {
        return Err(PublishError::Msg(format!(
            "token_ref {r:?} has an empty scope or name"
        )));
    }
    Ok((scope, name))
}

/// The default resolver: `cuora://` refs via `hypha-secret-cuora`,
/// env var fallback. One [`CuoraSecrets`] store per scope, built
/// lazily and cached for the resolver's lifetime (cheap under the
/// `cli` transport; under `serve` it keeps the `cuora serve` child
/// alive across lookups).
#[derive(Default)]
pub struct CuoraTokenResolver {
    stores: Mutex<HashMap<String, CuoraSecrets>>,
    /// Test seam — when set, every scope's store uses this `cuora`
    /// binary (mirrors `CuoraSecrets::with_bin`).
    bin: Option<PathBuf>,
}

impl CuoraTokenResolver {
    /// A resolver honoring `TARSI_CUORA_BIN`/`TARSI_CUORA_TRANSPORT`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Override the `cuora` binary for every scope — the test seam.
    #[must_use]
    pub fn with_bin(mut self, bin: PathBuf) -> Self {
        self.bin = Some(bin);
        self
    }

    fn store(&self, scope: &str) -> CuoraSecrets {
        let store = CuoraSecrets::from_env(scope, CUORA_BIN_ENV, CUORA_TRANSPORT_ENV);
        match &self.bin {
            Some(b) => store.with_bin(b.clone()),
            None => store,
        }
    }

    fn cuora_get(&self, scope: &str, name: &str) -> Option<String> {
        let mut stores = self.stores.lock().ok()?;
        let store = stores
            .entry(scope.to_string())
            .or_insert_with(|| self.store(scope));
        store.get(name)
    }
}

impl TokenResolver for CuoraTokenResolver {
    fn resolve(
        &self,
        token_env: &str,
        token_ref: Option<&str>,
    ) -> Result<Option<String>, PublishError> {
        if let Some(r) = token_ref {
            let (scope, name) = parse_cuora_ref(r)?;
            if let Some(v) = self.cuora_get(scope, name) {
                return Ok(Some(v));
            }
            // cuora miss → fall through to the env var.
        }
        Ok(std::env::var(token_env).ok().filter(|v| !v.is_empty()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A fake `cuora` binary: records its argv to `$CAPTURE` and
    /// prints `VALUE` on `get`. The capture path is baked into the
    /// generated script.
    fn stub(dir: &std::path::Path, capture: &std::path::Path, value: &str) -> PathBuf {
        let script = format!(
            "#!/bin/sh\necho \"$@\" > '{}'\nprintf '%s\\n' '{}'\n",
            capture.display(),
            value
        );
        let bin = dir.join("cuora-stub.sh");
        std::fs::write(&bin, script).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    fn parses_nested_scope_refs() {
        assert_eq!(
            parse_cuora_ref("cuora://tamias/crates-token").unwrap(),
            ("tamias", "crates-token")
        );
        assert_eq!(
            parse_cuora_ref("cuora://peregrin/push/vapid").unwrap(),
            ("peregrin/push", "vapid")
        );
        assert!(parse_cuora_ref("https://x/y").is_err());
        assert!(parse_cuora_ref("cuora://noslash").is_err());
        assert!(parse_cuora_ref("cuora:///name").is_err());
        assert!(parse_cuora_ref("cuora://scope/").is_err());
    }

    #[test]
    fn token_ref_wins_over_env() {
        let dir = tempfile::tempdir().unwrap();
        let capture = dir.path().join("args.txt");
        let bin = stub(dir.path(), &capture, "ref-secret");
        std::env::set_var("TARSI_TEST_TOKEN_ENV_ONLY", "env-secret");
        let r = CuoraTokenResolver::new().with_bin(bin);
        let v = r
            .resolve(
                "TARSI_TEST_TOKEN_ENV_ONLY",
                Some("cuora://tamias/crates-token"),
            )
            .unwrap()
            .unwrap();
        assert_eq!(v, "ref-secret");
        // The CLI leg fetched exactly the configured ref.
        let args = std::fs::read_to_string(&capture).unwrap();
        assert!(args.contains("cuora://tamias/crates-token"), "{args}");
        assert!(args.contains("get"), "{args}");
    }

    #[test]
    fn env_is_the_fallback_and_default() {
        let dir = tempfile::tempdir().unwrap();
        // A failing cuora: ref misses, env still answers.
        let bin = dir.path().join("cuora-fail.sh");
        std::fs::write(&bin, "#!/bin/sh\nexit 1\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("TARSI_TEST_TOKEN_FALLBACK", "env-value");
        let r = CuoraTokenResolver::new().with_bin(bin);
        // token_ref set but cuora misses → env fallback.
        let v = r
            .resolve("TARSI_TEST_TOKEN_FALLBACK", Some("cuora://tamias/missing"))
            .unwrap()
            .unwrap();
        assert_eq!(v, "env-value");
        // No token_ref → plain env lookup (pre-T-041 behavior).
        let v = r
            .resolve("TARSI_TEST_TOKEN_FALLBACK", None)
            .unwrap()
            .unwrap();
        assert_eq!(v, "env-value");
        // Nothing configured anywhere → None.
        assert_eq!(r.resolve("TARSI_TEST_TOKEN_UNSET_XYZ", None).unwrap(), None);
    }
}
