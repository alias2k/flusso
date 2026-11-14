//! The derive's boundary, pinned (issue #294): every type shape × field kind
//! × position, each compiled and read.
//!
//! Two shape lists are placed at each field of the `boundary` fixture index
//! ([`KINDS`]: one of each kind, required ones bare, nullable ones in
//! `Option<…>`), on a root and in a fragment:
//!
//! - [`named_shapes`], written by hand: the shapes issue #294 and its reviews
//!   named, field attributes (`opaque`, serde's), and the map-like shapes also
//!   `#[serde(flatten)]`ed into a root;
//! - [`generated_shapes`], an independent cross product: an inner type ×
//!   a wrapper around it × a newtype derive over that. A generated shape whose
//!   declarations alone are refused is refused everywhere, so it is compiled
//!   once, not placed.
//!
//! Each case is one module in a generated crate, compiled with cargo, then read
//! against sample documents of the index ([`DOCS`]: every field set, empty
//! arrays and maps, `null`, missing keys, one and many elements). It ends up in
//! one of three outcomes:
//!
//! - `refused`: the derive turns it down at compile time;
//! - `reads`: it compiles, and reads every sample document;
//! - `trusted <rule>`: it compiles, and a sample document fails to read —
//!   allowed only under a rule of [`TRUSTED`], each one line of the manual's
//!   list of what the derive trusts (`docs/src/query/document-structs.md`).
//!
//! A case that compiles and fails to read under no rule fails the test, and so
//! does a shape whose root and fragment placements at one field disagree,
//! unless [`DISAGREE`] lists the pair. The outcomes are recorded in
//! `tests/boundary.outcomes`, so a change to what the derive refuses or reads
//! shows up there as a diff to review; re-record with
//! `FLUSSO_BLESS=1 cargo nextest run -p flusso-query-derive --test boundary`.
//!
//! The generated crate shares trybuild's target directory and dependency set,
//! so it reuses what the `ui` harness built. Cases go in files of at most
//! [`CHUNK`] modules, compiled three at a time: generated derives are heavy
//! on `rustc`'s memory. A file's refused cases can hide the next phase's
//! errors in the same file, so refused cases are dropped and the rest are
//! compiled again until everything left builds; a file is rewritten only when
//! its content changes, so only those files recompile.
//!
//! `Arc<…>` is in no list: serde reads it only with its `rc` feature, which
//! the workspace does not enable, so it never compiles.
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

/// A Rust type the generator places: the declarations it needs, the type as a
/// field writes it, and attributes on that field.
#[derive(Clone)]
struct Shape {
    name: String,
    decls: Vec<String>,
    ty: String,
    attrs: &'static str,
    /// Reads one JSON object of dynamic keys: placed `#[serde(flatten)]` too,
    /// where the `flattened` rule covers it.
    map: bool,
    /// A tagged `FlussoValue` newtype over a type the derive can't see into.
    unseen: bool,
    /// Reads one string (a `String`, or a newtype over one).
    string: bool,
}

fn shape(name: &str, ty: &str, decls: &[&str]) -> Shape {
    Shape {
        name: name.to_string(),
        decls: decls.iter().map(ToString::to_string).collect(),
        ty: ty.to_string(),
        attrs: "",
        map: false,
        unseen: false,
        string: false,
    }
}

impl Shape {
    fn map(mut self) -> Shape {
        self.map = true;
        self
    }

    fn unseen(mut self) -> Shape {
        self.unseen = true;
        self
    }

    fn string(mut self) -> Shape {
        self.string = true;
        self
    }

    fn attrs(mut self, attrs: &'static str) -> Shape {
        self.attrs = attrs;
        self
    }
}

const SUB: &str =
    "#[derive(Debug, Serialize, Deserialize, FlussoFragment)] pub struct Sub { pub k: String }";
const EN: &str = "#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] \
     #[serde(rename_all = \"lowercase\")] pub enum En { A, B }";
const CODE: &str =
    "#[derive(Debug, Serialize, Deserialize, FlussoValue)] pub struct Code(pub String);";
const MAP_KW: &str = "#[derive(Debug, Serialize, Deserialize, FlussoMap)] #[flusso(keyword)] \
     pub struct MapKw(pub HashMap<String, String>);";
const FRAGMENT_OVER_MAP: &str =
    "#[derive(Debug, Deserialize, FlussoFragment)] pub struct Fm(pub HashMap<String, String>);";
const PLAIN: &str = "#[derive(Debug, Deserialize)] pub struct Plain { pub zz: String }";
const ALIAS: &str = "pub type Ms = HashMap<String, String>;";

/// The shapes written by hand.
fn named_shapes() -> Vec<Shape> {
    vec![
        shape("string", "String", &[]).string(),
        shape("long", "i64", &[]),
        shape("vec_string", "Vec<String>", &[]),
        shape("code", "Code", &[CODE]).string(),
        shape(
            "tags",
            "Tags",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] pub struct Tags(pub Vec<String>);"],
        ),
        shape("enum", "En", &[EN]),
        shape("value", "serde_json::Value", &[]).map(),
        shape("geo_point", "flusso_query::GeoPoint", &[]),
        shape("sub", "Sub", &[SUB]),
        shape("vec_sub", "Vec<Sub>", &[SUB]),
        shape(
            "subs",
            "Subs",
            &[SUB, "#[derive(Debug, Deserialize, FlussoFragment)] pub struct Subs(pub Vec<Sub>);"],
        ),
        shape("hash_map", "HashMap<String, String>", &[]).map(),
        shape("btree_map_long", "BTreeMap<String, i64>", &[]).map(),
        shape("hash_map_enum", "HashMap<String, En>", &[EN]).map(),
        shape("hash_map_long_key", "HashMap<i64, String>", &[]).map(),
        shape("fragment_over_map", "Fm", &[FRAGMENT_OVER_MAP]).map(),
        shape("vec_fragment_over_map", "Vec<Fm>", &[FRAGMENT_OVER_MAP]),
        shape(
            "fragment_over_option_map",
            "Fo",
            &["#[derive(Debug, Deserialize, FlussoFragment)] pub struct Fo(pub Option<HashMap<String, String>>);"],
        )
        .map(),
        shape(
            "fragment_over_value_map",
            "Fv",
            &["#[derive(Debug, Deserialize, FlussoFragment)] pub struct Fv(pub HashMap<String, serde_json::Value>);"],
        )
        .map(),
        shape(
            "fragment_over_long_key_map",
            "Fk",
            &["#[derive(Debug, Deserialize, FlussoFragment)] pub struct Fk(pub HashMap<i64, String>);"],
        )
        .map(),
        shape("map_keyword", "MapKw", &[MAP_KW]).map(),
        shape(
            "map_text",
            "MapTx",
            &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(text)] pub struct MapTx(pub HashMap<String, String>);"],
        )
        .map(),
        shape(
            "map_keyword_of_long",
            "MapKl",
            &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct MapKl(pub HashMap<String, i64>);"],
        )
        .map(),
        shape(
            "map_keyword_long_key",
            "MapLk",
            &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct MapLk(pub HashMap<i64, String>);"],
        )
        .map(),
        shape(
            "map_named",
            "Named",
            &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct Named { \
               pub x: Option<String>, #[serde(flatten)] pub rest: HashMap<String, String> }"],
        )
        .map(),
        shape(
            "map_named_required",
            "NamedReq",
            &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct NamedReq { pub x: String }"],
        )
        .map(),
        shape(
            "map_named_alias",
            "NamedAlias",
            &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct NamedAlias { \
               #[serde(alias = \"y\")] pub x: Option<String> }"],
        )
        .map(),
        shape(
            "map_over_box",
            "Boxed",
            &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct Boxed(pub Box<String>);"],
        )
        .map(),
        shape(
            "map_over_alias",
            "MapAl",
            &[ALIAS, "#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct MapAl(pub Ms);"],
        )
        .map(),
        shape(
            "map_over_map_type",
            "Mm",
            &[MAP_KW, "#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct Mm(pub MapKw);"],
        )
        .map(),
        shape(
            "value_over_map",
            "Vm",
            &[MAP_KW, "#[derive(Debug, Serialize, Deserialize, FlussoValue)] pub struct Vm(pub MapKw);"],
        )
        .map(),
        shape(
            "fragment_over_map_type",
            "Fmt",
            &[MAP_KW, "#[derive(Debug, Deserialize, FlussoFragment)] pub struct Fmt(pub MapKw);"],
        )
        .map(),
        shape(
            "tagged_over_map_type",
            "Tm",
            &[MAP_KW, "#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tm(pub MapKw);"],
        ),
        shape(
            "tagged_over_hash_map",
            "Th",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Th(pub HashMap<String, String>);"],
        ),
        shape(
            "tagged_over_boxed_map",
            "Tb",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tb(pub Box<HashMap<String, String>>);"],
        ),
        shape(
            "tagged_over_long",
            "Tl",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tl(pub i64);"],
        ),
        shape(
            "tagged_over_alias",
            "Ta",
            &[ALIAS, "#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Ta(pub Ms);"],
        )
        .unseen(),
        shape(
            "tagged_over_plain_map_newtype",
            "Tp",
            &[
                "#[derive(Debug, Serialize, Deserialize)] pub struct Pm(pub HashMap<String, String>);",
                "#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tp(pub Pm);",
            ],
        )
        .unseen(),
        shape("opaque_plain", "Plain", &[PLAIN]).attrs("#[flusso(opaque)]"),
        shape(
            "deserialize_with",
            "String",
            &["pub fn number_as_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> { \
               i64::deserialize(d).map(|n| n.to_string()) }"],
        )
        .attrs("#[serde(deserialize_with = \"number_as_string\")]"),
        shape(
            "try_from",
            "Long",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] \
               #[serde(try_from = \"String\")] pub struct Long(pub String);",
              "impl TryFrom<String> for Long { type Error = String; fn try_from(s: String) -> Result<Self, String> { \
               if s.len() > 1 { Ok(Long(s)) } else { Err(format!(\"too short: {s}\")) } } }"],
        ),
        shape(
            "deny_unknown_fields",
            "Strict",
            &["#[derive(Debug, Deserialize, FlussoFragment)] #[serde(deny_unknown_fields)] pub struct Strict { pub k: String }"],
        ),
    ]
}

/// The inner types of [`generated_shapes`]: name, type, declarations, and
/// whether it reads a map.
const INNERS: &[(&str, &str, &[&str], bool)] = &[
    ("s", "String", &[], false),
    ("l", "i64", &[], false),
    ("cow", "Cow<'static, str>", &[], false),
    ("code", "Code", &[CODE], false),
    ("sub", "Sub", &[SUB], false),
    ("hm", "HashMap<String, String>", &[], true),
    ("hml", "HashMap<String, i64>", &[], true),
    ("hmk", "HashMap<i64, String>", &[], true),
    ("btv", "BTreeMap<String, serde_json::Value>", &[], true),
    ("alias", "Ms", &[ALIAS], true),
    ("mk", "MapKw", &[MAP_KW], true),
];

/// The wrappers of [`generated_shapes`], around the inner type.
const WRAPPERS: &[(&str, &str)] = &[
    ("", "{}"),
    ("opt_", "Option<{}>"),
    ("vec_", "Vec<{}>"),
    ("box_", "Box<{}>"),
];

/// The derives of [`generated_shapes`], over the wrapped type `{}`: the field
/// itself, or a newtype (of a newtype) over it.
const DERIVES: &[(&str, &[&str])] = &[
    ("field", &[]),
    (
        "value",
        &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] pub struct N(pub {});"],
    ),
    (
        "keyword_value",
        &[
            "#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct N(pub {});",
        ],
    ),
    (
        "fragment",
        &["#[derive(Debug, Deserialize, FlussoFragment)] pub struct N(pub {});"],
    ),
    (
        "map",
        &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct N(pub {});"],
    ),
    (
        "value_of_value",
        &[
            "#[derive(Debug, Serialize, Deserialize, FlussoValue)] pub struct N0(pub {});",
            "#[derive(Debug, Serialize, Deserialize, FlussoValue)] pub struct N(pub N0);",
        ],
    ),
    (
        "fragment_of_fragment",
        &[
            "#[derive(Debug, Deserialize, FlussoFragment)] pub struct N0(pub {});",
            "#[derive(Debug, Deserialize, FlussoFragment)] pub struct N(pub N0);",
        ],
    ),
];

/// The second, independent shape list: every inner type × wrapper × derive.
fn generated_shapes() -> Vec<Shape> {
    let mut shapes = Vec::new();
    for &(inner, inner_ty, inner_decls, map) in INNERS {
        for &(wrap, wrapper) in WRAPPERS {
            let wrapped = wrapper.replace("{}", inner_ty);
            for &(derive, derive_decls) in DERIVES {
                let mut decls: Vec<String> = inner_decls.iter().map(ToString::to_string).collect();
                decls.extend(derive_decls.iter().map(|decl| decl.replace("{}", &wrapped)));
                let ty = if derive_decls.is_empty() {
                    wrapped.clone()
                } else {
                    "N".to_string()
                };
                let reads_a_map = map && matches!(wrap, "" | "box_") && derive != "keyword_value";
                shapes.push(Shape {
                    name: format!("{derive}_{wrap}{inner}"),
                    decls,
                    ty,
                    attrs: "",
                    map: reads_a_map,
                    unseen: derive == "keyword_value"
                        && inner == "alias"
                        && matches!(wrap, "" | "opt_" | "box_"),
                    string: matches!(inner, "s" | "cow" | "code")
                        && matches!(wrap, "" | "opt_" | "box_")
                        && derive != "map",
                });
            }
        }
    }
    shapes
}

/// A field of the `boundary` fixture index, and whether it is nullable.
const KINDS: &[(&str, bool)] = &[
    ("kw", false),
    ("n", false),
    ("kwa", false),
    ("ob", false),
    ("ne", false),
    ("mp", false),
    ("mpn", false),
    ("js", false),
    ("jsa", false),
    ("gp", false),
    ("kwo", true),
    ("nn", true),
    ("kwao", true),
    ("ho", true),
    ("mpo", true),
    ("jso", true),
];

/// Sample documents of the `boundary` index, as the sink writes them: every
/// field set with many elements; every array and map empty and every nullable
/// field `null`; nullable keys missing, with values a plain `keyword` takes and
/// an enum of `a`/`b` does not; one element each.
const DOCS: &[&str] = &[
    r#"{"id":"1","kw":"a","n":1,"kwa":["a","b"],"ob":{"k":"a","n":1},"ne":[{"k":"a","n":1},{"k":"b","n":2}],"mp":{"x":"a","y":"b"},"mpn":{"x":1,"y":2},"js":{"any":[1]},"jsa":[1,"x"],"gp":{"lat":1.5,"lon":2.5},"kwo":"a","nn":1,"kwao":["a","b"],"ho":{"k":"a","n":1},"mpo":{"x":"a","y":"b"},"jso":{"a":1}}"#,
    r#"{"id":"2","kw":"b","n":2,"kwa":[],"ob":{"k":"b","n":2},"ne":[],"mp":{},"mpn":{},"js":{},"jsa":[],"gp":{"lat":0.0,"lon":0.0},"kwo":null,"nn":null,"kwao":null,"ho":null,"mpo":null,"jso":null}"#,
    r#"{"id":"3","kw":"z","n":3,"kwa":["z"],"ob":{"k":"z","n":3},"ne":[{"k":"z","n":3}],"mp":{"x":"z"},"mpn":{"y":3},"js":"z","jsa":["z"],"gp":{"lat":-1.0,"lon":1.0}}"#,
    r#"{"id":"4","kw":"bb","n":4,"kwa":["a"],"ob":{"k":"a","n":4},"ne":[{"k":"a","n":4}],"mp":{"x":"a"},"mpn":{"x":4},"js":[1],"jsa":[{"a":1}],"gp":{"lat":2.0,"lon":3.0},"kwo":"b","nn":4,"kwao":[],"ho":{"k":"b","n":4},"mpo":{},"jso":[]}"#,
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
    shape: Shape,
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

const TRUSTED: &[Trust] =
    &[
        Trust {
            rule: "json",
            manual: "**Any type at a `json` field.**",
            applies: |case| matches!(case.kind, "js" | "jsa" | "jso"),
        },
        Trust {
            rule: "flattened",
            manual: "**A `#[serde(flatten)]` map.**",
            applies: |case| case.position == Position::Flattened && case.shape.map,
        },
        Trust {
            rule: "variants",
            manual: "**An enum where the schema declares no `variants:`.**",
            applies: |case| case.shape.decls.iter().any(|decl| decl == EN),
        },
        Trust {
            rule: "unseen",
            manual: "**A tagged `FlussoValue` newtype over a type the derive can't see into.**",
            applies: |case| case.shape.unseen,
        },
        Trust {
            rule: "opaque",
            manual: "**What [`#[flusso(opaque)]`](#escape-hatches) turns off**",
            applies: |case| case.shape.attrs.contains("opaque"),
        },
        Trust {
            rule: "serde",
            manual: "**serde attributes that change what is read**",
            applies: |case| {
                case.shape.attrs.contains("with")
                    || case.shape.decls.iter().any(|decl| {
                        decl.contains("try_from") || decl.contains("deny_unknown_fields")
                    })
            },
        },
        Trust {
            rule: "geo",
            manual: "**A `String` at a `geo` field.**",
            applies: |case| case.kind == "gp" && case.shape.string,
        },
    ];

/// The (shape, field) pairs whose root and fragment placements may disagree,
/// each with why. Every other pair records one outcome for both.
const DISAGREE: &[(&str, &str, &str)] = &[
    // A `type` alias of a map at a `map` field: a root checks it with a
    // `FlussoMap<K>` bound, which sees through the alias, and reads it; a
    // fragment reads a type's metadata in const evaluation, which a foreign
    // `HashMap` can't carry (a fragment walks every custom type with an
    // inherent `__flusso_check`), so it refuses. Both are safe.
    (
        "field_alias",
        "mp",
        "a root sees through a type alias, a fragment cannot",
    ),
    (
        "field_alias",
        "mpo",
        "a root sees through a type alias, a fragment cannot",
    ),
];

fn cases(shapes: &[Shape], prefix: &str, flattened: bool) -> Vec<Case> {
    let mut cases = Vec::new();
    for position in [Position::Root, Position::Fragment] {
        for shape in shapes {
            for &(kind, _) in KINDS {
                cases.push(Case {
                    id: format!("{prefix}{}/{}/{kind}", position.name(), shape.name),
                    shape: shape.clone(),
                    kind,
                    position,
                });
            }
        }
    }
    if flattened {
        for shape in shapes
            .iter()
            .filter(|shape| shape.map || shape.name == "sub")
        {
            cases.push(Case {
                id: format!("{prefix}flattened/{}", shape.name),
                shape: shape.clone(),
                kind: "",
                position: Position::Flattened,
            });
        }
    }
    cases
}

const HEADER: &str = "#![allow(unused_crate_dependencies, dead_code, unused_imports, non_snake_case, non_camel_case_types)]
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use flusso_query::{FlussoFragment, FlussoMap, FlussoRoot, FlussoValue};
use serde::{Deserialize, Serialize};
";

/// One unit of a generated file: a module body, and whether it has a root
/// to read documents into.
struct Unit {
    body: String,
    reads: bool,
}

/// A case as a module body, with a `read` that deserializes a document into
/// its root.
fn case_unit(case: &Case) -> Unit {
    let nullable = KINDS
        .iter()
        .any(|&(kind, nullable)| kind == case.kind && nullable);
    let ty = if nullable {
        format!("Option<{}>", case.shape.ty)
    } else {
        case.shape.ty.clone()
    };
    let (kind, attrs) = (case.kind, case.shape.attrs);
    let (fragment, root) = match case.position {
        Position::Root => (String::new(), format!("{attrs} pub {kind}: {ty}")),
        Position::Fragment => (
            format!(
                "#[derive(Debug, Deserialize, FlussoFragment)] pub struct F {{ {attrs} pub {kind}: {ty} }}"
            ),
            "#[serde(flatten)] pub f: F".to_string(),
        ),
        Position::Flattened => (
            String::new(),
            format!("pub kw: String, #[serde(flatten)] pub rest: {ty}"),
        ),
    };
    Unit {
        body: format!(
            "{}\n{fragment}\n\
             #[derive(Debug, Deserialize, FlussoRoot)] #[flusso(index = \"boundary\")] pub struct R {{ {root} }}\n\
             pub fn read(doc: &str) -> Result<(), String> {{ serde_json::from_str::<R>(doc).map(drop).map_err(|e| e.to_string()) }}\n",
            case.shape.decls.join("\n")
        ),
        reads: true,
    }
}

/// A generated file: its source, and the line range each unit owns.
struct File {
    source: String,
    lines: Vec<(usize, std::ops::Range<usize>)>,
}

fn file(units: &[Unit], members: &[usize]) -> File {
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
        writeln!(
            source,
            "mod c{index} {{\nuse super::*;\n{}}}",
            units[index].body
        )
        .unwrap();
        lines.push((index, start..source.lines().count() + 1));
    }
    source
        .push_str("fn main() {\n    let cases: &[(usize, fn(&str) -> Result<(), String>)] = &[\n");
    for &index in members.iter().filter(|&&index| units[index].reads) {
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
        write_if_changed(
            &dir.join("Cargo.lock"),
            &std::fs::read_to_string(workspace.join("Cargo.lock")).unwrap(),
        );
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

fn write_if_changed(path: &Path, content: &str) {
    if std::fs::read_to_string(path).ok().as_deref() != Some(content) {
        std::fs::write(path, content).unwrap();
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

/// The units each error of a build belongs to, by its primary span's line in
/// a generated file (following a macro expansion back to its call site). An
/// error no unit owns is returned as text.
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

/// Compile every unit (as bins named `{prefix}_{chunk}`), dropping refused
/// ones until the rest builds, then run what reads. Returns the refused units
/// and, per unit that builds, the first document it failed to read.
fn compile(
    project: &Project,
    prefix: &str,
    units: &[Unit],
) -> (BTreeSet<usize>, BTreeMap<usize, String>) {
    let mut alive: Vec<Vec<usize>> = (0..units.len())
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
        for (chunk, members) in alive.iter().enumerate() {
            let name = format!("{prefix}_{chunk}");
            let path = project.dir.join(format!("src/{name}.rs"));
            if members.is_empty() {
                let _ = std::fs::remove_file(path);
                continue;
            }
            let generated = file(units, members);
            write_if_changed(&path, &generated.source);
            files.insert(name, generated);
        }
        let bins: Vec<String> = files.keys().cloned().collect();
        write_if_changed(&project.dir.join("Cargo.toml"), &project.manifest(&bins));
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
    (refused, failures)
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

/// Compile and read every case. A case that compiles and fails to read under
/// no rule is an error with what it failed on.
fn observe(project: &Project, prefix: &str, cases: &[Case]) -> Vec<Result<Outcome, String>> {
    let units: Vec<Unit> = cases.iter().map(case_unit).collect();
    let (refused, failures) = compile(project, prefix, &units);
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

/// The generated shapes whose declarations alone the derives refuse.
fn refused_declarations(project: &Project, shapes: &[Shape]) -> BTreeSet<usize> {
    let units: Vec<Unit> = shapes
        .iter()
        .map(|shape| Unit {
            body: shape.decls.join("\n") + "\n",
            reads: false,
        })
        .collect();
    compile(project, "flusso_boundary_decls", &units).0
}

const OUTCOMES: &str = "tests/boundary.outcomes";

#[test]
fn every_shape_is_refused_reads_or_is_trusted() {
    let started = std::time::Instant::now();
    let project = Project::new();
    let mut cases = cases(&named_shapes(), "", true);
    let generated = generated_shapes();
    let refused_shapes = refused_declarations(&project, &generated);
    let placed: Vec<Shape> = generated
        .iter()
        .enumerate()
        .filter(|(index, _)| !refused_shapes.contains(index))
        .map(|(_, shape)| shape.clone())
        .collect();
    cases.extend(self::cases(&placed, "gen/", false));
    let observed = observe(&project, "flusso_boundary", &cases);
    println!(
        "{} declarations, {} cases compiled and read in {:?}",
        generated.len(),
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
        "{} cases compile and fail to read a document, and no rule of what the derive trusts \
         covers them — refuse them in the derive, or add a rule to `TRUSTED` and to the manual's \
         list:\n{}",
        untrusted.len(),
        untrusted.join("\n")
    );

    let outcome_of: BTreeMap<&str, String> = cases
        .iter()
        .zip(&observed)
        .map(|(case, outcome)| (case.id.as_str(), outcome.as_ref().unwrap().to_string()))
        .collect();
    let mut disagree = Vec::new();
    for (id, outcome) in &outcome_of {
        let Some((prefix, rest)) = id.split_once("root/") else {
            continue;
        };
        let other = &outcome_of[format!("{prefix}fragment/{rest}").as_str()];
        let (shape, kind) = rest.rsplit_once('/').unwrap();
        let excepted = DISAGREE.iter().any(|&(s, k, _)| s == shape && k == kind);
        if outcome != other && !excepted {
            disagree.push(format!("{id}: root `{outcome}`, fragment `{other}`"));
        }
    }
    assert!(
        disagree.is_empty(),
        "root and fragment disagree on {} placements — make them agree, or list the pair in \
         `DISAGREE` with why:\n{}",
        disagree.len(),
        disagree.join("\n")
    );

    let mut rendered = String::from(
        "# What the derive does with each generated case (tests/boundary.rs).\n\
         # Re-record: FLUSSO_BLESS=1 cargo nextest run -p flusso-query-derive --test boundary\n",
    );
    for (index, shape) in generated.iter().enumerate() {
        if refused_shapes.contains(&index) {
            writeln!(rendered, "gen/{} refused", shape.name).unwrap();
        }
    }
    for (id, outcome) in &outcome_of {
        writeln!(rendered, "{id} {outcome}").unwrap();
    }
    let path = Path::new(MANIFEST_DIR).join(OUTCOMES);
    if std::env::var_os("FLUSSO_BLESS").is_some() {
        std::fs::write(&path, &rendered).unwrap();
        return;
    }
    let recorded = std::fs::read_to_string(&path).unwrap_or_default();
    let lines = |text: &str| -> BTreeMap<String, String> {
        text.lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once(' '))
            .map(|(id, outcome)| (id.to_string(), outcome.to_string()))
            .collect()
    };
    let (recorded, now) = (lines(&recorded), lines(&rendered));
    let ids: BTreeSet<&String> = recorded.keys().chain(now.keys()).collect();
    let changed: Vec<String> = ids
        .into_iter()
        .filter(|id| recorded.get(*id) != now.get(*id))
        .map(|id| {
            format!(
                "{id}: recorded `{}`, now `{}`",
                recorded.get(id).map_or("nothing", String::as_str),
                now.get(id).map_or("nothing", String::as_str)
            )
        })
        .collect();
    assert!(
        changed.is_empty(),
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
