//! views — the driver's manifest: every ViewSpec plus the query
//! resolvers that feed them (the pavo read-only surface, PL-014
//! D3 remainder).
//!
//! Read path: every query is one `tarsi-serve` one-shot op — see
//! `data.rs`. `plan`/`changelog` take `dir`/`profile`/`range`
//! params (`--param dir=<repo>`, `--param profile=9aia-release`,
//! `--param range=HEAD~3..HEAD` on the pavo wire); absent `dir`
//! means `.`. A failed serve call degrades to an `{"error": …}`
//! payload inside an `ok:true` response, matching the sibling
//! drivers' contract — the views stay renderable with no
//! `tarsi-serve` on PATH.
//!
//! Surface mapping:
//!
//! | pavo view       | tarsi-serve op | data                              |
//! |-----------------|----------------|-----------------------------------|
//! | `tarsi:status`  | `manifest`     | binary, version, ops, default dir |
//! | `tarsi:profiles`| `profiles`     | release-profile list (all roots)  |
//! | `tarsi:plan`    | `plan`         | release-plan digest + unit rows   |
//! | `tarsi:changelog`| `changelog`   | per-unit changelog markdown      |
//!
//! `tarsi:plan` answers "what would a release of dir X do?" —
//! bumps, tags, publish-target switches, unattributed commits,
//! ordered steps — without mutating anything (`plan` is read-only
//! in the serve; `release` is never called).

use pavo_driver_kit as kit;
use pavo_protocol::wire::ViewResult;
use pavo_protocol::{DriverManifest, TableColumn, ViewNode, ViewSpec};
use serde_json::{json, Map, Value};

use crate::data;
use crate::Env;

// ---------------------------------------------------------------- helpers
// Node/manifest construction goes through pavo-driver-kit — the
// constructors emit the same ViewNode/QueryBinding structs the
// hand-rolled literals did.

fn col(title: &str, children: Vec<ViewNode>) -> ViewNode {
    kit::column(kit::base().title(title), children)
}

fn kv(title: &str, query: &str) -> ViewNode {
    kit::kv(kit::base().title(title).query(query), Vec::new())
}

fn md(title: &str, text: &str) -> ViewNode {
    kit::markdown(kit::base().title(title), text)
}

fn table(title: &str, query: &str, columns: &[(&str, &str)]) -> ViewNode {
    kit::sortable_table(
        kit::base().title(title).query(query),
        columns
            .iter()
            .map(|(key, t)| kit::table_column(key, t))
            .collect::<Vec<TableColumn>>(),
        Vec::new(),
    )
}

fn spec(
    id: &str,
    title: &str,
    route: &str,
    description: &str,
    queries: &[&str],
    root: ViewNode,
) -> ViewSpec {
    let mut view = kit::view(id, title, route, root).description(description);
    for q in queries {
        view = view.query(kit::binding(q, "query"));
    }
    view.build()
}

// ---------------------------------------------------------------- manifest

const PROFILE_COLS: &[(&str, &str)] = &[
    ("name", "Name"),
    ("version", "Version"),
    ("description", "Description"),
    ("source", "Source"),
];

const UNIT_COLS: &[(&str, &str)] = &[
    ("unit", "Unit"),
    ("path", "Path"),
    ("bump", "Bump"),
    ("new_version", "New version"),
    ("tag", "Tag"),
    ("releases", "Releases"),
    ("commits", "Commits"),
    ("targets", "Publish targets"),
];

/// Every view the driver declares — static (no tarsi state needed;
/// `manifest`/`views` must answer anywhere).
pub fn specs() -> Vec<ViewSpec> {
    vec![
        spec(
            "status",
            "Status",
            "/tarsi/status",
            "tarsi-serve reachability — binary, version, and the ops it answers. Params: dir=<repo> sets the default plan dir shown in the digest.",
            &["summary"],
            col("Tarsi serve", vec![kv("Serve", "summary")]),
        ),
        spec(
            "profiles",
            "Profiles",
            "/tarsi/profiles",
            "Release profiles visible to tarsi — dir profiles, ~/.config/tarsi, and tamias-store packs (name@label), from `tarsi-serve profiles`.",
            &["profiles"],
            table("Profiles", "profiles", PROFILE_COLS),
        ),
        spec(
            "plan",
            "Release plan",
            "/tarsi/plan",
            "What a release of the target dir would do — per-unit bumps, new versions, tags, publish-target switches, unattributed commits, and the ordered step list. Read-only (`tarsi-serve plan`). Params: dir=<repo> (default .), profile=<name>, range=<rev>..<rev>.",
            &["plan", "units"],
            col(
                "Release plan",
                vec![
                    kv("Plan", "plan"),
                    table("Units", "units", UNIT_COLS),
                ],
            ),
        ),
        spec(
            "changelog",
            "Changelog preview",
            "/tarsi/changelog",
            "The changelog markdown a release would write, per releasing unit — `tarsi-serve changelog`. Params: dir=<repo> (default .), profile=<name>, range=<rev>..<rev>.",
            &["changelog"],
            // The manifest root is a placeholder — `view_result`
            // inlines the resolved markdown (markdown nodes aren't
            // query-bound in the IR; the spec is personalized per
            // request, the myco-driver pattern).
            col("Changelog preview", vec![md("Preview", "*(loading…)*")]),
        ),
    ]
}

/// The `manifest` op result — built with `pavo_driver_kit`'s
/// `Manifest` fluent builder.
pub fn manifest() -> DriverManifest {
    let mut navbar = kit::contribution("plan", "Release plan", "/tarsi/plan");
    navbar.priority = 20;
    let mut dashboard = kit::contribution("status", "Tarsi status", "/tarsi/status");
    dashboard.priority = 30;

    let mut manifest = kit::Manifest::new("tarsi", "Tarsi")
        .version(env!("CARGO_PKG_VERSION"))
        .navbar(navbar)
        .dashboard(dashboard);
    for view in specs() {
        manifest = manifest.view(view);
    }
    manifest.build()
}

// ---------------------------------------------------------------- queries

/// Every query name the driver answers — the `query` op rejects
/// anything outside this set with `ok:false`.
pub fn known_query(name: &str) -> bool {
    matches!(
        name,
        "summary" | "profiles" | "plan" | "units" | "changelog"
    )
}

/// `{"op":"query","query":name,"params":{…}}` → the data payload.
/// Failures degrade to `{"error": …}` objects, never `ok:false`.
pub fn query_data(name: &str, params: &Map<String, Value>, env: Env<'_>) -> Value {
    match name {
        "summary" => summary(params, env),
        "profiles" => profiles_rows(env),
        "plan" => plan_digest(params, env),
        "units" => units_rows(params, env),
        "changelog" => changelog_data(params, env),
        other => json!({"error": format!("unknown query '{other}'")}),
    }
}

/// `summary` query — the status view's kv object: which
/// `tarsi-serve` binary answered (or why none did), its version +
/// ops, and the dir plan queries default to.
fn summary(params: &Map<String, Value>, env: Env<'_>) -> Value {
    let dir = data::repo_dir(params);
    let mut out = Map::new();
    out.insert(
        "binary".into(),
        json!(env("TARSI_SERVE")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "tarsi-serve".to_string())),
    );
    out.insert("dir".into(), json!(dir));
    match data::manifest_payload(env) {
        Ok(v) => {
            out.insert("serve".into(), json!("ok"));
            out.insert("version".into(), v["version"].clone());
            out.insert("ops".into(), v["ops"].clone());
        }
        Err(e) => {
            out.insert("serve".into(), json!("error"));
            out.insert("error".into(), json!(e));
        }
    }
    Value::Object(out)
}

/// `profiles` query — `tarsi-serve profiles` rows verbatim
/// (`{profiles: […]}` unwraps to the array the table binds).
fn profiles_rows(env: Env<'_>) -> Value {
    match data::profiles_payload(env) {
        Ok(v) => v["profiles"].clone(),
        Err(e) => json!({"error": e}),
    }
}

/// The `plan` op payload → `{specs, plan}`; errors degrade.
fn plan_or_error(params: &Map<String, Value>, env: Env<'_>) -> Value {
    match data::plan_payload(params, env) {
        Ok(v) => v,
        Err(e) => json!({"error": e}),
    }
}

/// `plan` query — the digest half of the plan view: target dir,
/// the effective profile/range params, ordered steps, and counts.
fn plan_digest(params: &Map<String, Value>, env: Env<'_>) -> Value {
    let payload = plan_or_error(params, env);
    if let Some(e) = payload.get("error").and_then(Value::as_str) {
        return json!({"error": e});
    }
    let plan = &payload["plan"];
    let units = plan["units"].as_array().map(Vec::len).unwrap_or(0);
    let releasing = plan["units"]
        .as_array()
        .map(|us| {
            us.iter()
                .filter(|u| u["releases"].as_bool().unwrap_or(false))
                .count()
        })
        .unwrap_or(0);
    json!({
        "dir": data::repo_dir(params),
        "profile": params.get("profile").cloned().unwrap_or(Value::Null),
        "range": params.get("range").cloned().unwrap_or(Value::Null),
        "steps": plan["steps"],
        "units": units,
        "releasing": releasing,
        "unattributed": plan["unattributed"].as_array().map(Vec::len).unwrap_or(0),
    })
}

/// `spec.publish` targets → `"crates.io:on, ghreleases:off"` —
/// the effective switch (`publish.<target>.enabled`), not just
/// the declaration, since the serve already resolved it.
fn targets_cell(u: &Value) -> String {
    u["publish_targets"]
        .as_array()
        .map(|ts| {
            ts.iter()
                .map(|t| {
                    format!(
                        "{}:{}",
                        t["target"].as_str().unwrap_or("?"),
                        if t["enabled"].as_bool().unwrap_or(false) {
                            "on"
                        } else {
                            "off"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

/// One plan unit → table row.
fn unit_row(u: &Value) -> Value {
    json!({
        "unit": u["spec"]["name"],
        "path": u["spec"]["path"],
        "bump": u["bump"],
        "new_version": u["new_version"],
        "tag": u["tag"],
        "releases": u["releases"],
        "commits": u["commits"],
        "targets": targets_cell(u),
    })
}

/// `units` query — per-unit release rows for the plan table.
fn units_rows(params: &Map<String, Value>, env: Env<'_>) -> Value {
    let payload = plan_or_error(params, env);
    if let Some(e) = payload.get("error").and_then(Value::as_str) {
        return json!({"error": e});
    }
    Value::Array(
        payload["plan"]["units"]
            .as_array()
            .map(|us| us.iter().map(unit_row).collect())
            .unwrap_or_default(),
    )
}

/// `changelog` query — the serve's `{changelogs: […]}` payload
/// verbatim (each entry: unit, version, tag, markdown).
fn changelog_data(params: &Map<String, Value>, env: Env<'_>) -> Value {
    match data::changelog_payload(params, env) {
        Ok(v) => v,
        Err(e) => json!({"error": e}),
    }
}

// ---------------------------------------------------------------- view op

/// `{"op":"view","view":id,"params":{…}}` → [`ViewResult`].
///
/// Every view resolves its bound queries against `params`; the
/// `changelog` view additionally inlines the resolved markdown
/// into its root (markdown nodes aren't query-bound — the spec is
/// personalized per request, the myco-driver pattern).
pub fn view_result(
    view_id: &str,
    params: &Map<String, Value>,
    env: Env<'_>,
) -> Result<ViewResult, String> {
    let spec = specs()
        .into_iter()
        .find(|v| v.id == view_id)
        .ok_or_else(|| format!("unknown view: {view_id}"))?;

    let mut data_map = Map::new();
    for b in &spec.queries {
        data_map.insert(b.name.clone(), query_data(&b.name, params, env));
    }

    let root = if view_id == "changelog" {
        changelog_root(&data_map["changelog"])
    } else {
        spec.root.clone()
    };

    Ok(ViewResult {
        view: ViewSpec { root, ..spec },
        data: data_map,
    })
}

/// The `changelog` view's personalized tree — one markdown node
/// per releasing unit (`## <unit> <version>` heading already lives
/// in each entry's markdown; the node title carries unit+tag), or
/// a single error/note node when the serve call degraded.
fn changelog_root(payload: &Value) -> ViewNode {
    if let Some(e) = payload.get("error").and_then(Value::as_str) {
        return col(
            "Changelog preview",
            vec![md("Preview", &format!("*(error: {e})*"))],
        );
    }
    let entries = payload["changelogs"].as_array();
    let Some(entries) = entries else {
        return col(
            "Changelog preview",
            vec![md("Preview", "*(no changelog payload)*")],
        );
    };
    if entries.is_empty() {
        return col(
            "Changelog preview",
            vec![md(
                "Preview",
                "*(nothing to release — no changelog preview)*",
            )],
        );
    }
    let mut nodes = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let unit = entry["unit"].as_str().unwrap_or("unit");
        let version = entry["version"].as_str().unwrap_or("");
        let title = if version.is_empty() {
            unit.to_string()
        } else {
            format!("{unit} {version}")
        };
        // The tag already rides inside each entry's markdown heading.
        let text = entry["markdown"].as_str().unwrap_or("");
        let mut node = md(&title, text);
        // Keep node ids stable + unique across units.
        if let ViewNode::Markdown(m) = &mut node {
            m.base.id = Some(format!("changelog-{i}"));
        }
        nodes.push(node);
    }
    col("Changelog preview", nodes)
}

// ---------------------------------------------------------------- tests
// (unit tests live at the bottom — see `mod tests`)

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use pavo_protocol::wire::ViewSummary;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn env_of(vars: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: BTreeMap<String, String> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    /// A stub `tarsi-serve`: a shell script that answers the
    /// one-shot envelope for the ops the views call, and records
    /// its argv so tests can assert the flags that were passed.
    #[cfg(unix)]
    fn stub_serve(dir: &Path, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("tarsi-serve");
        std::fs::write(&script, body).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    #[test]
    fn manifest_advertises_pavo_and_lists_every_view() {
        let m = manifest();
        assert!(m.verbs.iter().any(|v| v == "serve"));
        assert!(m.capabilities.iter().any(|c| c == "pavo"));
        assert!(m.renderers.is_empty());
        let ids: std::collections::BTreeSet<&str> = m.views.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids.len(), m.views.len(), "duplicate view ids");
        for want in ["status", "profiles", "plan", "changelog"] {
            assert!(ids.contains(want), "missing view {want}");
        }
        assert!(m
            .contributes
            .navbar
            .iter()
            .any(|c| c.route == "/tarsi/plan"));
    }

    #[test]
    fn every_binding_is_a_known_query() {
        for v in specs() {
            for b in &v.queries {
                assert!(known_query(&b.name), "{}: unknown query {}", v.id, b.name);
            }
        }
        assert!(!known_query("bogus"));
    }

    #[test]
    fn summaries_degrade_without_serve() {
        // TARSI_SERVE points at a nonexistent binary — queries must
        // degrade to {"error": …} payloads, never panic/err the op.
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("no-such-tarsi-serve");
        let env = env_of(&[("TARSI_SERVE", missing.to_str().unwrap())]);
        let v = summary(&Map::new(), &env);
        assert_eq!(v["serve"], "error");
        assert!(v["error"].as_str().unwrap().contains("tarsi-serve"));
        assert_eq!(v["dir"], json!(Path::new(".")));
        let p = profiles_rows(&env);
        assert!(p["error"].as_str().unwrap().contains("tarsi"));
        let d = plan_digest(&Map::new(), &env);
        assert!(d["error"].is_string());
        let u = units_rows(&Map::new(), &env);
        assert!(u["error"].is_string());
        let c = changelog_data(&Map::new(), &env);
        assert!(c["error"].is_string());
    }

    #[cfg(unix)]
    #[test]
    fn queries_ride_the_serve_one_shot() {
        let tmp = tempfile::tempdir().unwrap();
        // Stub answers each op with a canned envelope and appends
        // its argv to a log for assertion.
        let log = tmp.path().join("argv.log");
        let log_s = log.display().to_string();
        let script = format!(
            "#!/bin/sh\necho \"$@\" >> {log_s}\n\
             case \"$1\" in\n\
             manifest) echo '{{\"ok\":true,\"result\":{{\"name\":\"tarsi-serve\",\"version\":\"9.9.9\",\"ops\":[\"plan\"]}}}}' ;;\n\
             profiles) echo '{{\"ok\":true,\"result\":{{\"profiles\":[{{\"name\":\"p\",\"version\":\"1.0.0\",\"description\":\"d\",\"source\":\"tamias-store\"}}]}}}}' ;;\n\
             plan) echo '{{\"ok\":true,\"result\":{{\"specs\":[],\"plan\":{{\"units\":[{{\"spec\":{{\"name\":\"u\",\"path\":\".\"}},\"bump\":\"minor\",\"new_version\":\"1.1.0\",\"tag\":\"u-v1.1.0\",\"releases\":true,\"changelog_markdown\":\"# x\",\"commits\":2,\"publish_targets\":[{{\"target\":\"crates-io\",\"enabled\":false}}]}}],\"unattributed\":[],\"steps\":[\"commit-bump\"]}}}}}}' ;;\n\
             changelog) echo '{{\"ok\":true,\"result\":{{\"changelogs\":[{{\"unit\":\"u\",\"version\":\"1.1.0\",\"tag\":\"u-v1.1.0\",\"markdown\":\"## u 1.1.0\\n\\n- feat: x\"}}]}}}}' ;;\n\
             *) echo '{{\"ok\":false,\"error\":\"unknown op\"}}' ;;\n\
             esac\n"
        );
        let bin = stub_serve(tmp.path(), &script);
        let env = env_of(&[("TARSI_SERVE", bin.to_str().unwrap())]);

        // summary → manifest payload reshaped.
        let mut params = Map::new();
        params.insert("dir".into(), json!("/repo"));
        let s = summary(&params, &env);
        assert_eq!(s["serve"], "ok");
        assert_eq!(s["version"], "9.9.9");
        assert_eq!(s["dir"], json!(Path::new("/repo")));

        // profiles → the array, not the envelope.
        let p = profiles_rows(&env);
        assert_eq!(p[0]["name"], "p");
        assert_eq!(p[0]["source"], "tamias-store");

        // plan digest + units rows.
        let d = plan_digest(&params, &env);
        assert_eq!(d["units"], 1);
        assert_eq!(d["releasing"], 1);
        assert_eq!(d["steps"], json!(["commit-bump"]));
        let u = units_rows(&params, &env);
        let rows = u.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["unit"], "u");
        assert_eq!(rows[0]["new_version"], "1.1.0");
        assert_eq!(rows[0]["targets"], "crates-io:off");

        // changelog → passthrough payload.
        let c = changelog_data(&params, &env);
        assert_eq!(c["changelogs"][0]["unit"], "u");

        // profile/range params ride --set.
        let mut p2 = Map::new();
        p2.insert("profile".into(), json!("9aia-release"));
        p2.insert("range".into(), json!("HEAD~3..HEAD"));
        let _ = plan_digest(&p2, &env);
        let log_text = std::fs::read_to_string(&log).unwrap();
        assert!(
            log_text.contains("--set profile=9aia-release"),
            "{log_text}"
        );
        assert!(log_text.contains("--set range=HEAD~3..HEAD"), "{log_text}");
        assert!(log_text.contains("--dir /repo"), "{log_text}");
    }

    #[cfg(unix)]
    #[test]
    fn serve_error_envelope_becomes_err() {
        let tmp = tempfile::tempdir().unwrap();
        let script = "#!/bin/sh\necho '{\"ok\":false,\"error\":\"boom\"}'\n";
        let bin = stub_serve(tmp.path(), script);
        let env = env_of(&[("TARSI_SERVE", bin.to_str().unwrap())]);
        let e = data::serve_op("plan", Path::new("."), &[], &env).unwrap_err();
        assert!(e.contains("boom"), "{e}");
        // And the query layer degrades it.
        let v = plan_digest(&Map::new(), &env);
        assert_eq!(v["error"], "tarsi: boom");
    }

    #[test]
    fn changelog_root_inlines_markdown() {
        let payload = json!({"changelogs": [
            {"unit": "a", "version": "1.0.0", "tag": "a-v1.0.0", "markdown": "## a\n\n- x"},
            {"unit": "b", "version": "2.0.0", "tag": "b-v2.0.0", "markdown": "## b\n\n- y"},
        ]});
        let root = changelog_root(&payload);
        let ViewNode::Column(c) = &root else {
            panic!("expected column");
        };
        assert_eq!(c.children.len(), 2);
        let ViewNode::Markdown(m0) = &c.children[0] else {
            panic!("expected markdown");
        };
        assert_eq!(m0.base.title.as_deref(), Some("a 1.0.0"));
        assert!(m0.text.contains("- x"));

        // Error payload → single note node.
        let err_root = changelog_root(&json!({"error": "no serve"}));
        let ViewNode::Column(c2) = &err_root else {
            panic!("expected column");
        };
        let ViewNode::Markdown(m) = &c2.children[0] else {
            panic!("expected markdown");
        };
        assert!(m.text.contains("no serve"));

        // Empty changelogs → "nothing to release" note.
        let empty = changelog_root(&json!({"changelogs": []}));
        let ViewNode::Column(c3) = &empty else {
            panic!("expected column");
        };
        let ViewNode::Markdown(m3) = &c3.children[0] else {
            panic!("expected markdown");
        };
        assert!(m3.text.contains("nothing to release"));
    }

    #[test]
    fn view_result_binds_queries_and_personalizes_changelog() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("no-such-tarsi-serve");
        let env = env_of(&[("TARSI_SERVE", missing.to_str().unwrap())]);
        let params = Map::new();
        // Unknown view errors.
        assert!(view_result("bogus", &params, &env).is_err());
        // Known views answer with every binding populated — degraded
        // payloads included.
        for id in ["status", "profiles", "plan", "changelog"] {
            let r = view_result(id, &params, &env).unwrap();
            assert_eq!(r.view.id, id);
            for b in &r.view.queries {
                assert!(r.data.contains_key(&b.name), "{id}: missing {}", b.name);
            }
        }
        // changelog root was personalized (error note inlined).
        let r = view_result("changelog", &params, &env).unwrap();
        let ViewNode::Column(c) = &r.view.root else {
            panic!("expected column");
        };
        let ViewNode::Markdown(m) = &c.children[0] else {
            panic!("expected markdown");
        };
        assert!(m.text.contains("error"));
    }

    #[test]
    fn param_helpers() {
        let mut p = Map::new();
        assert_eq!(data::repo_dir(&p), Path::new("."));
        p.insert("dir".into(), json!("/x"));
        p.insert("profile".into(), json!("prof"));
        p.insert("range".into(), json!("a..b"));
        p.insert("bogus".into(), json!("ignored"));
        assert_eq!(data::repo_dir(&p), Path::new("/x"));
        let sets = data::plan_sets(&p);
        assert_eq!(
            sets,
            vec![
                ("profile".to_string(), "prof".to_string()),
                ("range".to_string(), "a..b".to_string())
            ]
        );
        // Empty strings don't ride --set.
        p.insert("profile".into(), json!(""));
        assert_eq!(data::plan_sets(&p).len(), 1);
    }

    // Keep ViewSummary referenced — dispatch uses it.
    #[test]
    fn summaries_shape() {
        let v: ViewSummary = ViewSummary::from(&specs()[0]);
        assert_eq!(v.id, "status");
    }

    #[test]
    fn targets_cell_renders_enabled_flags() {
        let u = json!({"publish_targets": [
            {"target": "crates-io", "enabled": true},
            {"target": "ghreleases", "enabled": false},
        ]});
        assert_eq!(targets_cell(&u), "crates-io:on, ghreleases:off");
        assert_eq!(targets_cell(&json!({})), "");
    }

    #[test]
    fn unit_row_maps_plan_fields() {
        let u = json!({
            "spec": {"name": "demo", "path": "crates/demo"},
            "bump": "patch",
            "new_version": "1.0.1",
            "tag": "demo-v1.0.1",
            "releases": true,
            "commits": 3,
            "publish_targets": [{"target": "crates-io", "enabled": true}],
        });
        let row = unit_row(&u);
        assert_eq!(row["unit"], "demo");
        assert_eq!(row["path"], "crates/demo");
        assert_eq!(row["commits"], 3);
        assert_eq!(row["targets"], "crates-io:on");
    }
}
