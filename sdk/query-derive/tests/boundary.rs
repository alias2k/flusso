//! The derive's boundary, pinned (issue #294): type shapes × the field kinds
//! of one fixture × position, each compiled and read. It covers the kinds the
//! fixture holds, not every kind a schema can declare: the manual's "Known
//! gaps" (issue #306) lie outside it.
//!
//! Two shape lists are placed at each field of the `boundary` fixture index
//! ([`KINDS`]: one of each kind it holds, required ones bare, nullable ones in
//! `Option<…>`):
//!
//! - [`named_shapes`], written by hand: the shapes issue #294 and its reviews
//!   named, field attributes (`opaque`, serde's), and the map-like shapes also
//!   `#[serde(flatten)]`ed into a root;
//! - [`generated_shapes`], an independent cross product: an inner type ×
//!   a wrapper around it × a newtype derive over that.
//!
//! A shape whose declarations alone are refused is refused everywhere, so it
//! is compiled once and not placed. Every other shape is placed on a root,
//! then in a fragment, but only if it compiles at some root field: the
//! root/fragment parity check covers the shapes that compile somewhere.
//!
//! Each case is one module in a generated crate, compiled with cargo, then read
//! against sample documents of the index ([`DOCS`]: every field set, empty
//! arrays and maps, `null`, missing keys, one and many elements, a `null` map
//! value, each date form). It ends up in one of three outcomes:
//!
//! - `refused`: the derive turns it down at compile time;
//! - `reads`: it compiles, and reads every sample document;
//! - `trusted <rule>`: it compiles, and each document it fails to read is one
//!   a rule of [`TRUSTED`] describes, with the error it describes; each rule
//!   is one line of the manual's list (`docs/src/query/document-structs.md`).
//!
//! A case that compiles and fails to read under no rule fails the test, and so
//! does a shape whose root and fragment placements at one field disagree,
//! unless [`DISAGREE`] lists the pair. The outcomes are recorded in
//! `tests/boundary.outcomes`, so a change to what the derive refuses or reads
//! shows up there as a diff to review; re-record with
//! `FLUSSO_BLESS=1 cargo nextest run -p flusso-query-derive --test boundary`.
//!
//! `refused` doesn't say which check refused. Three refusals are backed up by
//! another check on every shape here, so removing one leaves this test green;
//! their `ui` cases pin them instead, and fail when it is removed:
//! `value_tagged_newtype_over_a_collection` (a tagged `FlussoValue` newtype
//! over a collection), `fragment_newtype_reads_another_array` (a
//! `FlussoFragment` newtype over a leaf) and `map_type_reads_no_object` (a
//! `FlussoMap` newtype over a `Vec`).
//!
//! It sees only what compiles and fails to read, never a refusal of a type
//! that would have read: the `ui_pass` cases (`map_keys_of_your_own`,
//! `tagged_newtypes_over_named_types`) guard against over-refusal.
//!
//! The generated crate lives in its own folder with its own target directory
//! (`target/tests/flusso-boundary/`), so the `ui` harness never waits on its
//! lock. Cases go in files of at most [`CHUNK`] modules, compiled three at a
//! time: generated derives are heavy on `rustc`'s memory. A file's refused
//! cases can hide the next phase's errors in the same file, so refused cases
//! are dropped and the rest are compiled again until everything left builds:
//! the first round with `cargo check`, later ones with `cargo build`. A file is
//! named after a hash of its text and of every input its result depends on
//! ([`Project::inputs`]), and its results are cached under that name, so a
//! rerun on an unchanged derive compiles nothing.
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
const CHUNK: usize = 60;

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
    /// A chrono date or time type, or a newtype over one.
    date: bool,
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
        date: false,
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

    fn date(mut self) -> Shape {
        self.date = true;
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
const EN_A: &str = "#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] \
     #[serde(rename_all = \"lowercase\")] pub enum EnA { A }";
const EXHAUSTIVE: &str = "#[derive(Debug, Serialize, Deserialize, FlussoValue)] \
     #[flusso(keyword, exhaustive)] #[serde(rename_all = \"lowercase\")] pub enum Ex { A, B }";

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
        shape("hash_map_option", "HashMap<String, Option<String>>", &[]).map(),
        shape("btree_map_option_long", "BTreeMap<String, Option<i64>>", &[]).map(),
        shape("hash_map_option_option", "HashMap<String, Option<Option<String>>>", &[]).map(),
        shape(
            "map_keyword_option",
            "MapKo",
            &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct MapKo(pub HashMap<String, Option<String>>);"],
        )
        .map(),
        shape(
            "map_named_option_rest",
            "NamedOpt",
            &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] pub struct NamedOpt { \
               pub x: Option<String>, #[serde(flatten)] pub rest: BTreeMap<String, Option<String>> }"],
        )
        .map(),
        shape(
            "map_named_deny",
            "NamedDeny",
            &["#[derive(Debug, Deserialize, FlussoMap)] #[flusso(keyword)] #[serde(deny_unknown_fields)] \
               pub struct NamedDeny { pub x: Option<String> }"],
        )
        .map(),
        shape(
            "fragment_over_option_value_map",
            "Fov",
            &["#[derive(Debug, Deserialize, FlussoFragment)] pub struct Fov(pub HashMap<String, Option<String>>);"],
        )
        .map(),
        shape("exhaustive", "Ex", &[EXHAUSTIVE]),
        shape("hash_map_exhaustive", "HashMap<String, Ex>", &[EXHAUSTIVE]).map(),
        shape("hash_map_box_str", "HashMap<String, Box<str>>", &[]).map(),
        shape("hash_map_char", "HashMap<String, char>", &[]).map(),
        shape("hash_map_vec", "HashMap<String, Vec<String>>", &[]).map(),
        shape("hash_map_map", "HashMap<String, HashMap<String, String>>", &[]).map(),
        shape("hash_map_code", "HashMap<String, Code>", &[CODE]).map(),
        shape("hash_map_sub", "HashMap<String, Sub>", &[SUB]).map(),
        shape(
            "hash_map_custom_hasher",
            "HashMap<String, String, std::hash::BuildHasherDefault<std::collections::hash_map::DefaultHasher>>",
            &[],
        )
        .map(),
        shape("char", "char", &[]),
        shape("u128", "u128", &[]),
        shape("f32", "f32", &[]),
        shape("box_str", "Box<str>", &[]),
        shape("option_box_vec", "Option<Box<Vec<String>>>", &[]),
        shape("vec_option", "Vec<Option<String>>", &[]),
        shape("naive_date", "chrono::NaiveDate", &[]).date(),
        shape(
            "tagged_over_naive_date",
            "Tnd",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tnd(pub chrono::NaiveDate);"],
        )
        .unseen(),
        shape(
            "value_over_naive_date",
            "Vnd",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] pub struct Vnd(pub chrono::NaiveDate);"],
        )
        .date(),
        shape("enum_partial", "EnA", &[EN_A]),
        shape("vec_enum_partial", "Vec<EnA>", &[EN_A]),
        shape(
            "tagged_over_decimal",
            "Tdc",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tdc(pub flusso_query::Decimal);"],
        )
        .unseen(),
        shape("string_alias", "Sa", &["pub type Sa = String;"]),
        shape("naive_date_time", "chrono::NaiveDateTime", &[]).date(),
        shape("naive_time", "chrono::NaiveTime", &[]).date(),
        shape("date_time_utc", "chrono::DateTime<chrono::Utc>", &[]).date(),
        shape(
            "tagged_over_non_zero",
            "Tnz",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tnz(pub std::num::NonZeroU32);"],
        ),
        shape(
            "tagged_over_wrapping",
            "Tw",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tw(pub std::num::Wrapping<i64>);"],
        ),
        shape(
            "tagged_over_reverse",
            "Trv",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Trv(pub std::cmp::Reverse<i64>);"],
        ),
        shape(
            "tagged_over_cell",
            "Tc",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tc(pub std::cell::Cell<i64>);"],
        ),
        shape(
            "tagged_over_ref_cell_map",
            "Trc",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Trc(pub std::cell::RefCell<HashMap<String, String>>);"],
        ),
        shape(
            "tagged_over_mutex_map",
            "Tmx",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tmx(pub std::sync::Mutex<HashMap<String, String>>);"],
        ),
        shape(
            "tagged_over_cow_slice",
            "Tcs",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tcs(pub Cow<'static, [String]>);"],
        ),
        shape(
            "tagged_over_duration",
            "Td",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Td(pub std::time::Duration);"],
        )
        .unseen(),
        shape(
            "tagged_over_geo_point",
            "Tg",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tg(pub flusso_query::GeoPoint);"],
        )
        .unseen(),
        shape(
            "tagged_over_char",
            "Tch",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tch(pub char);"],
        )
        .unseen(),
        shape(
            "tagged_over_cow_str",
            "Tcw",
            &["#[derive(Debug, Serialize, Deserialize, FlussoValue)] #[flusso(keyword)] pub struct Tcw(pub Cow<'static, str>);"],
        )
        .unseen()
        .string(),

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
    ("ch", "char", &[], false),
    ("f", "f32", &[], false),
    ("bs", "Box<str>", &[], false),
    ("hmo", "HashMap<String, Option<String>>", &[], true),
    ("hmv", "HashMap<String, Vec<String>>", &[], true),
    ("hmm", "HashMap<String, HashMap<String, String>>", &[], true),
    ("hme", "BTreeMap<String, En>", &[EN], true),
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
                        && matches!(inner, "alias" | "ch")
                        && matches!(wrap, "" | "opt_" | "box_"),
                    date: false,
                    string: matches!(inner, "s" | "cow" | "code" | "bs")
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
    ("dt", false),
    ("ts", false),
    ("cm", false),
    ("co", false),
    ("ev", false),
];

/// Sample documents of the `boundary` index, as the sink writes them (see
/// `sink`'s `json::tests` for the date forms): every field set with many
/// elements; every array and map empty and every nullable field `null`;
/// nullable keys missing, with values a plain `keyword` takes and an enum of
/// `a`/`b` does not; one element each; and [`NULL_DOC`], a `null` under a map
/// key. A `timestamp` field (`ts`) holds whatever its column is: a
/// `timestamptz`, a naive `timestamp` or a `time`.
const DOCS: &[&str] = &[
    r#"{"id":"1","kw":"a","n":1,"kwa":["a","b"],"ob":{"k":"a","n":1},"ne":[{"k":"a","n":1},{"k":"b","n":2}],"mp":{"x":"a","y":"b"},"mpn":{"x":1,"y":2},"js":{"any":[1]},"jsa":[1,"x"],"gp":{"lat":1.5,"lon":2.5},"kwo":"a","nn":1,"kwao":["a","b"],"ho":{"k":"a","n":1},"mpo":{"x":"a","y":"b"},"jso":{"a":1},"dt":"2024-01-02","ts":"2024-01-02T03:04:05.123456+00:00","cm":[1,"x"],"co":{"a":1},"ev":"a"}"#,
    r#"{"id":"2","kw":"b","n":2,"kwa":[],"ob":{"k":"b","n":2},"ne":[],"mp":{},"mpn":{},"js":{},"jsa":[],"gp":{"lat":0.0,"lon":0.0},"kwo":null,"nn":null,"kwao":null,"ho":null,"mpo":null,"jso":null,"dt":"2024-01-03","ts":"2024-01-03T00:00:00+00:00","cm":[1,"x"],"co":{"a":1},"ev":"b"}"#,
    r#"{"id":"3","kw":"z","n":3,"kwa":["z"],"ob":{"k":"z","n":3},"ne":[{"k":"z","n":3}],"mp":{"x":"z"},"mpn":{"y":3},"js":"z","jsa":["z"],"gp":{"lat":-1.0,"lon":1.0},"dt":"2024-01-04","ts":"2024-01-02T03:04:05.123456","cm":[1,"x"],"co":{"a":1},"ev":"a"}"#,
    r#"{"id":"4","kw":"bb","n":4,"kwa":["a"],"ob":{"k":"a","n":4},"ne":[{"k":"a","n":4}],"mp":{"x":"a"},"mpn":{"x":4},"js":[1],"jsa":[{"a":1}],"gp":{"lat":2.0,"lon":3.0},"kwo":"b","nn":4,"kwao":[],"ho":{"k":"b","n":4},"mpo":{},"jso":[],"dt":"2024-01-05","ts":"03:04:05.123456","cm":[1,"x"],"co":{"a":1},"ev":"b"}"#,
    r#"{"id":"5","kw":"a","n":5,"kwa":["a"],"ob":{"k":"a","n":5},"ne":[{"k":"a","n":5}],"mp":{"x":null,"y":"a"},"mpn":{"x":null},"js":{},"jsa":[],"gp":{"lat":1.0,"lon":1.0},"kwo":"a","nn":5,"kwao":["a"],"ho":{"k":"a","n":5},"mpo":{"x":null},"jso":{},"dt":"2024-01-06","ts":"2024-01-06T00:00:00+00:00","cm":[1,"x"],"co":{"a":1},"ev":"a"}"#,
];

/// The document whose only difference is a `null` under a map key.
const NULL_DOC: usize = 4;

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
    /// Whether the rule covers this case failing on document `doc` (an
    /// index into [`DOCS`]) with `error`: a rule is tied to the failure it
    /// describes, not only to the shape.
    applies: fn(&Case, usize, &str) -> bool,
}

const TRUSTED: &[Trust] = &[
    Trust {
        rule: "json",
        manual: "**Any type at a `json` field, or at a `constant` stored unindexed.**",
        applies: |case, _, _| matches!(case.kind, "js" | "jsa" | "jso" | "cm" | "co"),
    },
    Trust {
        rule: "flattened",
        manual: "**A `#[serde(flatten)]` map.**",
        applies: |case, _, _| case.position == Position::Flattened && case.shape.map,
    },
    Trust {
        rule: "variants",
        manual: "**An enum where the schema declares no `variants:`.**",
        applies: |case, _, error| {
            case.shape.decls.iter().any(|decl| decl == EN) && error.contains("unknown variant")
        },
    },
    Trust {
        rule: "null",
        manual: "**A map value that is not an `Option`.**",
        // serde says `null` for most types, and "expected value" for an
        // enum, which reads `null` as no variant at all.
        applies: |_, doc, error| {
            doc == NULL_DOC && (error.contains("null") || error.starts_with("expected value"))
        },
    },
    Trust {
        rule: "partial",
        manual: "**An enum without `exhaustive` where the schema declares `variants:`.**",
        applies: |case, _, error| {
            case.shape.decls.iter().any(|decl| decl == EN_A) && error.contains("unknown variant")
        },
    },
    Trust {
        rule: "dates",
        manual: "**A date type at a `date` or `timestamp` field.**",
        applies: |case, _, _| matches!(case.kind, "dt" | "ts") && case.shape.date,
    },
    Trust {
        rule: "unseen",
        manual: "**Any other type behind a tagged `FlussoValue` newtype.**",
        applies: |case, _, _| case.shape.unseen,
    },
    Trust {
        rule: "opaque",
        manual: "**What [`#[flusso(opaque)]`](#escape-hatches) turns off**",
        applies: |case, _, _| case.shape.attrs.contains("opaque"),
    },
    Trust {
        rule: "serde",
        manual: "**serde attributes that change what is read**",
        applies: |case, _, _| {
            case.shape.attrs.contains("with")
                || case.shape.decls.iter().any(|decl| {
                    decl.contains("try_from")
                        || (decl.contains("deny_unknown_fields") && decl.contains("FlussoFragment"))
                })
        },
    },
    Trust {
        rule: "geo",
        manual: "**A `String` at a `geo` field.**",
        applies: |case, _, _| case.kind == "gp" && case.shape.string,
    },
];

/// The (shape, field) pairs whose root and fragment placements may disagree,
/// each with why. Every other pair records one outcome for both.
const DISAGREE: &[(&str, &str, &str)] = &[
    // A `type` alias at a field its target fits: a root checks the field with
    // a trait bound (`FlussoMap<K>`, `FlussoValue<K>`), which sees through the
    // alias, and reads it; a fragment walks every custom type with an inherent
    // `__flusso_check`, which a foreign type (`HashMap`, `String`) can't have,
    // so it refuses. Both are safe. `*` is every field.
    (
        "field_alias",
        "*",
        "a root sees through a type alias, a fragment cannot",
    ),
    (
        "string_alias",
        "*",
        "a root sees through a type alias, a fragment cannot",
    ),
];

fn cases(shapes: &[Shape], prefix: &str, position: Position) -> Vec<Case> {
    let mut cases = Vec::new();
    if position != Position::Flattened {
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
    if position == Position::Flattened && prefix.is_empty() {
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
#[derive(Clone)]
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
         if let Err(error) = read(doc) {\n                println!(\"{index}\\t{}\\t{error}\", \
         DOCS.iter().position(|d| d == doc).unwrap());\n            }\n        }\n    }\n}\n",
    );
    File { source, lines }
}

/// The generated crate, in its own folder under the target directory, with a
/// target directory of its own (so the `ui` harness never waits on its
/// lock) and a cache of every file's results.
struct Project {
    dir: PathBuf,
    target: PathBuf,
    cache: PathBuf,
    /// A hash of every input a file's result depends on besides its own
    /// text: the derive and `flusso-query`, what they read (`config`,
    /// `kernel`), the fixtures, the lock file and the compiler.
    inputs: u128,
}

impl Project {
    fn new() -> Project {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .parent()
            .unwrap()
            .join("tests")
            .join("flusso-boundary");
        let (dir, target, cache) = (
            root.join("project"),
            root.join("target"),
            root.join("cache"),
        );
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(&cache).unwrap();
        let workspace = Path::new(MANIFEST_DIR)
            .join("../..")
            .canonicalize()
            .unwrap();
        let lock = std::fs::read_to_string(workspace.join("Cargo.lock")).unwrap();
        write_if_changed(&dir.join("Cargo.lock"), &lock);
        let mut inputs = Vec::new();
        for folder in [
            "sdk/query-derive/src",
            "sdk/query-derive/Cargo.toml",
            "sdk/query-derive/tests/fixtures",
            "sdk/query/src",
            "sdk/query/Cargo.toml",
            "libs/0-kernel/src",
            "libs/0-kernel/derive/src",
            "libs/1-config/src",
            "Cargo.lock",
            "Cargo.toml",
            "libs/0-kernel/Cargo.toml",
            "libs/0-kernel/derive/Cargo.toml",
            "libs/1-config/Cargo.toml",
            "rust-toolchain.toml",
        ] {
            read_tree(&workspace.join(folder), &mut inputs);
        }
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let rustc = Command::new(cargo)
            .arg("--version")
            .arg("--verbose")
            .output()
            .unwrap();
        inputs.extend(rustc.stdout);
        inputs.extend(project_manifest_inputs());
        Project {
            dir,
            target,
            cache,
            inputs: hash(&inputs),
        }
    }

    /// The manifest trybuild writes for this crate (its dependencies and
    /// dev-dependencies, the workspace's dependency table), plus `chrono` and
    /// one bin per generated file.
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
        // The date shapes name chrono's types; the derive sees them by name.
        let mut chrono = toml::Table::new();
        chrono.insert("workspace".into(), true.into());
        dependencies.insert("chrono".into(), chrono.into());
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
        // No debug info: the binaries only print what failed to read.
        let mut dev = toml::Table::new();
        dev.insert("debug".into(), false.into());
        let mut profile = toml::Table::new();
        profile.insert("dev".into(), dev.into());
        manifest.insert("profile".into(), profile.into());
        toml::to_string(&manifest).unwrap()
    }

    /// `cargo check` or `cargo build` of `bins`, with JSON diagnostics.
    fn cargo(&self, command: &str, bins: &[String]) -> (bool, String) {
        write_if_changed(&self.dir.join("Cargo.toml"), &self.manifest(bins));
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
                command,
                "--offline",
                "--quiet",
                "--keep-going",
                "--message-format=json",
                "-j3",
            ])
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

    fn cached(&self, key: &str, step: &str) -> Option<String> {
        std::fs::read_to_string(self.cache.join(format!("{key}.{step}"))).ok()
    }

    fn remember(&self, key: &str, step: &str, result: &str) {
        std::fs::write(self.cache.join(format!("{key}.{step}")), result).unwrap();
    }
}

/// Whatever of this test's own manifest decides the generated crate: its
/// dependencies.
fn project_manifest_inputs() -> Vec<u8> {
    std::fs::read(Path::new(MANIFEST_DIR).join("Cargo.toml")).unwrap()
}

/// Every file under `path` (or `path` itself), in a stable order, appended
/// with its relative name.
fn read_tree(path: &Path, into: &mut Vec<u8>) {
    if path.is_file() {
        into.extend(path.display().to_string().bytes());
        into.extend(std::fs::read(path).unwrap());
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.map(|entry| entry.unwrap().path()).collect();
    entries.sort();
    for entry in entries {
        read_tree(&entry, into);
    }
}

/// A 128-bit hash: two `DefaultHasher` runs, told apart by a prefix. Only
/// ever compared with hashes this same test binary made.
fn hash(bytes: &[u8]) -> u128 {
    use std::hash::{Hash, Hasher};
    let half = |seed: u8| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        seed.hash(&mut hasher);
        bytes.hash(&mut hasher);
        hasher.finish()
    };
    (u128::from(half(0)) << 64) | u128::from(half(1))
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

/// Compile every unit, dropping refused ones until the rest builds, then run
/// what reads. Returns the refused units and, per unit that builds, every
/// document it failed to read.
///
/// The refusal rounds run `cargo check` (the derive's checks are macro errors,
/// type errors and const evaluation, all of which `check` reports); only what
/// is left is built and run. A file is named after a hash of its text, and its
/// results are cached under that hash and [`Project::inputs`], so a file seen
/// before with unchanged inputs is never compiled again.
fn compile(
    project: &Project,
    units: &[Unit],
) -> (BTreeSet<usize>, BTreeMap<usize, Vec<(usize, String)>>) {
    let mut alive: Vec<Vec<usize>> = (0..units.len())
        .collect::<Vec<_>>()
        .chunks(CHUNK)
        .map(<[usize]>::to_vec)
        .collect();
    let mut refused = BTreeSet::new();
    for round in 0.. {
        assert!(
            round < 12,
            "the generated cases never stopped failing to build"
        );
        let files = files(project, units, &alive);
        let mut now_refused = BTreeSet::new();
        // The first round only checks: most cases are refused, and `check`
        // reports every refusal without code generation. Later rounds build,
        // which also reports any error a refusal in the same file hid.
        let steps: &[&str] = if round == 0 { &["check"] } else { &["build"] };
        for &step in steps {
            let missing: BTreeMap<String, File> = files
                .iter()
                .filter(|(key, _)| project.cached(key, step).is_none())
                .map(|(key, file)| (key.clone(), file.clone()))
                .collect();
            if !missing.is_empty() {
                for (key, file) in &missing {
                    write_if_changed(&project.dir.join(format!("src/{key}.rs")), &file.source);
                }
                let bins: Vec<String> = missing.keys().cloned().collect();
                let started = std::time::Instant::now();
                let (_, messages) = project.cargo(step, &bins);
                println!(
                    "round {round}: cargo {step} of {} files in {:?}",
                    bins.len(),
                    started.elapsed()
                );
                let (owned, unowned) = refused_by(&messages, &missing);
                assert!(
                    unowned.is_empty(),
                    "the generated cases failed to build with errors no case owns:\n{}",
                    unowned.join("\n")
                );
                for (key, file) in &missing {
                    let mine: Vec<String> = file
                        .lines
                        .iter()
                        .map(|(index, _)| *index)
                        .filter(|index| owned.contains(index))
                        .map(|index| index.to_string())
                        .collect();
                    project.remember(key, step, &mine.join("\n"));
                }
            }
            for key in files.keys() {
                let result = project.cached(key, step).unwrap();
                now_refused.extend(result.lines().map(|line| line.parse::<usize>().unwrap()));
            }
            if !now_refused.is_empty() {
                break;
            }
        }
        if now_refused.is_empty() && round > 0 {
            return (refused, run(project, &files));
        }
        for members in &mut alive {
            members.retain(|index| !now_refused.contains(index));
        }
        refused.extend(now_refused);
    }
    unreachable!()
}

/// The generated files of one round, keyed by the hash of their text and the
/// project's inputs, which is also the bin's name.
fn files(project: &Project, units: &[Unit], alive: &[Vec<usize>]) -> BTreeMap<String, File> {
    alive
        .iter()
        .filter(|members| !members.is_empty())
        .map(|members| {
            let file = file(units, members);
            let mut keyed = project.inputs.to_le_bytes().to_vec();
            keyed.extend(file.source.bytes());
            (format!("b{:032x}", hash(&keyed)), file)
        })
        .collect()
}

/// Run each built file's binary (or take its cached output): one line per
/// document a case failed to read.
fn run(project: &Project, files: &BTreeMap<String, File>) -> BTreeMap<usize, Vec<(usize, String)>> {
    let mut failures: BTreeMap<usize, Vec<(usize, String)>> = BTreeMap::new();
    for key in files.keys() {
        let output = project.cached(key, "run").unwrap_or_else(|| {
            let output = Command::new(project.target.join("debug").join(key))
                .output()
                .unwrap();
            assert!(output.status.success(), "{key} did not run to the end");
            let output = String::from_utf8_lossy(&output.stdout).into_owned();
            project.remember(key, "run", &output);
            output
        });
        for line in output.lines() {
            let mut parts = line.splitn(3, '\t');
            let (index, doc, error) = (
                parts.next().unwrap(),
                parts.next().unwrap(),
                parts.next().unwrap(),
            );
            failures
                .entry(index.parse::<usize>().unwrap())
                .or_default()
                .push((doc.parse::<usize>().unwrap(), error.to_string()));
        }
    }
    failures
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Refused,
    Reads,
    /// The rules covering every failing document, joined with `+`.
    Trusted(String),
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
fn observe(project: &Project, cases: &[Case]) -> Vec<Result<Outcome, String>> {
    let units: Vec<Unit> = cases.iter().map(case_unit).collect();
    let (refused, failures) = compile(project, &units);
    cases
        .iter()
        .enumerate()
        .map(|(index, case)| {
            if refused.contains(&index) {
                return Ok(Outcome::Refused);
            }
            let Some(failed) = failures.get(&index) else {
                return Ok(Outcome::Reads);
            };
            let mut rules = BTreeSet::new();
            for (doc, error) in failed {
                let rule = TRUSTED
                    .iter()
                    .find(|trust| (trust.applies)(case, *doc, error))
                    .ok_or_else(|| format!("document {doc}: {error}"))?;
                rules.insert(rule.rule);
            }
            Ok(Outcome::Trusted(
                rules.into_iter().collect::<Vec<_>>().join("+"),
            ))
        })
        .collect()
}

/// The shapes whose declarations alone the derives refuse: refused at every
/// placement, so never placed.
fn refused_declarations(project: &Project, shapes: &[Shape]) -> BTreeSet<usize> {
    let units: Vec<Unit> = shapes
        .iter()
        .map(|shape| Unit {
            body: shape.decls.join("\n") + "\n",
            reads: false,
        })
        .collect();
    compile(project, &units).0
}

const OUTCOMES: &str = "tests/boundary.outcomes";

#[test]
fn every_shape_is_refused_reads_or_is_trusted() {
    let started = std::time::Instant::now();
    let project = Project::new();
    let lists = [("", named_shapes()), ("gen/", generated_shapes())];
    let declared: Vec<Shape> = lists
        .iter()
        .flat_map(|(_, shapes)| shapes.clone())
        .collect();
    let refused_shapes = refused_declarations(&project, &declared);
    let mut placed = Vec::new();
    let mut refused_names = Vec::new();
    let mut offset = 0;
    for (prefix, shapes) in &lists {
        for (index, shape) in shapes.iter().enumerate() {
            if refused_shapes.contains(&(offset + index)) {
                refused_names.push(format!("{prefix}{}", shape.name));
            } else {
                placed.push((*prefix, shape.clone()));
            }
        }
        offset += shapes.len();
    }
    // Roots first. A shape refused at every root placement is placed in no
    // fragment: the parity check covers only shapes that compile somewhere.
    let mut cases: Vec<Case> = placed
        .iter()
        .flat_map(|(prefix, shape)| {
            let shape = std::slice::from_ref(shape);
            let mut cases = self::cases(shape, prefix, Position::Root);
            cases.extend(self::cases(shape, prefix, Position::Flattened));
            cases
        })
        .collect();
    let mut observed = observe(&project, &cases);
    let compiles: BTreeSet<String> = cases
        .iter()
        .zip(&observed)
        .filter(|(case, outcome)| {
            case.position == Position::Root && !matches!(outcome, Ok(Outcome::Refused))
        })
        .map(|(case, _)| case.shape.name.clone())
        .collect();
    let fragments: Vec<Case> = placed
        .iter()
        .filter(|(_, shape)| compiles.contains(&shape.name))
        .flat_map(|(prefix, shape)| {
            self::cases(std::slice::from_ref(shape), prefix, Position::Fragment)
        })
        .collect();
    observed.extend(observe(&project, &fragments));
    cases.extend(fragments);
    println!(
        "{} declarations, {} cases compiled and read in {:?}",
        declared.len(),
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
        let Some(other) = outcome_of.get(format!("{prefix}fragment/{rest}").as_str()) else {
            continue;
        };
        let (shape, kind) = rest.rsplit_once('/').unwrap();
        let excepted = DISAGREE
            .iter()
            .any(|&(s, k, _)| s == shape && (k == kind || k == "*"));
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
    for name in &refused_names {
        writeln!(rendered, "{name} refused").unwrap();
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

/// The plugin skill's map example is the one `ui_pass/docs_map_examples.rs`
/// compiles and reads (the manual includes its own by anchor).
#[test]
fn the_docs_map_examples_are_the_tested_ones() {
    let root = Path::new(MANIFEST_DIR).join("../..");
    let tested =
        std::fs::read_to_string(Path::new(MANIFEST_DIR).join("tests/ui_pass/docs_map_examples.rs"))
            .unwrap();
    let anchored = tested
        .split("// ANCHOR: plugin\n")
        .nth(1)
        .and_then(|rest| rest.split("// ANCHOR_END: plugin").next())
        .unwrap();
    let skill = std::fs::read_to_string(root.join("plugin/skills/flusso-query/maps.md")).unwrap();
    let block = skill
        .split("```rust\n")
        .nth(1)
        .and_then(|rest| rest.split("```").next())
        .unwrap();
    assert_eq!(
        block, anchored,
        "the plugin's map example drifted from the tested one"
    );
    let manual = std::fs::read_to_string(root.join("docs/src/query/maps.md")).unwrap();
    assert!(manual.contains("docs_map_examples.rs:translation}}"));
}
