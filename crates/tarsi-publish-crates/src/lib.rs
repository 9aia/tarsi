//! tarsi-publish-crates — crates.io publisher. Runs
//! `cargo publish --manifest-path <unit>/Cargo.toml` per unit;
//! the *caller* supplies units already in dependency order
//! (plan_release topo-sorts). Retries crates.io 429/rate-limit
//! responses with exponential backoff + jitter, honoring a
//! server `Retry-After` hint when cargo surfaces one; each
//! attempt is bounded by a wall-clock timeout (verify builds on
//! heavy crates can run for minutes — I-003). "Already exists"
//! counts as success (idempotent re-runs — release.sh semantics).

use std::path::PathBuf;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tarsi_core::PublishTarget;
use tarsi_publish::{
    CuoraTokenResolver, PublishError, PublishJob, PublishReport, Publisher, TokenResolver,
};

/// The env var cargo reads for the default-registry token — a
/// resolved token is always injected under this name regardless
/// of which `token_env` fallback supplied it.
pub const CARGO_TOKEN_ENV: &str = "CARGO_REGISTRY_TOKEN";

/// Default per-attempt wall-clock cap for `cargo publish` —
/// covers the package verify build plus upload on heavy crates
/// (tamias-serve/tamias-tui exceeded shorter limits, I-003).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Default base delay for 429 retries — doubles per attempt up
/// to [`DEFAULT_RETRY_MAX`]. crates.io asks bulk publishers to
/// slow down by minutes, not seconds.
pub const DEFAULT_RETRY_BASE: Duration = Duration::from_secs(30);
/// Default ceiling for computed 429 backoff. A server
/// `Retry-After` hint is honored verbatim and may exceed this.
pub const DEFAULT_RETRY_MAX: Duration = Duration::from_secs(5 * 60);

/// crates.io publisher. `repo_root` anchors relative manifest dirs.
#[derive(Clone)]
pub struct CratesPublisher {
    pub repo_root: PathBuf,
    /// Max attempts per unit when retry_429 is on.
    pub max_attempts: u32,
    pub retry_429: bool,
    /// Per-attempt wall-clock cap for one `cargo publish`
    /// invocation (verify build + upload). `Duration::ZERO`
    /// disables the cap.
    pub timeout: Duration,
    /// Base delay between 429 retries — doubles each attempt.
    pub retry_base: Duration,
    /// Ceiling for computed 429 backoff delays.
    pub retry_max: Duration,
    /// Env var NAME consulted for the token when `token_ref` is
    /// unset or misses (default `CARGO_REGISTRY_TOKEN`).
    pub token_env: String,
    /// Optional `cuora://<scope>/<name>` secret ref — takes
    /// precedence over `token_env`.
    pub token_ref: Option<String>,
    /// Token resolver — defaults to [`CuoraTokenResolver`]; tests
    /// inject a fake.
    pub resolver: Arc<dyn TokenResolver>,
}

impl CratesPublisher {
    #[must_use]
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            max_attempts: 5,
            retry_429: true,
            timeout: DEFAULT_TIMEOUT,
            retry_base: DEFAULT_RETRY_BASE,
            retry_max: DEFAULT_RETRY_MAX,
            token_env: CARGO_TOKEN_ENV.to_string(),
            token_ref: None,
            resolver: Arc::new(CuoraTokenResolver::new()),
        }
    }
}

/// crates.io rate-limit / 429 detection. Scans the *full*
/// stderr — cargo may print lock-wait noise ahead of the real
/// error, and the observed bulk-release throttle message is
/// "too many updates to existing crates" (I-003), which matches
/// neither "429" nor "too many requests".
fn is_rate_limited(stderr: &str) -> bool {
    let s = stderr.to_lowercase();
    s.contains("429")
        || s.contains("rate limit")
        || s.contains("rate-limit")
        || s.contains("rate_limit")
        || s.contains("too many")
}

fn is_already_published(stderr: &str) -> bool {
    stderr.contains("already exists")
        || stderr.contains("is already uploaded")
        || stderr.contains("previously uploaded")
}

/// Server-directed retry hint. cargo doesn't always surface the
/// `Retry-After` header, but when it does (or when crates.io's
/// message says "try again in N seconds") we honor it verbatim —
/// it beats any computed backoff.
fn parse_retry_after(stderr: &str) -> Option<u64> {
    for line in stderr.lines() {
        let l = line.to_lowercase();
        if !(l.contains("retry") || l.contains("try again")) {
            continue;
        }
        // Last integer on the line is the delay in seconds
        // ("Retry-After: 120", "try again in 60 seconds").
        let mut last = None;
        let mut cur = String::new();
        for c in l.chars() {
            if c.is_ascii_digit() {
                cur.push(c);
            } else {
                if !cur.is_empty() {
                    last = cur.parse::<u64>().ok().or(last);
                    cur.clear();
                }
            }
        }
        if !cur.is_empty() {
            last = cur.parse::<u64>().ok().or(last);
        }
        if let Some(n) = last {
            if n > 0 {
                return Some(n);
            }
        }
    }
    None
}

/// Exponential backoff with jitter: `base * 2^(attempt-1)` capped
/// at `max`, plus up to 25% jitter so parallel publishers don't
/// retry in lockstep. A `Retry-After` hint in `stderr` wins.
fn retry_delay(attempt: u32, stderr: &str, base: Duration, max: Duration) -> Duration {
    if let Some(secs) = parse_retry_after(stderr) {
        return Duration::from_secs(secs);
    }
    let capped = base
        .saturating_mul(1u32 << attempt.saturating_sub(1).min(20))
        .min(max);
    // Jitter: up to +25% of the capped delay, from wall-clock
    // nanos (no rng dep for a release tool).
    let span = capped.as_nanos() / 4 + 1;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u128)
        .unwrap_or(0);
    capped + Duration::from_nanos((nanos % span) as u64)
}

/// One finished `cargo publish` invocation — exit status plus
/// captured output (stderr is what carries the diagnostics).
struct Captured {
    /// `None` when the attempt was killed on timeout.
    status: Option<ExitStatus>,
    stderr: String,
}

/// How one attempt ended.
enum Attempt {
    Exited(Captured),
    TimedOut(Captured),
}

/// Spawn `cmd` with output redirected to temp files, poll for
/// exit, and enforce `timeout` (`Duration::ZERO` = no cap).
///
/// Files — not pipes — carry the output: a killed `cargo` can
/// leave grandchildren (rustc, the registry client) holding pipe
/// write-ends open, which would block a `read_to_end` join long
/// after the timeout fired (observed: a 500ms cap still waited
/// out the child's full 30s sleep). File reads return whatever
/// was written and never block on a still-open fd.
fn run_attempt(mut cmd: Command, timeout: Duration) -> Result<Attempt, PublishError> {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    let base = std::env::temp_dir().join(format!("tarsi-publish-{}-{seq}", std::process::id()));
    let out_path = base.with_extension("out");
    let err_path = base.with_extension("err");
    cmd.stdout(Stdio::from(std::fs::File::create(&out_path)?))
        .stderr(Stdio::from(std::fs::File::create(&err_path)?));
    let mut child = cmd.spawn()?;

    let deadline = (!timeout.is_zero()).then(|| Instant::now() + timeout);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    // cargo diagnostics live on stderr; stdout is drained-and-
    // dropped (a verify build's compile log is noise here).
    let stderr_bytes = std::fs::read(&err_path).unwrap_or_default();
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    let captured = Captured {
        status,
        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
    };
    Ok(match status {
        Some(_) => Attempt::Exited(captured),
        None => Attempt::TimedOut(captured),
    })
}

/// Tail of stderr for error details — lock-wait noise leads,
/// the real error trails (I-003 truncated details hid it).
fn stderr_tail(stderr: &str, lines: usize) -> String {
    let all: Vec<&str> = stderr.lines().collect();
    all.iter()
        .rev()
        .take(lines)
        .rev()
        .copied()
        .collect::<Vec<_>>()
        .join(" | ")
}

impl Publisher for CratesPublisher {
    fn target(&self) -> PublishTarget {
        PublishTarget::CratesIo
    }

    fn publish(&self, job: &PublishJob) -> Result<PublishReport, PublishError> {
        let manifest = self.repo_root.join(&job.manifest_dir).join("Cargo.toml");
        if job.dry_run {
            return Ok(PublishReport {
                unit: job.unit.clone(),
                target: self.target(),
                ok: true,
                detail: format!("dry-run: would cargo publish {}", manifest.display()),
            });
        }
        // Resolve the token once per unit — after the dry-run
        // early-return so dry-runs never touch cuora/env secrets.
        let token = self
            .resolver
            .resolve(&self.token_env, self.token_ref.as_deref())?;
        let attempts = if self.retry_429 { self.max_attempts } else { 1 };
        let mut last_err = String::new();
        for attempt in 1..=attempts {
            let mut cmd = Command::new("cargo");
            cmd.arg("publish").arg("--manifest-path").arg(&manifest);
            // Inject under the name cargo actually reads — the
            // configured `token_env` only names the fallback.
            if let Some(t) = &token {
                cmd.env(CARGO_TOKEN_ENV, t);
            }
            match run_attempt(cmd, self.timeout)? {
                Attempt::TimedOut(c) => {
                    return Err(PublishError::Msg(format!(
                        "cargo publish {} timed out after {:?}: {}",
                        job.unit,
                        self.timeout,
                        stderr_tail(&c.stderr, 8)
                    )));
                }
                Attempt::Exited(c) if c.status.is_some_and(|s| s.success()) => {
                    return Ok(PublishReport {
                        unit: job.unit.clone(),
                        target: self.target(),
                        ok: true,
                        detail: format!("published {} (attempt {attempt})", job.version),
                    });
                }
                Attempt::Exited(c) => {
                    if is_already_published(&c.stderr) {
                        return Ok(PublishReport {
                            unit: job.unit.clone(),
                            target: self.target(),
                            ok: true,
                            detail: "already published".to_string(),
                        });
                    }
                    let rate_limited = is_rate_limited(&c.stderr);
                    last_err = stderr_tail(&c.stderr, 8);
                    if !(self.retry_429 && rate_limited) || attempt == attempts {
                        break;
                    }
                    let delay = retry_delay(attempt, &c.stderr, self.retry_base, self.retry_max);
                    std::thread::sleep(delay);
                }
            }
        }
        Err(PublishError::Msg(format!(
            "cargo publish {} failed: {last_err}",
            job.unit
        )))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_detection() {
        assert!(is_rate_limited("error: 429 Too Many Requests"));
        assert!(is_rate_limited("you have hit your rate limit"));
        // crates.io bulk-update throttle (I-003) — matches none
        // of the old needles.
        assert!(is_rate_limited(
            "error: too many updates to existing crates in a short period"
        ));
        assert!(!is_rate_limited("error: no such package"));
        assert!(is_already_published(
            "error: crate version 1.0.0 is already uploaded"
        ));
    }

    #[test]
    fn retry_after_parsing() {
        assert_eq!(parse_retry_after("Retry-After: 120"), Some(120));
        assert_eq!(
            parse_retry_after("please try again in 45 seconds"),
            Some(45)
        );
        assert_eq!(parse_retry_after("error: 429 no hint"), None);
        // "429" alone must not be read as a delay.
        assert_eq!(parse_retry_after("error: 429 Too Many Requests"), None);
    }

    #[test]
    fn backoff_grows_and_caps() {
        let base = Duration::from_secs(10);
        let max = Duration::from_secs(60);
        let d1 = retry_delay(1, "", base, max);
        let d3 = retry_delay(3, "", base, max);
        // attempt1: 10s (+jitter ≤2.5s); attempt3: 40s (+jitter ≤10s)
        assert!(d1 >= base && d1 < base + Duration::from_secs(3));
        assert!(d3 >= base * 4 && d3 < base * 4 + Duration::from_secs(11));
        // cap
        let d9 = retry_delay(9, "", base, max);
        assert!(d9 <= max + max / 4);
        // Retry-After wins over computation
        let hinted = retry_delay(1, "Retry-After: 300", base, max);
        assert_eq!(hinted, Duration::from_secs(300));
    }

    #[test]
    fn dry_run_never_spawns() {
        let p = CratesPublisher::new(PathBuf::from("/nonexistent"));
        let job = PublishJob {
            unit: "u".into(),
            version: "1.0.0".into(),
            manifest_dir: "crates/u".into(),
            tag: "u-v1.0.0".into(),
            notes: String::new(),
            assets: Vec::new(),
            commitish: None,
            dry_run: true,
        };
        let r = p.publish(&job).unwrap();
        assert!(r.ok && r.detail.contains("dry-run"));
    }

    fn job(dry_run: bool) -> PublishJob {
        PublishJob {
            unit: "u".into(),
            version: "1.0.0".into(),
            manifest_dir: "crates/u".into(),
            tag: "u-v1.0.0".into(),
            notes: String::new(),
            assets: Vec::new(),
            commitish: None,
            dry_run,
        }
    }

    /// Tempdir + PATH-scoped fake `cargo`: creates a tempdir
    /// containing an executable `cargo` running `script`, prepends
    /// the dir to PATH for the duration of `f`, restores after.
    /// Serialized by PATH_LOCK (env mutation).
    static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_fake_cargo<S, F>(script_for: S, f: F)
    where
        S: FnOnce(&std::path::Path) -> String,
        F: FnOnce(&PathBuf),
    {
        let _g = PATH_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("cargo");
        std::fs::write(&fake, script_for(dir.path())).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let old_path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{}:{old_path}", dir.path().display()));
        f(&dir.path().to_path_buf());
        std::env::set_var("PATH", old_path);
    }

    /// Resolver seam — fixed token or error, env ignored.
    struct FakeResolver(Result<Option<String>, String>);

    impl TokenResolver for FakeResolver {
        fn resolve(
            &self,
            _token_env: &str,
            _token_ref: Option<&str>,
        ) -> Result<Option<String>, PublishError> {
            match &self.0 {
                Ok(v) => Ok(v.clone()),
                Err(e) => Err(PublishError::Msg(e.clone())),
            }
        }
    }

    #[test]
    fn resolved_token_is_injected_as_cargo_token_env() {
        with_fake_cargo(
            |dir| {
                format!(
                    "#!/bin/sh\nprintf '%s' \"${{CARGO_REGISTRY_TOKEN:-}}\" > '{}'\nexit 0\n",
                    dir.join("token.txt").display()
                )
            },
            |dir| {
                let mut p = CratesPublisher::new(dir.to_path_buf());
                p.retry_429 = false;
                p.resolver = Arc::new(FakeResolver(Ok(Some("sekret-token".into()))));
                p.token_ref = Some("cuora://tamias/crates-token".into());
                // manifest_dir is repo-root-relative; point repo_root
                // at the tempdir so the (nonexistent) manifest path is
                // fine — the fake cargo never reads it.
                let r = p.publish(&job(false)).unwrap();
                assert!(r.ok, "{}", r.detail);
                assert_eq!(
                    std::fs::read_to_string(dir.join("token.txt")).unwrap(),
                    "sekret-token"
                );
            },
        );
    }

    #[test]
    fn resolver_error_fails_before_spawning() {
        let dir = tempfile::tempdir().unwrap();
        let mut p = CratesPublisher::new(dir.path().to_path_buf());
        p.retry_429 = false;
        p.resolver = Arc::new(FakeResolver(Err("bad token_ref".into())));
        let err = p.publish(&job(false)).unwrap_err();
        assert!(err.to_string().contains("bad token_ref"));
    }

    #[test]
    fn slow_publish_times_out() {
        with_fake_cargo(
            |_| "#!/bin/sh\nsleep 30\nexit 0\n".to_string(),
            |dir| {
                let mut p = CratesPublisher::new(dir.to_path_buf());
                p.retry_429 = false;
                p.timeout = Duration::from_millis(500);
                let err = p.publish(&job(false)).unwrap_err();
                let msg = err.to_string();
                assert!(msg.contains("timed out"), "{msg}");
            },
        );
    }

    #[test]
    fn rate_limited_publish_retries_then_succeeds() {
        // First call 429s (counted via marker file), second succeeds.
        with_fake_cargo(
            |dir| {
                let count = dir.join("count.txt");
                format!(
                    "#!/bin/sh\nn=$(cat '{}' 2>/dev/null || echo 0)\nn=$((n+1))\necho $n > '{}'\nif [ \"$n\" -eq 1 ]; then echo 'error: too many updates to existing crates' >&2; exit 1; fi\nexit 0\n",
                    count.display(),
                    count.display()
                )
            },
            |dir| {
                let mut p = CratesPublisher::new(dir.to_path_buf());
                p.max_attempts = 3;
                p.retry_base = Duration::from_millis(1);
                p.retry_max = Duration::from_millis(5);
                let r = p.publish(&job(false)).unwrap();
                assert!(r.ok, "{}", r.detail);
                assert!(r.detail.contains("attempt 2"), "{}", r.detail);
                assert_eq!(
                    std::fs::read_to_string(dir.join("count.txt"))
                        .unwrap()
                        .trim(),
                    "2"
                );
            },
        );
    }

    #[test]
    fn exhausted_attempts_fail_with_tail() {
        with_fake_cargo(
            |_| {
                "#!/bin/sh\necho 'Blocking waiting for file lock on package cache' >&2\necho 'error: too many updates to existing crates' >&2\nexit 1\n".to_string()
            },
            |dir| {
                let mut p = CratesPublisher::new(dir.to_path_buf());
                p.max_attempts = 2;
                p.retry_base = Duration::from_millis(1);
                p.retry_max = Duration::from_millis(5);
                let err = p.publish(&job(false)).unwrap_err();
                let msg = err.to_string();
                // The tail must carry the real error, not just the
                // leading lock-wait noise (I-003).
                assert!(msg.contains("too many updates"), "{msg}");
            },
        );
    }
}
