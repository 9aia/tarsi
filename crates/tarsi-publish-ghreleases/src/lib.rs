//! tarsi-publish-ghreleases — GitHub Releases publisher. Creates a
//! release per unit tag with the rendered notes as the body, then
//! uploads the job's [`PublishAsset`]s to the release's `upload_url`
//! (the two-step flow: `POST /repos/{o}/{r}/releases` →
//! `POST {upload_url}?name=<file>` with the binary body).
//!
//! Idempotent re-runs: the release is looked up by tag first
//! (`GET /repos/{o}/{r}/releases/tags/{tag}`) and reused when it
//! exists; an asset whose name is already attached is DELETEd and
//! re-uploaded — replace semantics, latest build wins. Blocking
//! `ureq` per the bupha-github convention; the token comes from
//! the [`TokenResolver`] — `token_ref` (a `cuora://` secret ref)
//! first, then the env var named `token_env` — the value never
//! appears in config, logs, or args.

use std::path::Path;
use std::sync::Arc;

use tarsi_core::PublishTarget;
use tarsi_publish::{
    CuoraTokenResolver, PublishAsset, PublishError, PublishJob, PublishReport, Publisher,
    TokenResolver,
};

/// GitHub Releases publisher for `owner/repo`.
#[derive(Clone)]
pub struct GhReleasePublisher {
    /// `owner/repo` — resolved from git remote or config.
    pub repo: String,
    /// Env var NAME holding the token (e.g. `GITHUB_TOKEN`) —
    /// the fallback when `token_ref` is unset or misses.
    pub token_env: String,
    /// Optional `cuora://<scope>/<name>` secret ref — resolved via
    /// cuora at publish time; takes precedence over `token_env`.
    pub token_ref: Option<String>,
    /// Token resolver — defaults to [`CuoraTokenResolver`]; tests
    /// inject a fake.
    pub resolver: Arc<dyn TokenResolver>,
    /// API base — override for GHES/tests.
    pub api_base: String,
}

/// One release as returned by the API — the fields we consume.
#[derive(Debug, serde::Deserialize)]
struct ReleaseJson {
    id: u64,
    html_url: Option<String>,
    /// `…/releases/{id}/assets{?name,label}` — the template GitHub
    /// returns; we keep the base and append `?name=` ourselves.
    upload_url: Option<String>,
    #[serde(default)]
    assets: Vec<AssetJson>,
}

#[derive(Debug, serde::Deserialize)]
struct AssetJson {
    id: u64,
    name: String,
}

impl GhReleasePublisher {
    #[must_use]
    pub fn new(repo: String, token_env: String) -> Self {
        Self {
            repo,
            token_env,
            token_ref: None,
            resolver: Arc::new(CuoraTokenResolver::new()),
            api_base: "https://api.github.com".to_string(),
        }
    }

    fn token(&self) -> Result<String, PublishError> {
        match self
            .resolver
            .resolve(&self.token_env, self.token_ref.as_deref())?
        {
            Some(t) => Ok(t),
            None => Err(PublishError::Msg(format!(
                "{} is not set and no token_ref resolved — set the env var or configure token_ref",
                self.token_env
            ))),
        }
    }

    fn auth(&self, token: &str) -> [(&'static str, String); 4] {
        [
            ("Authorization", format!("Bearer {token}")),
            ("Accept", "application/vnd.github+json".to_string()),
            ("X-GitHub-Api-Version", "2022-11-28".to_string()),
            ("User-Agent", "tarsi".to_string()),
        ]
    }

    /// GET the release for `tag`; `Ok(None)` on 404.
    fn find_release(&self, token: &str, tag: &str) -> Result<Option<ReleaseJson>, PublishError> {
        let url = format!(
            "{}/repos/{}/releases/tags/{}",
            self.api_base,
            self.repo,
            urlencoding(tag)
        );
        let mut req = ureq::get(&url);
        for (k, v) in self.auth(token) {
            req = req.header(k, &v);
        }
        match req.call() {
            Ok(resp) => {
                let rel: ReleaseJson = resp
                    .into_body()
                    .read_json()
                    .map_err(|e| PublishError::Http(e.to_string()))?;
                Ok(Some(rel))
            }
            Err(ureq::Error::StatusCode(404)) => Ok(None),
            Err(e) => Err(PublishError::Http(e.to_string())),
        }
    }

    /// POST a new release; returns the created release JSON.
    fn create_release(&self, token: &str, job: &PublishJob) -> Result<ReleaseJson, PublishError> {
        let url = format!("{}/repos/{}/releases", self.api_base, self.repo);
        let mut body = serde_json::json!({
            "tag_name": job.tag,
            "name": format!("{} {}", job.unit, job.version),
            "body": job.notes,
            "draft": false,
            "prerelease": false,
        });
        // Pin the commit the tag resolves to — without this GitHub
        // creates a missing tag at the default-branch HEAD.
        if let Some(c) = &job.commitish {
            body["target_commitish"] = serde_json::json!(c);
        }
        let mut req = ureq::post(&url);
        for (k, v) in self.auth(token) {
            req = req.header(k, &v);
        }
        let resp = req
            .send_json(&body)
            .map_err(|e| PublishError::Http(e.to_string()))?;
        resp.into_body()
            .read_json()
            .map_err(|e| PublishError::Http(e.to_string()))
    }

    /// Upload one asset. `upload_base` is the release's `upload_url`
    /// minus the `{?name,label}` template. Same-name assets on the
    /// release are deleted first (replace semantics).
    fn upload_asset(
        &self,
        token: &str,
        release: &ReleaseJson,
        asset: &PublishAsset,
    ) -> Result<(), PublishError> {
        // Replace: drop any existing asset with this name.
        for existing in &release.assets {
            if existing.name == asset.name {
                let url = format!(
                    "{}/repos/{}/releases/assets/{}",
                    self.api_base, self.repo, existing.id
                );
                let mut req = ureq::delete(&url);
                for (k, v) in self.auth(token) {
                    req = req.header(k, &v);
                }
                req.call()
                    .map_err(|e| PublishError::Http(format!("delete asset: {e}")))?;
            }
        }

        let bytes = std::fs::read(&asset.path)?;
        let upload_base = release
            .upload_url
            .as_deref()
            .map(|u| {
                u.split('{')
                    .next()
                    .unwrap_or(u)
                    .trim_end_matches('?')
                    .to_string()
            })
            .unwrap_or_else(|| {
                format!(
                    "https://uploads.github.com/repos/{}/releases/{}/assets",
                    self.repo, release.id
                )
            });
        let url = format!("{}?name={}", upload_base, urlencoding(&asset.name));
        let content_type = asset
            .content_type
            .clone()
            .unwrap_or_else(|| guess_content_type(&asset.name).to_string());
        let mut req = ureq::post(&url);
        for (k, v) in self.auth(token) {
            req = req.header(k, &v);
        }
        req.content_type(&content_type)
            .send(&bytes[..])
            .map_err(|e| PublishError::Http(format!("upload {}: {e}", asset.name)))?;
        Ok(())
    }
}

/// Minimal percent-encoding for the path/query segments we put in
/// URLs — tags, asset names, repo path. Keeps `+`, `/`, `?`, `&`,
/// `%`, `#` and whitespace safe without pulling in a url crate.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Best-effort `Content-Type` for an asset name — used when the
/// asset spec doesn't declare one. Falls back to octet-stream.
#[must_use]
pub fn guess_content_type(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".tar.gz") || lower.ends_with(".tgz") {
        "application/gzip"
    } else if lower.ends_with(".tar") {
        "application/x-tar"
    } else if lower.ends_with(".zip") {
        "application/zip"
    } else if lower.ends_with(".gz") {
        "application/gzip"
    } else if lower.ends_with(".json") {
        "application/json"
    } else if lower.ends_with(".md")
        || lower.ends_with(".txt")
        || lower.ends_with(".sha256")
        || lower.ends_with(".sig")
    {
        "text/plain; charset=utf-8"
    } else {
        "application/octet-stream"
    }
}

/// Expand asset glob patterns into concrete [`PublishAsset`]s.
///
/// Patterns are repo-root-relative; `{unit}` and `{version}` are
/// substituted before globbing (so `dist/{unit}-{version}-*`
/// resolves per unit). A pattern with no glob magic must match an
/// existing file — a typo fails loudly. Magic patterns that match
/// nothing are skipped (artifacts are build-dependent). Matches
/// are deduplicated by canonical path and sorted by name for
/// deterministic uploads.
pub fn expand_assets(
    repo_root: &Path,
    patterns: &[String],
    unit: &str,
    version: &str,
) -> Result<Vec<PublishAsset>, PublishError> {
    let mut out: Vec<PublishAsset> = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for pat in patterns {
        let rendered = pat.replace("{unit}", unit).replace("{version}", version);
        let has_magic = rendered.contains('*')
            || rendered.contains('?')
            || rendered.contains('[')
            || rendered.contains('{');
        let full = repo_root.join(&rendered);
        let mut matched = 0usize;
        for entry in glob::glob(&full.to_string_lossy())
            .map_err(|e| PublishError::Msg(format!("bad asset pattern {pat:?}: {e}")))?
        {
            let path = entry.map_err(|e| PublishError::Msg(format!("glob {pat:?}: {e}")))?;
            if !path.is_file() {
                continue;
            }
            matched += 1;
            let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !seen.insert(canon) {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| rendered.clone());
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            out.push(PublishAsset {
                path,
                name,
                content_type: None,
                size,
            });
        }
        if !has_magic && matched == 0 {
            return Err(PublishError::Msg(format!(
                "asset {pat:?} (as {rendered:?}) matched no file under {}",
                repo_root.display()
            )));
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    // Asset names must be unique on a release — two matches with
    // the same basename would 422 on the second upload.
    out.dedup_by(|a, b| a.name == b.name);
    Ok(out)
}

impl Publisher for GhReleasePublisher {
    fn target(&self) -> PublishTarget {
        PublishTarget::GithubRelease
    }

    fn publish(&self, job: &PublishJob) -> Result<PublishReport, PublishError> {
        if job.dry_run {
            let assets = job
                .assets
                .iter()
                .map(|a| {
                    format!(
                        "{} ({} bytes, {})",
                        a.name,
                        a.size,
                        a.content_type
                            .clone()
                            .unwrap_or_else(|| guess_content_type(&a.name).to_string())
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let detail = if assets.is_empty() {
                format!(
                    "dry-run: would create GH release {} on {}",
                    job.tag, self.repo
                )
            } else {
                format!(
                    "dry-run: would create GH release {} on {} with assets: {}",
                    job.tag, self.repo, assets
                )
            };
            return Ok(PublishReport {
                unit: job.unit.clone(),
                target: self.target(),
                ok: true,
                detail,
            });
        }
        let token = self.token()?;
        // Reuse an existing release for the tag (idempotent re-run)
        // or create one.
        let (release, created) = match self.find_release(&token, &job.tag)? {
            Some(rel) => (rel, false),
            None => (self.create_release(&token, job)?, true),
        };
        for asset in &job.assets {
            self.upload_asset(&token, &release, asset)?;
        }
        let html = release
            .html_url
            .clone()
            .unwrap_or_else(|| format!("{}/releases/tag/{}", self.repo, job.tag));
        let suffix = if created {
            String::new()
        } else {
            format!(
                " (existing release reused; {} assets replaced/added)",
                job.assets.len()
            )
        };
        Ok(PublishReport {
            unit: job.unit.clone(),
            target: self.target(),
            ok: true,
            detail: format!("{html}{suffix}"),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// One recorded HTTP request: request line + headers + body,
    /// plus the parsed `METHOD path` key and raw bytes.
    struct Seen {
        raw: String,
        key: String,
    }

    /// Read one HTTP/1.1 request (headers + `Content-Length` body)
    /// off a stream. Returns `(head_with_crlf, body)`.
    fn read_http_request(stream: &mut std::net::TcpStream) -> Option<(String, Vec<u8>)> {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        let header_end = loop {
            if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
                break pos + 4;
            }
            let n = stream.read(&mut chunk).ok()?;
            if n == 0 {
                return None;
            }
            buf.extend_from_slice(&chunk[..n]);
        };
        let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
        let content_length = head
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|v| v.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        let mut body = buf[header_end..].to_vec();
        while body.len() < content_length {
            let n = stream.read(&mut chunk).ok()?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&chunk[..n]);
        }
        body.truncate(content_length);
        Some((head, body))
    }

    fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    fn release_json(id: u64, upload_base: &str, assets: serde_json::Value) -> Vec<u8> {
        serde_json::json!({
            "id": id,
            "html_url": "https://github.com/o/r/releases/tag/u-v1.2.3",
            "upload_url": format!("{upload_base}{{?name,label}}"),
            "assets": assets,
        })
        .to_string()
        .into_bytes()
    }

    fn job(dry_run: bool, assets: Vec<PublishAsset>) -> PublishJob {
        PublishJob {
            unit: "u".into(),
            version: "1.2.3".into(),
            manifest_dir: PathBuf::from("crates/u"),
            tag: "u-v1.2.3".into(),
            notes: "notes".into(),
            assets,
            commitish: None,
            dry_run,
        }
    }

    fn publisher(base: &str) -> GhReleasePublisher {
        let mut p = GhReleasePublisher::new("o/r".into(), "TARSI_TEST_GH_TOKEN".into());
        p.api_base = base.to_string();
        p
    }

    fn token_env() {
        // Unique-ish per test process; the value never leaves the
        // test and is only ever sent to the local mock.
        std::env::set_var("TARSI_TEST_GH_TOKEN", "test-token");
    }

    #[test]
    fn creates_release_then_uploads_assets() {
        token_env();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("u-1.2.3.tar.gz");
        std::fs::write(&file, b"fake-tarball-bytes").unwrap();

        // upload_url must point back at the mock — bind the
        // listener first, then build the handler with its base.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let upload_base = format!("{base}/uploads/o/r/releases/42/assets");
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let Some((head, body)) = read_http_request(&mut stream) else {
                    continue;
                };
                let request_line = head.lines().next().unwrap_or("").to_string();
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let target = parts.next().unwrap_or("").to_string();
                let path = target.split('?').next().unwrap_or("").to_string();
                sink.lock().unwrap().push(Seen {
                    raw: format!("{head}\n{}", String::from_utf8_lossy(&body)),
                    key: format!("{method} {path}"),
                });
                let (status, resp_body): (u16, Vec<u8>) = match (method.as_str(), path.as_str()) {
                    ("GET", "/repos/o/r/releases/tags/u-v1.2.3") => (404, b"{}".to_vec()),
                    ("POST", "/repos/o/r/releases") => {
                        (201, release_json(42, &upload_base, serde_json::json!([])))
                    }
                    ("POST", "/uploads/o/r/releases/42/assets") => (201, b"{}".to_vec()),
                    _ => (404, b"{}".to_vec()),
                };
                let reason = if status == 201 {
                    "Created"
                } else {
                    "Not Found"
                };
                let resp = format!(
                    "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    resp_body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.write_all(&resp_body);
                let _ = stream.flush();
            }
        });

        let p = publisher(&base);
        let j = job(
            false,
            vec![PublishAsset {
                path: file,
                name: "u-1.2.3.tar.gz".into(),
                content_type: None,
                size: 17,
            }],
        );
        let report = p.publish(&j).unwrap();
        assert!(report.ok, "{}", report.detail);
        assert!(
            report.detail.contains("github.com/o/r"),
            "{}",
            report.detail
        );

        let recorded = seen.lock().unwrap();
        assert_eq!(
            recorded.len(),
            3,
            "{:?}",
            recorded.iter().map(|s| &s.key).collect::<Vec<_>>()
        );
        assert_eq!(recorded[0].key, "GET /repos/o/r/releases/tags/u-v1.2.3");
        assert_eq!(recorded[1].key, "POST /repos/o/r/releases");
        // Upload went to the release's upload_url with ?name=, the
        // guessed content type, and the raw file bytes as body.
        assert!(
            recorded[2]
                .raw
                .contains("POST /uploads/o/r/releases/42/assets?name=u-1.2.3.tar.gz"),
            "{}",
            recorded[2].raw
        );
        assert!(recorded[2].raw.contains("content-type: application/gzip"));
        assert!(recorded[2].raw.ends_with("fake-tarball-bytes"));
    }

    #[test]
    fn rerun_reuses_release_and_replaces_same_name_asset() {
        token_env();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("artifact.bin");
        std::fs::write(&file, b"v2-bytes").unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let upload_base = format!("{base}/uploads/o/r/releases/7/assets");
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let Some((head, body)) = read_http_request(&mut stream) else {
                    continue;
                };
                let request_line = head.lines().next().unwrap_or("").to_string();
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let target = parts.next().unwrap_or("").to_string();
                let path = target.split('?').next().unwrap_or("").to_string();
                sink.lock().unwrap().push(Seen {
                    raw: format!("{head}\n{}", String::from_utf8_lossy(&body)),
                    key: format!("{method} {path}"),
                });
                let (status, resp_body): (u16, Vec<u8>) = match (method.as_str(), path.as_str()) {
                    ("GET", "/repos/o/r/releases/tags/u-v1.2.3") => (
                        200,
                        release_json(
                            7,
                            &upload_base,
                            serde_json::json!([{"id": 99, "name": "artifact.bin"}]),
                        ),
                    ),
                    ("DELETE", "/repos/o/r/releases/assets/99") => (204, Vec::new()),
                    ("POST", "/uploads/o/r/releases/7/assets") => (201, b"{}".to_vec()),
                    _ => (404, b"{}".to_vec()),
                };
                let reason = match status {
                    200 => "OK",
                    201 => "Created",
                    204 => "No Content",
                    _ => "Not Found",
                };
                let resp = format!(
                    "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    resp_body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.write_all(&resp_body);
                let _ = stream.flush();
            }
        });

        let p = publisher(&base);
        let j = job(
            false,
            vec![PublishAsset {
                path: file,
                name: "artifact.bin".into(),
                content_type: Some("application/x-custom".into()),
                size: 8,
            }],
        );
        let report = p.publish(&j).unwrap();
        assert!(report.ok, "{}", report.detail);
        assert!(
            report.detail.contains("existing release reused"),
            "{}",
            report.detail
        );

        let recorded = seen.lock().unwrap();
        // GET by tag → DELETE the same-name asset → POST upload.
        // No release-create POST at all.
        assert_eq!(
            recorded.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(),
            vec![
                "GET /repos/o/r/releases/tags/u-v1.2.3",
                "DELETE /repos/o/r/releases/assets/99",
                "POST /uploads/o/r/releases/7/assets",
            ]
        );
        assert!(recorded[2].raw.contains("?name=artifact.bin"));
        assert!(recorded[2]
            .raw
            .contains("content-type: application/x-custom"));
        assert!(recorded[2].raw.ends_with("v2-bytes"));
    }

    #[test]
    fn dry_run_reports_assets_without_http() {
        // Listener that counts connections — must stay at zero.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                counter.fetch_add(1, Ordering::SeqCst);
                drop(stream);
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("dist.tar.gz");
        std::fs::write(&file, b"0123456789").unwrap();
        let p = publisher(&format!("http://{addr}"));
        let j = job(
            true,
            vec![PublishAsset {
                path: file,
                name: "dist.tar.gz".into(),
                content_type: None,
                size: 10,
            }],
        );
        let report = p.publish(&j).unwrap();
        assert!(report.ok);
        assert!(report.detail.contains("dry-run"));
        assert!(report.detail.contains("dist.tar.gz"));
        assert!(report.detail.contains("10 bytes"));
        assert!(report.detail.contains("application/gzip"));
        assert_eq!(hits.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn expand_assets_globs_and_substitutes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("dist")).unwrap();
        std::fs::write(root.join("dist/u-1.2.3-linux.tar.gz"), b"a").unwrap();
        std::fs::write(root.join("dist/u-1.2.3-macos.tar.gz"), b"bb").unwrap();
        std::fs::write(root.join("dist/old.tar.gz"), b"c").unwrap();
        std::fs::create_dir_all(root.join("u")).unwrap();
        std::fs::write(root.join("u/checksums.txt"), b"sums").unwrap();

        let assets = expand_assets(
            root,
            &[
                "dist/{unit}-{version}-*.tar.gz".to_string(),
                "u/checksums.txt".to_string(),
            ],
            "u",
            "1.2.3",
        )
        .unwrap();
        let names: Vec<&str> = assets.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "checksums.txt",
                "u-1.2.3-linux.tar.gz",
                "u-1.2.3-macos.tar.gz"
            ]
        );
        let sums = assets.iter().find(|a| a.name == "checksums.txt").unwrap();
        assert!(sums.path.ends_with("u/checksums.txt"));
        assert_eq!(sums.size, 4);

        // Literal missing file → loud error.
        let err = expand_assets(root, &["nope.bin".to_string()], "u", "1.2.3").unwrap_err();
        assert!(err.to_string().contains("nope.bin"));

        // Magic pattern matching nothing → skipped, not an error.
        let ok = expand_assets(
            root,
            &["dist/{unit}-{version}-windows-*".to_string()],
            "u",
            "1.2.3",
        )
        .unwrap();
        assert!(ok.is_empty());
    }

    #[test]
    fn content_type_guessing() {
        assert_eq!(guess_content_type("a.tar.gz"), "application/gzip");
        assert_eq!(guess_content_type("a.tgz"), "application/gzip");
        assert_eq!(guess_content_type("a.zip"), "application/zip");
        assert_eq!(guess_content_type("a.json"), "application/json");
        assert_eq!(guess_content_type("a.bin"), "application/octet-stream");
    }

    /// Resolver seam — returns a fixed token regardless of env.
    struct FakeResolver(Option<String>);

    impl TokenResolver for FakeResolver {
        fn resolve(
            &self,
            _token_env: &str,
            _token_ref: Option<&str>,
        ) -> Result<Option<String>, PublishError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn token_comes_from_resolver_when_env_is_unset() {
        // Point token_env at a var that is certainly unset — the
        // resolver supplies the token (token_ref path) and env
        // must not be consulted. (A unique name avoids racing the
        // env-mutating tests above in the same process.)
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let base = format!("http://{addr}");
        let upload_base = format!("{base}/uploads/o/r/releases/42/assets");
        let seen: Arc<Mutex<Vec<Seen>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let Some((head, body)) = read_http_request(&mut stream) else {
                    continue;
                };
                let request_line = head.lines().next().unwrap_or("").to_string();
                let mut parts = request_line.split_whitespace();
                let method = parts.next().unwrap_or("").to_string();
                let target = parts.next().unwrap_or("").to_string();
                let path = target.split('?').next().unwrap_or("").to_string();
                sink.lock().unwrap().push(Seen {
                    raw: format!("{head}\n{}", String::from_utf8_lossy(&body)),
                    key: format!("{method} {path}"),
                });
                let (status, resp_body): (u16, Vec<u8>) = match (method.as_str(), path.as_str()) {
                    ("GET", "/repos/o/r/releases/tags/u-v1.2.3") => (404, b"{}".to_vec()),
                    ("POST", "/repos/o/r/releases") => {
                        (201, release_json(42, &upload_base, serde_json::json!([])))
                    }
                    _ => (404, b"{}".to_vec()),
                };
                let reason = if status == 201 {
                    "Created"
                } else {
                    "Not Found"
                };
                let resp = format!(
                    "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    resp_body.len()
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.write_all(&resp_body);
                let _ = stream.flush();
            }
        });

        let mut p = publisher(&base);
        p.token_env = "TARSI_TEST_UNSET_RESOLVER_TOKEN".into();
        p.token_ref = Some("cuora://o/r/token".into());
        p.resolver = Arc::new(FakeResolver(Some("ref-token".into())));
        let report = p.publish(&job(false, vec![])).unwrap();
        assert!(report.ok, "{}", report.detail);

        let recorded = seen.lock().unwrap();
        assert_eq!(recorded.len(), 2);
        for s in recorded.iter() {
            assert!(
                s.raw.to_lowercase().contains("bearer ref-token"),
                "missing auth header: {}",
                s.raw
            );
        }
    }
}
