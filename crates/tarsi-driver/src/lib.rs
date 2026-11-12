//! tarsi-driver — library half of `tamias-driver-tarsi`, the tamias
//! content driver for `type: tarsi` release-profile packs (PL-014 D3).
//!
//! A `type: tarsi` pack is a **release profile**: a payload whose
//! root carries `tarsi.{yaml,yml,toml}` (a plain
//! [`tarsi_core::Config`] document — no `[profile]` table; `profile`
//! is the *selector* key) plus optional extras (`styleguide.md`,
//! commit maps, …). See `tarsi-config`'s `profile` module for the
//! discovery contract this driver installs into.
//!
//! # Where install places the profile
//!
//! tamias dispatches `install` *before* its store commit, with
//! `--dir` = the payload root; `uninstall`/`deploy`/`status` get the
//! committed store entry dir (`<store>/[@scope/]<name>@<label>/`).
//! tarsi's profile discovery already treats the tamias store as a
//! first-class source — `store_profiles` finds every
//! `<store>/name@label/` carrying a `tarsi.*` and `find_dir` picks
//! the newest version label. **The store commit is therefore the
//! placement**: `install` validates the payload and reports; the
//! pack becomes resolvable the moment tamias commits it, versioned
//! by label.
//!
//! The driver deliberately does *not* copy payloads into
//! `~/.config/tarsi/profiles/<name>/` by default: dir profiles are
//! searched before the store and win `find_dir`, and they carry no
//! version — an unversioned copy would shadow every store version,
//! including ones `tamias remove` already dropped (the host only
//! dispatches `uninstall` on the *last* same-name sibling, T-029).
//!
//! Opt-in escape hatch: `--config` may name a `profilesDir`. Then
//! `install`/`deploy` also materialize an **unversioned dir-profile
//! copy** at `<profilesDir>/<name>/` (scoped names nest
//! `@scope/<name>/`) stamped with a `.tamias-driver-tarsi.json`
//! marker, and `uninstall` removes marker-bearing copies it finds
//! under the profiles roots. Use it to pin a profile outside the
//! evictable store — knowing the copy shadows store resolution.
//!
//! # Verbs (the tamias driver contract)
//!
//! `<verb> --dir <d> --name <n> [--config <f>]`:
//!
//! - `capabilities` — `{"capabilities": ["tamias"], "types":
//!   ["tarsi"], "verbs": […]}`; tamias's discovery probe.
//! - `install` — validate the payload is a well-formed profile
//!   (`tarsi.*` present and parses as `Config`; `tamias.yaml`'s
//!   `type` must be `tarsi` when present), then optionally write
//!   the `profilesDir` copy. JSON report; nothing else is applied.
//! - `deploy` — refresh the operator-facing state: re-validate the
//!   installed entry and re-sync the `profilesDir` copy when
//!   configured; reports how `tarsi` resolves the name.
//! - `uninstall` — remove marker-bearing `profilesDir` copies under
//!   the profiles roots (`$TARSI_PROFILES_DIR`,
//!   `~/.config/tarsi/profiles`, plus the configured dir). The store
//!   entry itself is dropped by tamias after this verb returns.
//! - `status` — resolution report: which dir `find_dir` picks for
//!   the name, from which source, and every installed candidate.
//! - `health` — discovery probe: profiles roots + visible profile
//!   count. No services exist for a profile pack.
//! - `logs` / `hooks` — empty reports; `type: tarsi` packs run no
//!   units and declare no forge hooks.
//! - `config-schema` — JSON Schema (draft 2020-12) of the optional
//!   `--config` file (see [`DriverConfig`]).
//!
//! # The pavo read-only surface
//!
//! The same binary is also a **pavo view driver** (PL-014 D3
//! remainder): `manifest` prints the [`pavo_protocol::DriverManifest`]
//! and `serve` answers ndjson JSON-RPC (`manifest`/`views`/`view`/
//! `query`) over [`views`] — release plans, changelog previews, and
//! publish-profile state, all read through `tarsi-serve` one-shot
//! ops ([`data`]). The surfaces are disjoint by argv: pavo's probes
//! (`manifest`/`capabilities`/`serve`) take no `--dir`/`--name`,
//! tamias's verbs always carry them — `capabilities` answers a
//! merged payload satisfying both discoverers. Nothing on the pavo
//! side mutates: the serve ops called are `manifest`, `profiles`,
//! `plan`, `changelog` — `release` is never dispatched.
//!
//! See `views.rs` for the view/ query mapping and `data.rs` for
//! the `tarsi-serve` invocation (`$TARSI_SERVE` overrides the
//! binary, default `tarsi-serve` on PATH).

pub mod data;
pub mod views;

use std::fs;
use std::path::{Component, Path, PathBuf};

use scarab_config::ConfigFormat;
use scarab_config_yaml::YamlFormat;
use serde_json::{json, Value};
use tarsi_config::profile::{self, ProfileInfo};

/// Env lookup — injectable so tests never touch the process env.
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The name errors/reports are prefixed with — matches the binary.
const WHO: &str = "tamias-driver-tarsi";

/// Marker file written into driver-managed `profilesDir` copies —
/// `uninstall` only removes dirs carrying it (a hand-made profile
/// dir is never touched).
const MARKER: &str = ".tamias-driver-tarsi.json";

/// Config-file extensions a profile payload may carry, in the same
/// preference order `tarsi-config`'s profile discovery uses.
const CONFIG_EXTS: [&str; 3] = ["yaml", "yml", "toml"];

/// One dispatched verb invocation — the flags tamias always passes.
#[derive(Debug, Clone)]
pub struct Invocation {
    /// The payload dir on `install`; the committed store entry dir
    /// (`<store>/[@scope/]<name>@<label>/`) on the other verbs.
    pub dir: PathBuf,
    /// The installed package name (bare — scope is not passed).
    pub name: String,
    /// Optional `--config` file (see [`DriverConfig`]).
    pub config: Option<PathBuf>,
}

/// The optional `--config` document (YAML; JSON parses too):
///
/// ```yaml
/// profilesDir: ~/.config/tarsi/profiles   # opt-in unversioned copy
/// ```
///
/// `profilesDir` roots the dir-profile copy `<profilesDir>/<name>/`
/// `install`/`deploy` maintain and `uninstall` removes. Relative
/// paths resolve against the config file's directory; `~` expands
/// via `$HOME`. Absent config or absent key = store-only install
/// (the default — see the module docs for why).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DriverConfig {
    /// Root for the opt-in dir-profile copy.
    pub profiles_dir: Option<PathBuf>,
}

/// `capabilities` — merged for both consumers of this binary.
/// pavo's discovery checks `capabilities`/`verbs` for
/// `"pavo"`/`"serve"`; tamias's checks `types` for the content kind
/// and warns on undeclared `verbs`. One JSON answers both: the
/// kit's `{capabilities, verbs}` shape plus `"types": ["tarsi"]`,
/// `"tamias"` in `capabilities`, and every verb folded into
/// `verbs`.
pub fn capabilities_value() -> Value {
    let m = views::manifest();
    let mut v = pavo_driver_kit::capabilities_value(&m);
    v["views"] = json!(m.views.len());
    v["types"] = json!(["tarsi"]);
    v["capabilities"] = json!(["pavo", "tamias"]);
    let mut verbs: Vec<Value> = v["verbs"].as_array().cloned().unwrap_or_default();
    for extra in [
        "capabilities",
        "config-schema",
        "install",
        "deploy",
        "uninstall",
        "status",
        "health",
        "logs",
        "hooks",
    ] {
        if !verbs.iter().any(|x| x.as_str() == Some(extra)) {
            verbs.push(json!(extra));
        }
    }
    v["verbs"] = Value::Array(verbs);
    v
}

/// `config-schema` — the reserved verb printing the JSON Schema of
/// the `--config` file. Instance-agnostic (draft 2020-12).
pub fn config_schema() -> Value {
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://9aia.com/tarsi/tamias-driver-config.schema.json",
        "title": "tamias-driver-tarsi --config",
        "description": "Operator config for the tarsi tamias content driver. YAML (JSON parses too). All fields optional; an absent file means store-only installs.",
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "profilesDir": {
                "type": "string",
                "description": "Opt-in root for an unversioned dir-profile copy: install/deploy materialize <profilesDir>/<name>/ (scoped names nest @scope/<name>/) stamped with a .tamias-driver-tarsi.json marker; uninstall removes marker-bearing copies under the profiles roots. Dir profiles shadow tamias-store resolution (they are searched first and carry no version) — use to pin a profile outside the evictable store. Relative paths resolve against the config file's directory; '~' expands via $HOME."
            }
        }
    })
}

/// Load `--config`: `None` → defaults; a path → YAML (JSON is valid
/// YAML) with `profilesDir` rebased onto the config file's dir.
///
/// # Errors
/// Unreadable file, non-mapping document, unknown keys, or a
/// non-string `profilesDir`.
pub fn load_config(path: Option<&Path>, env: Env<'_>) -> Result<DriverConfig, String> {
    let Some(path) = path else {
        return Ok(DriverConfig::default());
    };
    let text = fs::read_to_string(path)
        .map_err(|e| format!("{WHO}: cannot read config {}: {e}", path.display()))?;
    let value: Value = YamlFormat
        .parse(&text)
        .map_err(|e| format!("{WHO}: {}: invalid driver config: {e}", path.display()))?;
    let Some(map) = value.as_object() else {
        return Err(format!(
            "{WHO}: {}: driver config must be a mapping",
            path.display()
        ));
    };
    for key in map.keys() {
        if key != "profilesDir" {
            return Err(format!(
                "{WHO}: {}: unknown config key '{key}'",
                path.display()
            ));
        }
    }
    let Some(raw) = map.get("profilesDir") else {
        return Ok(DriverConfig::default());
    };
    let Some(dir) = raw.as_str() else {
        return Err(format!(
            "{WHO}: {}: profilesDir must be a string",
            path.display()
        ));
    };
    // A readable config path always has a parent — "." is the
    // bare-filename fallback.
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    // `~` expands before relativizing — `~/profs` is absolute, a
    // bare `profs` rebases onto the config file's directory.
    let expanded = expand_tilde(Path::new(dir), env);
    let resolved = if expanded.is_absolute() {
        expanded
    } else {
        base.join(expanded)
    };
    Ok(DriverConfig {
        profiles_dir: Some(expand_tilde(&resolved, env)),
    })
}

/// `~`/`~/…` → `$HOME`-joined; anything else unchanged. Only a
/// leading `~` component expands (same rule scarab's `expand_tilde`
/// applies for the single-user case).
fn expand_tilde(path: &Path, env: Env<'_>) -> PathBuf {
    let mut comps = path.components();
    if comps.next() != Some(Component::Normal(std::ffi::OsStr::new("~"))) {
        return path.to_path_buf();
    }
    match env("HOME").or_else(|| env("USERPROFILE")) {
        Some(home) if !home.is_empty() => Path::new(&home).join(comps.as_path()),
        _ => path.to_path_buf(),
    }
}

/// The `tarsi.*` config file inside `dir`, if any — same preference
/// order `tarsi-config`'s profile discovery uses (yaml → yml → toml).
fn config_file_in(dir: &Path) -> Option<PathBuf> {
    CONFIG_EXTS
        .iter()
        .map(|e| dir.join(format!("tarsi.{e}")))
        .find(|p| p.is_file())
}

/// Parse a YAML/JSON-ish doc into a `serde_json::Value`.
fn parse_yaml(text: &str) -> Result<Value, String> {
    YamlFormat.parse(text)
}

/// `tamias.yaml|yml` inside `dir` → parsed Value, best-effort.
fn manifest_in(dir: &Path) -> Option<Value> {
    for name in ["tamias.yaml", "tamias.yml"] {
        let p = dir.join(name);
        if p.is_file() {
            if let Ok(text) = fs::read_to_string(&p) {
                if let Ok(v) = parse_yaml(&text) {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// Validate `dir` is a well-formed `type: tarsi` profile payload:
/// a `tarsi.*` file that parses as [`tarsi_core::Config`] through
/// the real loader (this is what rejects a `[profile]` *table* —
/// `Config.profile` is the string selector), plus a `tamias.yaml`
/// whose `type` is `tarsi` when the manifest parses.
///
/// Returns `(config_file, manifest)` for the report.
fn validate_profile(dir: &Path) -> Result<(PathBuf, Option<Value>), String> {
    let cfg_file = config_file_in(dir).ok_or_else(|| {
        format!(
            "{WHO}: {} carries no tarsi.{{yaml,yml,toml}} — not a type:tarsi profile",
            dir.display()
        )
    })?;
    tarsi_config::load_path(&cfg_file)
        .map_err(|e| format!("{WHO}: invalid profile config {}: {e}", cfg_file.display()))?;
    let manifest = manifest_in(dir);
    if let Some(m) = &manifest {
        match m.get("type").and_then(Value::as_str) {
            Some("tarsi") | None => {}
            Some(other) => {
                return Err(format!(
                    "{WHO}: {} declares type '{other}' — this driver serves type: tarsi",
                    dir.display()
                ));
            }
        }
    }
    Ok((cfg_file, manifest))
}

/// The dir-profile path for `name` under `root`: `name` →
/// `<root>/<name>`; `@scope/name` → `<root>/@scope/<name>`.
/// Rejects anything that isn't a safe relative profile name.
fn profile_dir_for(root: &Path, name: &str) -> Result<PathBuf, String> {
    if !safe_profile_name(name) {
        return Err(format!("{WHO}: unsafe profile name {name:?}"));
    }
    let mut out = root.to_path_buf();
    for seg in name.split('/') {
        out.push(seg);
    }
    Ok(out)
}

/// Profile-name safety — mirrors `tarsi-config`'s `safe_name`:
/// `name` or `@scope/name`, no traversal, no absolute paths, no
/// NUL. The scope segment must be exactly `@<name>`.
fn safe_profile_name(name: &str) -> bool {
    fn seg_ok(s: &str) -> bool {
        !s.is_empty() && s != "." && s != ".." && !s.contains('\0') && !s.starts_with('@')
    }
    if name.is_empty()
        || name.len() > 128
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.contains('\0')
    {
        return false;
    }
    let segs: Vec<&str> = name.split('/').collect();
    match segs.as_slice() {
        [single] => seg_ok(single),
        [scope, single] => {
            scope
                .strip_prefix('@')
                .is_some_and(|s| !s.is_empty() && s != "." && s != ".." && !s.contains('\0'))
                && seg_ok(single)
        }
        _ => false,
    }
}

/// The profiles roots `uninstall` scans for managed copies, in
/// `tarsi-config`'s dir-profile precedence order:
/// `$TARSI_PROFILES_DIR` then `~/.config/tarsi/profiles`.
fn profiles_roots(env: Env<'_>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(d) = env("TARSI_PROFILES_DIR").filter(|s| !s.is_empty()) {
        roots.push(PathBuf::from(d));
    }
    if let Some(home) = env("HOME")
        .filter(|s| !s.is_empty())
        .or_else(|| env("USERPROFILE").filter(|s| !s.is_empty()))
    {
        roots.push(Path::new(&home).join(".config/tarsi/profiles"));
    }
    roots.dedup();
    roots
}

/// Does `entry` name satisfy requested `name`? Mirrors
/// `tarsi-config`'s `name_matches`: exact, or bare-name match
/// against a scoped entry (`@scope/name` satisfies `name`, and
/// `scope/name`/`@scope/name` requests match the scoped entry).
fn name_matches(req: &str, entry: &str) -> bool {
    if req == entry {
        return true;
    }
    let bare_req = req.strip_prefix('@').unwrap_or(req);
    let (esc, epkg) = match entry.strip_prefix('@') {
        Some(rest) => match rest.split_once('/') {
            Some((s, p)) => (Some(s), p),
            None => (None, rest),
        },
        None => (None, entry),
    };
    match bare_req.split_once('/') {
        Some((s, p)) => esc == Some(s) && p == epkg,
        None => epkg == bare_req,
    }
}

/// The bare package name for marker matching — `@scope/name` and
/// `scope/name` both reduce to `name` (the driver only ever sees
/// the store record's bare name anyway).
fn bare_name(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// Read the marker file in `dir`, if present and parseable.
fn read_marker(dir: &Path) -> Option<Value> {
    let text = fs::read_to_string(dir.join(MARKER)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Every marker-bearing profile dir under `root` whose marker name
/// bare-matches `name` — one level, `@scope/` nesting included
/// (same walk `tarsi-config`'s `dir_profiles` does).
fn managed_copies_under(root: &Path, name: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(children) = fs::read_dir(root) else {
        return out;
    };
    for child in children.flatten() {
        let path = child.path();
        if !path.is_dir() {
            continue;
        }
        let fname = child.file_name().to_string_lossy().to_string();
        if fname.starts_with('@') {
            if let Ok(grand) = fs::read_dir(&path) {
                for g in grand.flatten() {
                    let gpath = g.path();
                    if gpath.is_dir() && marker_matches(&gpath, name) {
                        out.push(gpath);
                    }
                }
            }
        } else if marker_matches(&path, name) {
            out.push(path);
        }
    }
    out
}

/// `dir` carries our marker AND the marker's `name` bare-matches
/// `name`. The marker is the ownership proof — a profile dir
/// without it is never driver-managed.
fn marker_matches(dir: &Path, name: &str) -> bool {
    read_marker(dir)
        .and_then(|m| m.get("name").and_then(Value::as_str).map(str::to_string))
        .is_some_and(|n| bare_name(&n) == bare_name(name))
}

/// All managed copies of `name` across `roots` (deduped, sorted).
fn managed_copies(roots: &[PathBuf], name: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = roots
        .iter()
        .flat_map(|r| managed_copies_under(r, name))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Recursively copy `src` → `dst`, skipping the payload's
/// top-level `drivers/` dir (bundled-driver bits aren't profile
/// content) and any marker file (a copy of a copy stays clean —
/// the marker is rewritten by [`sync_profiles_dir`]).
fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    copy_inner(src, dst, true)
}

fn copy_inner(src: &Path, dst: &Path, top: bool) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("{WHO}: cannot create {}: {e}", dst.display()))?;
    for entry in
        fs::read_dir(src).map_err(|e| format!("{WHO}: cannot read {}: {e}", src.display()))?
    {
        let entry = entry.map_err(|e| format!("{WHO}: read entry: {e}"))?;
        let from = entry.path();
        let name = entry.file_name();
        let ft = entry
            .file_type()
            .map_err(|e| format!("{WHO}: stat {}: {e}", from.display()))?;
        if top && ft.is_dir() && name == "drivers" {
            continue;
        }
        if name == MARKER {
            continue;
        }
        let to = dst.join(&name);
        if ft.is_dir() {
            copy_inner(&from, &to, false)?;
        } else if ft.is_file() {
            fs::copy(&from, &to).map_err(|e| {
                format!(
                    "{WHO}: cannot copy {} → {}: {e}",
                    from.display(),
                    to.display()
                )
            })?;
        }
        // symlinks/other: skipped — profile content is plain files.
    }
    Ok(())
}

/// (Re)materialize the `profilesDir` copy of `src` at
/// `<profilesDir>/<name>/`. An existing dest is replaced only when
/// it is driver-managed (marker present) or empty of a `tarsi.*` —
/// a hand-made profile dir is never clobbered.
fn sync_profiles_dir(
    src: &Path,
    profiles_dir: &Path,
    name: &str,
    version: Option<&str>,
) -> Result<PathBuf, String> {
    let dest = profile_dir_for(profiles_dir, name)?;
    if dest.exists() {
        let managed = read_marker(&dest).is_some();
        let has_profile = config_file_in(&dest).is_some();
        if !managed && has_profile {
            return Err(format!(
                "{WHO}: {} exists and is not driver-managed — refusing to overwrite a hand-made profile",
                dest.display()
            ));
        }
        fs::remove_dir_all(&dest)
            .map_err(|e| format!("{WHO}: cannot replace {}: {e}", dest.display()))?;
    }
    copy_tree(src, &dest)?;
    let marker = json!({
        "driver": "tamias-driver-tarsi",
        "name": name,
        "version": version,
        "source": src,
    });
    let text = serde_json::to_string_pretty(&marker)
        .map_err(|e| format!("{WHO}: cannot serialize marker: {e}"))?;
    fs::write(dest.join(MARKER), format!("{text}\n"))
        .map_err(|e| format!("{WHO}: cannot write marker in {}: {e}", dest.display()))?;
    Ok(dest)
}

/// How `name` resolves right now — the `find_dir` pick plus every
/// visible candidate, classified by source root.
fn resolve_report(name: &str, env: Env<'_>) -> Value {
    let resolved = profile::find_dir(name, env);
    let roots = profiles_roots(env);
    let source = resolved.as_ref().map(|p| {
        if roots.iter().any(|r| p.starts_with(r)) {
            "profiles-dir"
        } else {
            "tamias-store"
        }
    });
    let candidates: Vec<Value> = profile::list(env)
        .iter()
        .filter(|p| name_matches(name, &p.name))
        .map(candidate_json)
        .collect();
    json!({
        "name": name,
        "resolved": resolved,
        "source": source,
        "candidates": candidates,
    })
}

fn candidate_json(p: &ProfileInfo) -> Value {
    json!({
        "name": p.name,
        "version": p.version,
        "description": p.description,
        "source": p.source,
    })
}

/// `install` — validate the payload is a well-formed profile, then
/// optionally write the `profilesDir` copy. The store commit that
/// follows this verb is what makes the pack resolvable (and
/// versioned); see the module docs.
pub fn install(inv: &Invocation, env: Env<'_>) -> Result<Value, String> {
    let cfg = load_config(inv.config.as_deref(), env)?;
    let (cfg_file, manifest) = validate_profile(&inv.dir)?;
    let version = manifest
        .as_ref()
        .and_then(|m| m.get("version"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut report = json!({
        "ok": true,
        "package": inv.name,
        "payload": inv.dir,
        "profile": cfg_file.file_name().map(|n| n.to_string_lossy().to_string()),
        "manifestVersion": version,
        "placement": "tamias-store",
        "note": "the tamias store commit makes this profile resolvable and versioned; no files are copied by default",
    });
    if let Some(pd) = &cfg.profiles_dir {
        let dest = sync_profiles_dir(&inv.dir, pd, &inv.name, version.as_deref())?;
        report["profilesDir"] = json!(dest);
        report["placement"] = json!("profiles-dir+store");
    }
    Ok(report)
}

/// `deploy` — refresh operator-facing state for the installed
/// entry: re-validate it is still a profile payload and re-sync
/// the `profilesDir` copy when configured, then report how tarsi
/// resolves the name. Idempotent; never fails on resolution alone.
pub fn deploy(inv: &Invocation, env: Env<'_>) -> Result<Value, String> {
    let cfg = load_config(inv.config.as_deref(), env)?;
    let (cfg_file, _) = validate_profile(&inv.dir)?;
    let mut report = json!({
        "ok": true,
        "package": inv.name,
        "payload": inv.dir,
        "profile": cfg_file.file_name().map(|n| n.to_string_lossy().to_string()),
        "resolution": resolve_report(&inv.name, env),
    });
    if let Some(pd) = &cfg.profiles_dir {
        let version = manifest_in(&inv.dir)
            .and_then(|m| m.get("version").and_then(Value::as_str).map(str::to_string));
        let dest = sync_profiles_dir(&inv.dir, pd, &inv.name, version.as_deref())?;
        report["profilesDir"] = json!(dest);
    }
    Ok(report)
}

/// `uninstall` — remove driver-managed `profilesDir` copies under
/// every profiles root (plus the configured one). Only dirs
/// carrying our marker are touched; the store entry itself is
/// dropped by tamias after this verb returns.
pub fn uninstall(inv: &Invocation, env: Env<'_>) -> Result<Value, String> {
    let cfg = load_config(inv.config.as_deref(), env)?;
    let mut roots = profiles_roots(env);
    if let Some(pd) = &cfg.profiles_dir {
        roots.push(pd.clone());
    }
    roots.sort();
    roots.dedup();
    let removed = managed_copies(&roots, &inv.name);
    for dir in &removed {
        fs::remove_dir_all(dir)
            .map_err(|e| format!("{WHO}: cannot remove {}: {e}", dir.display()))?;
    }
    Ok(json!({
        "ok": true,
        "package": inv.name,
        "removedProfilesDir": removed,
        "note": "the tamias store entry is dropped by tamias after this verb; unmanaged profile dirs are never touched",
    }))
}

/// `status` — resolution report for the installed name: which dir
/// `find_dir` picks, from which source, and every candidate.
pub fn status(inv: &Invocation, env: Env<'_>) -> Result<Value, String> {
    let mut report = resolve_report(&inv.name, env);
    report["ok"] = json!(true);
    report["package"] = json!(inv.name);
    report["payload"] = json!(inv.dir);
    report["profile"] = json!(config_file_in(&inv.dir).map(|p| p.display().to_string()));
    Ok(report)
}

/// `health` — discovery probe: the profiles roots and how many
/// profiles tarsi can see. Profile packs run no services, so this
/// is the whole health surface.
pub fn health(_inv: &Invocation, env: Env<'_>) -> Result<Value, String> {
    let profiles = profile::list(env);
    Ok(json!({
        "ok": true,
        "profilesRoots": profiles_roots(env),
        "profiles": profiles.len(),
        "storeDiscovery": "tamias-store entries carrying tarsi.* resolve by name@label",
    }))
}

/// `logs` — profile packs run no units; empty report keeps the
/// verb contract total.
pub fn logs(inv: &Invocation) -> Value {
    json!({
        "ok": true,
        "package": inv.name,
        "units": [],
        "note": "type: tarsi packs are release profiles — no services, nothing to journal",
    })
}

/// `hooks` — profiles declare no forge hooks; empty report.
pub fn hooks(inv: &Invocation) -> Value {
    json!({
        "ok": true,
        "package": inv.name,
        "hooks": [],
        "note": "release profiles declare no forge hooks",
    })
}

/// Dispatch one tamias verb → its JSON report. `verb` is the
/// subcommand name; unknown verbs are an error (the clap front in
/// `main.rs` normally keeps them out).
pub fn run_verb(verb: &str, inv: &Invocation, env: Env<'_>) -> Result<Value, String> {
    match verb {
        "install" => install(inv, env),
        "deploy" => deploy(inv, env),
        "uninstall" => uninstall(inv, env),
        "status" => status(inv, env),
        "health" => health(inv, env),
        "logs" => Ok(logs(inv)),
        "hooks" => Ok(hooks(inv)),
        other => Err(format!("{WHO}: unknown verb '{other}'")),
    }
}

/// Print a JSON report to stdout.
pub fn emit(value: &Value) {
    match serde_json::to_string_pretty(value) {
        Ok(text) => println!("{text}"),
        Err(_) => println!("{value}"),
    }
}

// ---------------------------------------------------------------- pavo surface

/// `manifest` verb payload — the pavo [`pavo_protocol::DriverManifest`]
/// as a JSON value.
pub fn manifest_value() -> Value {
    serde_json::to_value(views::manifest()).unwrap_or(Value::Null)
}

/// Dispatch one parsed pavo wire [`pavo_protocol::wire::Request`] to
/// its response. `env` feeds the `tarsi-serve` lookup
/// (`$TARSI_SERVE` override) — injectable so tests never touch the
/// process env.
///
/// Read-only by construction: the only serve ops reachable are
/// `manifest`/`profiles`/`plan`/`changelog`; `release` is never
/// dispatched. `render`/`submit`/`jobs`/`notifications` answer
/// `ok:false` — the manifest claims none of them.
pub fn dispatch(req: &pavo_protocol::wire::Request, env: Env<'_>) -> pavo_protocol::wire::Response {
    use pavo_protocol::wire::{QueryResult, Request, Response, ViewSummary};
    match req {
        Request::Manifest => Response::ok_typed(&views::manifest()),
        Request::Views => {
            let summaries: Vec<ViewSummary> =
                views::specs().iter().map(ViewSummary::from).collect();
            Response::ok_typed(&summaries)
        }
        Request::View { view, params } => match views::view_result(view, params, env) {
            Ok(result) => Response::ok_typed(&result),
            Err(e) => Response::err(e),
        },
        Request::Query { query, params } => {
            if !views::known_query(query) {
                return Response::err(format!("unknown query: {query}"));
            }
            let payload = views::query_data(query, params, env);
            Response::ok_typed(&QueryResult {
                query: query.clone(),
                data: payload,
            })
        }
        Request::Render { kind, .. } => Response::err(format!(
            "tamias-driver-tarsi claims no renderers (kind '{kind}')"
        )),
        Request::Submit { .. } => {
            Response::err("tamias-driver-tarsi is read-only and implements no forms")
        }
        Request::Jobs => Response::err("tamias-driver-tarsi has no job feed"),
        Request::Notifications => Response::err("tamias-driver-tarsi has no notification feed"),
    }
}

/// The `serve` verb — ndjson JSON-RPC on stdio until EOF or a
/// broken pipe, via pavo-driver-kit's loop (blank lines skip,
/// parse failures answer `ok:false`, a dead client exits quietly).
pub fn serve(env: Env<'_>) -> i32 {
    let _ = pavo_driver_kit::serve_stdio(|req| dispatch(req, env));
    0
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn env_of(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: BTreeMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    /// A minimal valid `type: tarsi` payload: tamias.yaml +
    /// tarsi.toml (+ optional extra file).
    fn write_payload(dir: &Path, name: &str, version: &str, styleguide: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("tamias.yaml"),
            format!(
                "name: {name}\nversion: \"{version}\"\ntype: tarsi\ndescription: test profile\n"
            ),
        )
        .unwrap();
        fs::write(
            dir.join("tarsi.toml"),
            format!("[release]\nstyleguide = \"{styleguide}\"\n"),
        )
        .unwrap();
        fs::write(dir.join("styleguide.md"), "# style\n").unwrap();
    }

    fn inv(dir: &Path, name: &str) -> Invocation {
        Invocation {
            dir: dir.to_path_buf(),
            name: name.to_string(),
            config: None,
        }
    }

    #[test]
    fn install_validates_and_reports_store_placement() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, "9aia-release", "0.1.0", "keep-a-changelog");
        let home = tmp.path().join("home");
        let env = env_of(&[("HOME", home.to_str().unwrap())]);
        let report = install(&inv(&payload, "9aia-release"), &env).unwrap();
        assert_eq!(report["ok"], true);
        assert_eq!(report["placement"], "tamias-store");
        assert_eq!(report["profile"], "tarsi.toml");
        assert_eq!(report["manifestVersion"], "0.1.0");
        // Default install copies NOTHING — the store commit places it.
        assert!(!home.join(".config/tarsi/profiles").exists());
    }

    #[test]
    fn install_rejects_non_profile_payloads() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let env = env_of(&[("HOME", home.to_str().unwrap())]);
        // No tarsi.* at all.
        let bare = tmp.path().join("bare");
        fs::create_dir_all(&bare).unwrap();
        fs::write(bare.join("tamias.yaml"), "name: x\ntype: tarsi\n").unwrap();
        assert!(install(&inv(&bare, "x"), &env).is_err());
        // Wrong manifest type.
        let wrong = tmp.path().join("wrong");
        fs::create_dir_all(&wrong).unwrap();
        fs::write(wrong.join("tamias.yaml"), "name: x\ntype: files\n").unwrap();
        fs::write(wrong.join("tarsi.toml"), "[release]\n").unwrap();
        let err = install(&inv(&wrong, "x"), &env).unwrap_err();
        assert!(err.contains("type 'files'"), "{err}");
        // `[profile]` as a table — Config.profile is a string selector.
        let bad = tmp.path().join("bad");
        fs::create_dir_all(&bad).unwrap();
        fs::write(
            bad.join("tamias.yaml"),
            "name: x\ntype: tarsi\nversion: \"1\"\n",
        )
        .unwrap();
        fs::write(bad.join("tarsi.toml"), "[profile]\nname = \"p\"\n").unwrap();
        assert!(install(&inv(&bad, "x"), &env).is_err());
        // Broken TOML.
        fs::write(bad.join("tarsi.toml"), "[release\n").unwrap();
        assert!(install(&inv(&bad, "x"), &env).is_err());
    }

    #[test]
    fn profiles_dir_config_copies_and_resolves() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, "9aia-release", "0.2.0", "keep-a-changelog");
        let profs = tmp.path().join("profs");
        let cfg = tmp.path().join("driver.yaml");
        fs::write(&cfg, format!("profilesDir: {}\n", profs.display())).unwrap();
        let home = tmp.path().join("home");
        let env = env_of(&[
            ("HOME", home.to_str().unwrap()),
            ("TARSI_PROFILES_DIR", profs.to_str().unwrap()),
        ]);
        let mut i = inv(&payload, "9aia-release");
        i.config = Some(cfg.clone());
        let report = install(&i, &env).unwrap();
        let dest = profs.join("9aia-release");
        assert_eq!(report["profilesDir"], json!(dest));
        assert!(dest.join("tarsi.toml").is_file());
        assert!(dest.join("styleguide.md").is_file());
        assert!(dest.join("tamias.yaml").is_file());
        // drivers/ payload bits never copy into a profile dir.
        fs::create_dir_all(payload.join("drivers")).unwrap();
        fs::write(payload.join("drivers/tamias-driver-x"), "bin").unwrap();
        install(&i, &env).unwrap();
        assert!(!dest.join("drivers").exists());
        // Marker present; discovery resolves the dir profile.
        assert!(dest.join(MARKER).is_file());
        let found = profile::find_dir("9aia-release", &env).unwrap();
        assert_eq!(found, dest);
        let infos = profile::list(&env);
        let p = infos.iter().find(|p| p.name == "9aia-release").unwrap();
        assert_eq!(p.version, None);
        assert_eq!(p.description.as_deref(), Some("test profile"));
    }

    #[test]
    fn install_refuses_to_clobber_unmanaged_profile_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, "p", "1.0.0", "keep-a-changelog");
        let profs = tmp.path().join("profs");
        // A hand-made profile dir (no marker) occupying the name.
        let hand = profs.join("p");
        fs::create_dir_all(&hand).unwrap();
        fs::write(hand.join("tarsi.toml"), "[release]\n").unwrap();
        let cfg = tmp.path().join("driver.yaml");
        fs::write(&cfg, format!("profilesDir: {}\n", profs.display())).unwrap();
        let home = tmp.path().join("home");
        let env = env_of(&[("HOME", home.to_str().unwrap())]);
        let mut i = inv(&payload, "p");
        i.config = Some(cfg);
        let err = install(&i, &env).unwrap_err();
        assert!(err.contains("not driver-managed"), "{err}");
        // Hand-made dir untouched.
        assert!(hand.join("tarsi.toml").is_file());
        assert!(!hand.join(MARKER).exists());
    }

    #[test]
    fn uninstall_removes_only_marked_copies() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, "p", "1.0.0", "keep-a-changelog");
        let profs = tmp.path().join("profs");
        let cfg = tmp.path().join("driver.yaml");
        fs::write(&cfg, format!("profilesDir: {}\n", profs.display())).unwrap();
        let home = tmp.path().join("home");
        let env = env_of(&[
            ("HOME", home.to_str().unwrap()),
            ("TARSI_PROFILES_DIR", profs.to_str().unwrap()),
        ]);
        let mut i = inv(&payload, "p");
        i.config = Some(cfg.clone());
        install(&i, &env).unwrap();
        // An unmanaged same-name profile under the OTHER root.
        let user_prof = home.join(".config/tarsi/profiles/p");
        fs::create_dir_all(&user_prof).unwrap();
        fs::write(user_prof.join("tarsi.toml"), "[release]\n").unwrap();
        // Uninstall WITHOUT --config still finds the TARSI_PROFILES_DIR copy.
        let report = uninstall(&inv(&payload, "p"), &env).unwrap();
        let removed = report["removedProfilesDir"].as_array().unwrap();
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0], json!(profs.join("p")));
        assert!(!profs.join("p").exists());
        assert!(user_prof.join("tarsi.toml").is_file());
    }

    #[test]
    fn versioned_store_entries_resolve_newest() {
        let tmp = tempfile::tempdir().unwrap();
        let tamias_home = tmp.path().join("tamias-home");
        let store = tamias_home.join("store");
        for label in ["0.1.0", "0.2.0"] {
            let d = store.join(format!("9aia-release@{label}"));
            write_payload(&d, "9aia-release", label, "keep-a-changelog");
        }
        let home = tmp.path().join("home");
        let env = env_of(&[
            ("HOME", home.to_str().unwrap()),
            ("TAMIAS_HOME", tamias_home.to_str().unwrap()),
        ]);
        // status reports the store-sourced resolution at the newest label.
        let payload = store.join("9aia-release@0.2.0");
        let report = status(&inv(&payload, "9aia-release"), &env).unwrap();
        assert_eq!(report["resolved"], json!(store.join("9aia-release@0.2.0")));
        assert_eq!(report["source"], "tamias-store");
        let cands = report["candidates"].as_array().unwrap();
        assert_eq!(cands.len(), 2);
        // install of a third version still copies nothing by default.
        let p3 = tmp.path().join("payload3");
        write_payload(&p3, "9aia-release", "0.3.0", "keep-a-changelog");
        install(&inv(&p3, "9aia-release"), &env).unwrap();
        assert!(!home.join(".config/tarsi/profiles").exists());
    }

    #[test]
    fn deploy_resyncs_profiles_dir_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let profs = tmp.path().join("profs");
        let cfg = tmp.path().join("driver.yaml");
        fs::write(&cfg, format!("profilesDir: {}\n", profs.display())).unwrap();
        let home = tmp.path().join("home");
        let env = env_of(&[
            ("HOME", home.to_str().unwrap()),
            ("TARSI_PROFILES_DIR", profs.to_str().unwrap()),
        ]);
        // v1 installs the copy; v2's deploy refreshes it.
        let v1 = tmp.path().join("v1");
        write_payload(&v1, "p", "1.0.0", "keep-a-changelog");
        let v2 = tmp.path().join("v2");
        write_payload(&v2, "p", "2.0.0", "conventional-commit-grouped");
        let mut i1 = inv(&v1, "p");
        i1.config = Some(cfg.clone());
        install(&i1, &env).unwrap();
        assert!(profs.join("p/tarsi.toml").is_file());
        let mut i2 = inv(&v2, "p");
        i2.config = Some(cfg);
        let report = deploy(&i2, &env).unwrap();
        assert_eq!(report["ok"], true);
        let text = fs::read_to_string(profs.join("p/tarsi.toml")).unwrap();
        assert!(text.contains("conventional-commit-grouped"), "{text}");
        let marker = read_marker(&profs.join("p")).unwrap();
        assert_eq!(marker["version"], "2.0.0");
    }

    #[test]
    fn scoped_names_nest_under_profiles_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, "pkg", "1.0.0", "keep-a-changelog");
        let profs = tmp.path().join("profs");
        let cfg = tmp.path().join("driver.yaml");
        fs::write(&cfg, format!("profilesDir: {}\n", profs.display())).unwrap();
        let home = tmp.path().join("home");
        let env = env_of(&[
            ("HOME", home.to_str().unwrap()),
            ("TARSI_PROFILES_DIR", profs.to_str().unwrap()),
        ]);
        let mut i = inv(&payload, "@acme/pkg");
        i.config = Some(cfg);
        let report = install(&i, &env).unwrap();
        assert_eq!(report["profilesDir"], json!(profs.join("@acme/pkg")));
        assert!(profs.join("@acme/pkg/tarsi.toml").is_file());
        // Unsafe names never reach the filesystem.
        for bad in ["../x", "/abs", "a/../../b", ""] {
            let dest = profile_dir_for(&profs, bad);
            assert!(dest.is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn config_load_rebases_and_expands() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let env = env_of(&[("HOME", home.to_str().unwrap())]);
        // No config → defaults.
        assert_eq!(load_config(None, &env).unwrap(), DriverConfig::default());
        // Relative profilesDir rebases onto the config file's dir.
        let cfg = tmp.path().join("sub/driver.yaml");
        fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        fs::write(&cfg, "profilesDir: profs\n").unwrap();
        let loaded = load_config(Some(&cfg), &env).unwrap();
        assert_eq!(loaded.profiles_dir, Some(tmp.path().join("sub/profs")));
        // `~` expands via HOME.
        fs::write(&cfg, "profilesDir: ~/profs\n").unwrap();
        let loaded = load_config(Some(&cfg), &env).unwrap();
        assert_eq!(loaded.profiles_dir, Some(home.join("profs")));
        // Unknown keys and non-strings are errors.
        fs::write(&cfg, "profilesDir: 3\n").unwrap();
        assert!(load_config(Some(&cfg), &env).is_err());
        fs::write(&cfg, "other: x\n").unwrap();
        assert!(load_config(Some(&cfg), &env).is_err());
    }

    #[test]
    fn capabilities_and_schema_shapes() {
        let caps = capabilities_value();
        assert_eq!(caps["types"], json!(["tarsi"]));
        // Merged payload satisfies both discoverers.
        assert!(caps["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("pavo")));
        assert!(caps["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("tamias")));
        let verbs = caps["verbs"].as_array().unwrap();
        for v in ["serve", "manifest", "install", "status", "config-schema"] {
            assert!(verbs.contains(&json!(v)), "missing verb {v}");
        }
        let schema = config_schema();
        assert_eq!(
            schema["$schema"],
            "https://json-schema.org/draft/2020-12/schema"
        );
        assert!(schema["properties"]["profilesDir"].is_object());
    }

    #[test]
    fn status_health_logs_hooks_reports() {
        let tmp = tempfile::tempdir().unwrap();
        let payload = tmp.path().join("payload");
        write_payload(&payload, "p", "1.0.0", "keep-a-changelog");
        let home = tmp.path().join("home");
        let env = env_of(&[("HOME", home.to_str().unwrap())]);
        let i = inv(&payload, "p");
        let s = status(&i, &env).unwrap();
        assert_eq!(s["ok"], true);
        assert_eq!(s["resolved"], Value::Null);
        let h = health(&i, &env).unwrap();
        assert_eq!(h["ok"], true);
        assert_eq!(logs(&i)["units"], json!([]));
        assert_eq!(hooks(&i)["hooks"], json!([]));
        // Unknown verb errors.
        assert!(run_verb("bogus", &i, &env).is_err());
    }
}
