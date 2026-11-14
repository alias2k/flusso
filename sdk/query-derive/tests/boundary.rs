//! The derive's boundary, pinned (issue #294): every type shape × field kind
//! × position, each compiled and read.
//!
//! The generator places each [`SHAPES`] type at each field of the `boundary`
//! fixture index ([`KINDS`]), on a root and in a fragment, plus the map-like
//! shapes `#[serde(flatten)]`ed into a root ([`FLATTENED`]). Each case is one
//! module in a generated crate, compiled with cargo, then read against
//! sample documents of that index ([`DOCS`]). It ends up in one of three
//! outcomes:
//!
//! - `refused`: the derive turns it down at compile time;
//! - `reads`: it compiles, and reads every sample document;
//! - `trusted <rule>`: it compiles, and a sample document fails to read —
//!   allowed only under a rule of [`TRUSTED`], each the manual's list of what
//!   the derive trusts (`docs/src/query/document-structs.md`).
//!
//! A case that compiles and fails to read under no rule fails the test. The
//! outcomes are recorded in `tests/boundary.outcomes`, so a change to what the
//! derive refuses or reads shows up there as a diff to review; re-record with
//! `FLUSSO_BLESS=1 cargo nextest run -p flusso-query-derive --test boundary`.
//!
//! The generated crate shares trybuild's target directory and dependency set,
//! so it reuses what the `ui` harness built. Cases go in files of at most
//! [`CHUNK`] modules, compiled three at a time: generated derives are heavy
//! on `rustc`'s memory. A file's refused cases can hide the next phase's
//! errors in the same file, so refused cases are dropped and the rest are
//! compiled again until everything left builds.
#![allow(
    unused_crate_dependencies,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout
)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The most case modules in one generated file.
const CHUNK: usize = 50;

const MANIFEST_DIR: &str = env!("CARGO_MANIFEST_DIR");

/// A Rust type the generator places: the declarations it needs, and the
/// type as a field writes it.
struct Shape {
    name: &'static str,
    decls: &'static [&'static str],
    ty: &'static str,
}

const SUB: &str = "#[derive(Debug, Deserialize, FlussoFragment)] pub struct Sub { pub k: String }";
const EN: &str = "#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] \
     #[serde(rename_all = \"lowercase\")] pub enum En { A, B }";
const MAP_KW: &str = "#[derive(Debug, Serialize, Deserialize, FlussoMap)] #[flusso(keyword)] \
     pub struct MapKw(pub HashMap<String, String>);";
const FRAGMENT_OVER_MAP: &str =
    "#[derive(Debug, Deserialize, FlussoFragment)] pub struct Fm(pub HashMap<String, String>);";

macro_rules! shape {
    ($name:literal, $ty:literal) => {
        Shape { name: $name, decls: &[], ty: $ty }
    };
    ($name:literal, $ty:literal, $($decl:expr),+) => {
        Shape { name: $name, decls: &[$($decl),+], ty: $ty }
    };
}

const SHAPES: &[Shape] = &[
    shape!("string", "String"),
    shape!("long", "i64"),
    shape!("vec_string", "Vec<String>"),
    shape!(
        "code",
        "Code",
        "#[derive(Debug, Serialize, Deserialize, FlussoValue)] pub struct Code(pub String);"
    ),
    shape!(
        "tags",
        "Tags",
        "#[derive(Debug, Serialize, Deserialize, FlussoValue)] pub struct Tags(pub Vec<String>);"
    ),
    shape!("enum", "En", EN),
    shape!("value", "serde_json::Value"),
    shape!("sub", "Sub", SUB),
    shape!("vec_sub", "Vec<Sub>", SUB),
    shape!(
        "subs",
        "Subs",
        SUB,
        "#[derive(Debug, Deserialize, FlussoFragment)] pub struct Subs(pub Vec<Sub>);"
    ),
    shape!("hash_map", "HashMap<String, String>"),
    shape!("btree_map_long", "BTreeMap<String, i64>"),
    shape!("hash_map_enum", "HashMap<String, En>", EN),
    shape!("fragment_over_map", "Fm", FRAGMENT_OVER_MAP),
    shape!("vec_fragment_over_map", "Vec<Fm>", FRAGMENT_OVER_MAP),
    shape!("map_keyword", "MapKw", MAP_KW),
    shape!(
        "map_text",
        "MapTx",
        "#[derive(Debug, Deserialize, FlussoMap)] #[flusso(text)] \
         pub struct MapTx(pub HashMap<String, String>);"
    ),
    shape!(
        "map_keyword_of_long",
        "MapKl",
        "#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] \
         pub struct MapKl(pub HashMap<String, i64>);"
    ),
    shape!(
        "map_named",
        "Named",
        "#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct Named { \
         pub x: Option<String>, #[serde(flatten)] pub rest: HashMap<String, String> }"
    ),
    shape!(
        "map_named_required",
        "NamedReq",
        "#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] \
         pub struct NamedReq { pub x: String }"
    ),
    shape!(
        "map_over_box",
        "Boxed",
        "#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] \
         pub struct Boxed(pub Box<String>);"
    ),
    shape!(
        "map_over_map_type",
        "Mm",
        MAP_KW,
        "#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct Mm(pub MapKw);"
    ),
    shape!(
        "value_over_map",
        "Vm",
        MAP_KW,
        "#[derive(Debug, Serialize, Deserialize, FlussoValue)] pub struct Vm(pub MapKw);"
    ),
    shape!(
        "fragment_over_map_type",
        "Fmt",
        MAP_KW,
        "#[derive(Debug, Deserialize, FlussoFragment)] pub struct Fmt(pub MapKw);"
    ),
];

/// The fields of the `boundary` fixture index: one of each kind.
const KINDS: &[&str] = &["kw", "n", "kwa", "ob", "ne", "mp", "mpn", "js", "jsa"];

/// The shapes also placed `#[serde(flatten)]` beside `kw` on a root, where
/// they read every other key of the document.
const FLATTENED: &[&str] = &[
    "hash_map",
    "btree_map_long",
    "value",
    "sub",
    "map_keyword",
    "map_named",
    "fragment_over_map",
];

/// Sample documents of the `boundary` index, as the sink writes them: every
/// field set, then every array and map empty, then values a plain `keyword`
/// takes and an enum of `a`/`b` does not.
const DOCS: &[&str] = &[
    r#"{"id":"1","kw":"a","n":1,"kwa":["a"],"ob":{"k":"a","n":1},"ne":[{"k":"a","n":1}],"mp":{"x":"a"},"mpn":{"x":1},"js":{"any":[1]},"jsa":[1,"x"]}"#,
    r#"{"id":"2","kw":"b","n":2,"kwa":[],"ob":{"k":"b","n":2},"ne":[],"mp":{},"mpn":{},"js":{},"jsa":[]}"#,
    r#"{"id":"3","kw":"z","n":3,"kwa":["z"],"ob":{"k":"z","n":3},"ne":[{"k":"z","n":3}],"mp":{"x":"z"},"mpn":{"y":3},"js":"z","jsa":["z"]}"#,
];

/// Where the generator places a shape.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    /// A field of the root: `R { kw: T }`.
    Root,
    /// A field of a fragment flattened into the root: `F { kw: T }`.
    Fragment,
    /// `#[serde(flatten)]` beside `kw` on the root.
    Flattened,
}

impl Position {
    fn name(self) -> &'static str {
        match self {
            Position::Root => "root",
            Position::Fragment => "fragment",
            Position::Flattened => "flattened",
        }
    }
}

struct Case {
    id: String,
    shape: &'static Shape,
    kind: &'static str,
    position: Position,
}

/// What the derive trusts, each the manual's wording of it.
struct Trust {
    rule: &'static str,
    /// A phrase of the manual's list that states this rule.
    manual: &'static str,
    applies: fn(&Case) -> bool,
}

const TRUSTED: &[Trust] = &[
    Trust {
        rule: "json",
        manual: "**Any type at a `json` field.**",
        applies: |case| matches!(case.kind, "js" | "jsa") && case.position != Position::Flattened,
    },
    Trust {
        rule: "flattened",
        manual: "**A `#[serde(flatten)]` map.**",
        applies: |case| case.position == Position::Flattened,
    },
    Trust {
        rule: "variants",
        manual: "**An enum where the schema declares no `variants:`.**",
        applies: |case| case.shape.decls.contains(&EN),
    },
];

fn cases() -> Vec<Case> {
    let mut cases = Vec::new();
    for position in [Position::Root, Position::Fragment] {
        for shape in SHAPES {
            for kind in KINDS {
                cases.push(Case {
                    id: format!("{}/{}/{kind}", position.name(), shape.name),
                    shape,
                    kind,
                    position,
                });
            }
        }
    }
    for name in FLATTENED {
        let shape = SHAPES.iter().find(|shape| shape.name == *name).unwrap();
        cases.push(Case {
            id: format!("flattened/{name}"),
            shape,
            kind: "",
            position: Position::Flattened,
        });
    }
    cases
}

const HEADER: &str =
    "#![allow(unused_crate_dependencies, dead_code, unused_imports, non_snake_case)]
use std::collections::{BTreeMap, HashMap};
use flusso_query::{FlussoFragment, FlussoMap, FlussoRoot, FlussoValue};
use serde::{Deserialize, Serialize};
";

/// One case as a module, with a `read` that deserializes a document into its
/// root.
fn module(index: usize, case: &Case) -> String {
    let ty = case.shape.ty;
    let kind = case.kind;
    let (fragment, root) = match case.position {
        Position::Root => (String::new(), format!("pub {kind}: {ty}")),
        Position::Fragment => (
            format!(
                "#[derive(Debug, Deserialize, FlussoFragment)] pub struct F {{ pub {kind}: {ty} }}"
            ),
            "#[serde(flatten)] pub f: F".to_string(),
        ),
        Position::Flattened => (
            String::new(),
            format!("pub kw: String, #[serde(flatten)] pub rest: {ty}"),
        ),
    };
    format!(
        "mod c{index} {{\nuse super::*;\n{}\n{fragment}\n\
         #[derive(Debug, Deserialize, FlussoRoot)] #[flusso(index = \"boundary\")] pub struct R {{ {root} }}\n\
         pub fn read(doc: &str) -> Result<(), String> {{ serde_json::from_str::<R>(doc).map(drop).map_err(|e| e.to_string()) }}\n}}\n",
        case.shape.decls.join("\n")
    )
}

/// A generated file: its source, and the line range each case owns.
struct File {
    source: String,
    lines: Vec<(usize, std::ops::Range<usize>)>,
}

fn file(cases: &[Case], members: &[usize]) -> File {
    let mut source = HEADER.to_string();
    let docs = DOCS
        .iter()
        .map(|doc| format!("r#\"{doc}\"#"))
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(source, "const DOCS: &[&str] = &[{docs}];").unwrap();
    let mut lines = Vec::new();
    for &index in members {
        let start = source.lines().count() + 1;
        source.push_str(&module(index, &cases[index]));
        lines.push((index, start..source.lines().count() + 1));
    }
    source
        .push_str("fn main() {\n    let cases: &[(usize, fn(&str) -> Result<(), String>)] = &[\n");
    for &index in members {
        writeln!(source, "        ({index}, c{index}::read),").unwrap();
    }
    source.push_str(
        "    ];\n    for (index, read) in cases {\n        for doc in DOCS {\n            \
         if let Err(error) = read(doc) {\n                println!(\"{index}\\t{error}\");\n                \
         break;\n            }\n        }\n    }\n}\n",
    );
    File { source, lines }
}

/// The generated crate, beside trybuild's under `target/tests/trybuild`.
struct Project {
    dir: PathBuf,
    target: PathBuf,
}

impl Project {
    fn new() -> Project {
        let target = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .parent()
            .unwrap()
            .to_path_buf();
        let target = target.join("tests").join("trybuild");
        let dir = target.join("flusso-query-derive-boundary");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let workspace = Path::new(MANIFEST_DIR)
            .join("../..")
            .canonicalize()
            .unwrap();
        std::fs::copy(workspace.join("Cargo.lock"), dir.join("Cargo.lock")).unwrap();
        Project { dir, target }
    }

    /// The manifest trybuild writes for this crate (its dependencies and
    /// dev-dependencies, the workspace's dependency table), so the two share
    /// every dependency artifact; plus one bin per generated file.
    fn manifest(&self, bins: &[String]) -> String {
        let crate_dir = Path::new(MANIFEST_DIR);
        let workspace = crate_dir.join("../..").canonicalize().unwrap();
        let own = read_toml(&crate_dir.join("Cargo.toml"));
        let root = read_toml(&workspace.join("Cargo.toml"));
        let mut dependencies = toml::Table::new();
        for table in ["dependencies", "dev-dependencies"] {
            for (name, spec) in own[table].as_table().unwrap() {
                if name != "trybuild" {
                    dependencies.insert(name.clone(), absolute(spec.clone(), crate_dir));
                }
            }
        }
        let mut this = toml::Table::new();
        this.insert("path".into(), crate_dir.display().to_string().into());
        this.insert("default-features".into(), false.into());
        dependencies.insert("flusso-query-derive".into(), this.into());
        let mut workspace_dependencies = toml::Table::new();
        for (name, spec) in root["workspace"]["dependencies"].as_table().unwrap() {
            workspace_dependencies.insert(name.clone(), absolute(spec.clone(), &workspace));
        }
        let mut manifest = toml::Table::new();
        let mut package = toml::Table::new();
        package.insert("name".into(), "flusso-query-derive-boundary".into());
        package.insert("version".into(), "0.0.0".into());
        package.insert("edition".into(), "2024".into());
        package.insert("publish".into(), false.into());
        manifest.insert("package".into(), package.into());
        manifest.insert("dependencies".into(), dependencies.into());
        let mut ws = toml::Table::new();
        ws.insert("dependencies".into(), workspace_dependencies.into());
        manifest.insert("workspace".into(), ws.into());
        if let Some(patch) = root.get("patch") {
            manifest.insert("patch".into(), patch.clone());
        }
        let bins = bins
            .iter()
            .map(|name| {
                let mut bin = toml::Table::new();
                bin.insert("name".into(), name.clone().into());
                bin.insert("path".into(), format!("src/{name}.rs").into());
                toml::Value::Table(bin)
            })
            .collect::<Vec<_>>();
        manifest.insert("bin".into(), bins.into());
        toml::to_string(&manifest).unwrap()
    }

    /// `cargo build`, as trybuild runs it, with JSON diagnostics.
    fn build(&self, bins: &[String]) -> (bool, String) {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let output = Command::new(cargo)
            .current_dir(&self.dir)
            .env("CARGO_TARGET_DIR", &self.target)
            .env("CARGO_INCREMENTAL", "0")
            .env_remove("RUSTFLAGS")
            .env(
                "FLUSSO_CONFIG",
                Path::new(MANIFEST_DIR).join("tests/fixtures/flusso.toml"),
            )
            .args([
                "build",
                "--offline",
                "--quiet",
                "--keep-going",
                "--message-format=json",
                "-j3",
            ])
            .arg(
                r#"--config=build.rustflags=["--cfg", "trybuild", "--verbose", "-A", "dead_code"]"#,
            )
            .args(bins.iter().flat_map(|bin| ["--bin", bin.as_str()]))
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() || !String::from_utf8_lossy(&output.stdout).trim().is_empty(),
            "cargo failed before compiling anything:\n{stderr}"
        );
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    }
}

fn read_toml(path: &Path) -> toml::Table {
    toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A dependency spec with its `path` made absolute against `base`.
fn absolute(spec: toml::Value, base: &Path) -> toml::Value {
    let toml::Value::Table(mut table) = spec else {
        return spec;
    };
    if let Some(toml::Value::String(path)) = table.get("path") {
        let path = base.join(path).canonicalize().unwrap();
        table.insert("path".into(), path.display().to_string().into());
    }
    toml::Value::Table(table)
}

/// The cases each error of a build belongs to, by its primary span's line in
/// a generated file (following a macro expansion back to its call site). An
/// error no case owns is returned as text.
fn refused_by(messages: &str, files: &BTreeMap<String, File>) -> (BTreeSet<usize>, Vec<String>) {
    let mut refused = BTreeSet::new();
    let mut unowned = Vec::new();
    for line in messages.lines() {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if message["reason"] != "compiler-message" || message["message"]["level"] != "error" {
            continue;
        }
        let spans = message["message"]["spans"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if spans.is_empty() {
            continue;
        }
        let owner = spans
            .iter()
            .filter(|span| span["is_primary"] == true)
            .chain(spans.iter())
            .find_map(|span| owner(span, files));
        match owner {
            Some(index) => {
                refused.insert(index);
            }
            None => unowned.push(
                message["message"]["rendered"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
        }
    }
    (refused, unowned)
}

fn owner(span: &serde_json::Value, files: &BTreeMap<String, File>) -> Option<usize> {
    let file_name = span["file_name"].as_str().unwrap_or_default();
    let line = span["line_start"].as_u64().unwrap_or_default() as usize;
    let found = files.iter().find_map(|(name, file)| {
        if !file_name.ends_with(&format!("src/{name}.rs")) {
            return None;
        }
        file.lines
            .iter()
            .find(|(_, range)| range.contains(&line))
            .map(|(index, _)| *index)
    });
    found.or_else(|| {
        let expansion = &span["expansion"]["span"];
        (!expansion.is_null())
            .then(|| owner(expansion, files))
            .flatten()
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Refused,
    Reads,
    Trusted(&'static str),
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Outcome::Refused => f.write_str("refused"),
            Outcome::Reads => f.write_str("reads"),
            Outcome::Trusted(rule) => write!(f, "trusted {rule}"),
        }
    }
}

/// Compile every case, then read the documents into each one that builds.
/// A case that compiles and fails to read under no rule is returned as an
/// error with what it failed on.
fn observe(cases: &[Case]) -> Vec<Result<Outcome, String>> {
    let project = Project::new();
    let mut alive: Vec<Vec<usize>> = (0..cases.len())
        .collect::<Vec<_>>()
        .chunks(CHUNK)
        .map(<[usize]>::to_vec)
        .collect();
    let mut refused = BTreeSet::new();
    let mut files = BTreeMap::new();
    for round in 0.. {
        assert!(
            round < 10,
            "the generated cases never stopped failing to build"
        );
        files.clear();
        for entry in std::fs::read_dir(project.dir.join("src")).unwrap() {
            std::fs::remove_file(entry.unwrap().path()).unwrap();
        }
        for (chunk, members) in alive.iter().enumerate().filter(|(_, m)| !m.is_empty()) {
            let name = format!("flusso_boundary_{chunk}");
            let generated = file(cases, members);
            std::fs::write(
                project.dir.join(format!("src/{name}.rs")),
                &generated.source,
            )
            .unwrap();
            files.insert(name, generated);
        }
        let bins: Vec<String> = files.keys().cloned().collect();
        std::fs::write(project.dir.join("Cargo.toml"), project.manifest(&bins)).unwrap();
        let (built, messages) = project.build(&bins);
        let (now_refused, unowned) = refused_by(&messages, &files);
        if built {
            break;
        }
        assert!(
            !now_refused.is_empty(),
            "the generated cases failed to build with errors no case owns:\n{}",
            unowned.join("\n")
        );
        for members in &mut alive {
            members.retain(|index| !now_refused.contains(index));
        }
        refused.extend(now_refused);
    }
    let mut failures = BTreeMap::new();
    for name in files.keys() {
        let output = Command::new(project.target.join("debug").join(name))
            .output()
            .unwrap();
        assert!(output.status.success(), "{name} did not run to the end");
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let (index, error) = line.split_once('\t').unwrap();
            failures.insert(index.parse::<usize>().unwrap(), error.to_string());
        }
    }
    cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            if refused.contains(&index) {
                return Ok(Outcome::Refused);
            }
            let Some(error) = failures.get(&index) else {
                return Ok(Outcome::Reads);
            };
            TRUSTED
                .iter()
                .find(|trust| (trust.applies)(case))
                .map(|trust| Outcome::Trusted(trust.rule))
                .ok_or_else(|| error.clone())
        })
        .collect()
}

const OUTCOMES: &str = "tests/boundary.outcomes";

#[test]
fn every_shape_is_refused_reads_or_is_trusted() {
    let cases = cases();
    let started = std::time::Instant::now();
    let observed = observe(&cases);
    println!(
        "compiled and read {} cases in {:?}",
        cases.len(),
        started.elapsed()
    );

    let untrusted: Vec<String> = cases
        .iter()
        .zip(&observed)
        .filter_map(|(case, outcome)| {
            outcome
                .as_ref()
                .err()
                .map(|error| format!("{}: compiles, then fails to read: {error}", case.id))
        })
        .collect();
    assert!(
        untrusted.is_empty(),
        "these cases compile and fail to read a document, and no rule of what the derive trusts \
         covers them — refuse them in the derive, or add a rule to `TRUSTED` and to the manual's \
         list:\n{}",
        untrusted.join("\n")
    );

    let mut rendered = String::from(
        "# What the derive does with each generated case (tests/boundary.rs).\n\
         # Re-record: FLUSSO_BLESS=1 cargo nextest run -p flusso-query-derive --test boundary\n",
    );
    for (case, outcome) in cases.iter().zip(&observed) {
        writeln!(rendered, "{} {}", case.id, outcome.as_ref().unwrap()).unwrap();
    }
    let path = Path::new(MANIFEST_DIR).join(OUTCOMES);
    if std::env::var_os("FLUSSO_BLESS").is_some() {
        std::fs::write(&path, &rendered).unwrap();
        return;
    }
    let recorded = std::fs::read_to_string(&path).unwrap_or_default();
    let recorded: BTreeMap<&str, &str> = recorded
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once(' '))
        .collect();
    let changed: Vec<String> = rendered
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| line.split_once(' '))
        .filter(|(id, outcome)| recorded.get(id) != Some(outcome))
        .map(|(id, outcome)| {
            format!(
                "{id}: recorded `{}`, now `{outcome}`",
                recorded.get(id).copied().unwrap_or("nothing")
            )
        })
        .collect();
    assert!(
        changed.is_empty() && recorded.len() == cases.len(),
        "the derive's boundary moved — review, then re-record with FLUSSO_BLESS=1:\n{}",
        changed.join("\n")
    );
}

#[test]
fn every_trusted_rule_is_in_the_manual() {
    let manual = std::fs::read_to_string(
        Path::new(MANIFEST_DIR).join("../../docs/src/query/document-structs.md"),
    )
    .unwrap();
    for trust in TRUSTED {
        assert!(
            manual.contains(trust.manual),
            "the manual's list of what the derive trusts lacks rule `{}`: {}",
            trust.rule,
            trust.manual
        );
    }
}
