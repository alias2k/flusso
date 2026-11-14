//! Real-service e2e for geo points (issue #307): a one-column `geo` field
//! stores what its `text` or `jsonb` column holds, so `_source` carries the
//! point in any form OpenSearch takes. `GeoPoint` must read each one with the
//! coordinates OpenSearch indexed.
//!
//! Against a live OpenSearch indexed by the **real sink**, with each document's
//! fields as the Postgres source hands them over (a `text` column a string, a
//! `jsonb` column its JSON):
//!
//! - A `#[derive(FlussoRoot)]` struct reads every form back as the point it
//!   names (`tests/fixtures/geo/`).
//! - A `geo_distance` query around the point read back matches its document,
//!   and one around the same point with lat and lon swapped does not, so
//!   OpenSearch and `GeoPoint` agree on which coordinate is the latitude.
//!
//! Requires Docker. `#[ignore]`d like the other e2e tests; run with:
//!
//! ```text
//! cargo nextest run -p flusso-query-e2e --run-ignored all -E 'binary(geo_points)'
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    unused_crate_dependencies,
    missing_debug_implementations
)]

mod common;

use std::collections::BTreeMap;

use common::{refresh, seed_many, sink, start_opensearch};
use flusso_query::{Client, Distance, FlussoRoot, Geo, GeoPoint};
use kernel::{GenericValue, IndexMapping, IndexName};
use serde_json::{Value, json};

#[derive(Debug, serde::Deserialize, FlussoRoot)]
#[flusso(index = "places", config = "tests/fixtures/geo/flusso.toml")]
struct Place {
    id: String,
    pinned: Option<GeoPoint>,
    from_text: Option<GeoPoint>,
    from_jsonb: Option<GeoPoint>,
}

fn places_mapping() -> IndexMapping {
    let config = config::load(std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/geo/flusso.toml"
    )))
    .unwrap();
    let name = IndexName::try_new(Place::INDEX).unwrap();
    config.indexes.get(&name).unwrap().schema.resolve(name)
}

/// The value tree the Postgres source builds from a column's JSON.
fn generic(value: Value) -> GenericValue {
    match value {
        Value::Null => GenericValue::Null,
        Value::Bool(b) => GenericValue::Bool(b),
        Value::Number(n) => n.as_i64().map_or_else(
            || GenericValue::Double(n.as_f64().unwrap()),
            GenericValue::BigInt,
        ),
        Value::String(s) => GenericValue::String(s),
        Value::Array(items) => GenericValue::Array(items.into_iter().map(generic).collect()),
        Value::Object(fields) => {
            GenericValue::Map(fields.into_iter().map(|(k, v)| (k, generic(v))).collect())
        }
    }
}

/// One place: every field holds the point `(lat, lon)`, the two-column one as
/// `{lat, lon}`, the others in the given forms.
struct Case {
    id: &'static str,
    lat: f64,
    lon: f64,
    text: &'static str,
    jsonb: Value,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "coordinates",
            lat: 10.5,
            lon: 40.25,
            text: "10.5,40.25",
            jsonb: json!([40.25, 10.5]),
        },
        Case {
            id: "wkt-geojson",
            lat: -20.5,
            lon: 60.75,
            text: "POINT (60.75 -20.5)",
            jsonb: json!({ "type": "Point", "coordinates": [60.75, -20.5] }),
        },
        // A geohash is indexed as its cell's south-west corner: `u` spans
        // latitudes 45..90 and longitudes 0..45.
        Case {
            id: "geohash",
            lat: 45.0,
            lon: 0.0,
            text: "u",
            jsonb: json!({ "geohash": "u" }),
        },
        Case {
            id: "altitude-string",
            lat: -33.5,
            lon: 151.25,
            text: " -33.5 , 151.25 , 7 ",
            jsonb: json!("-33.5,151.25"),
        },
    ]
}

fn document(case: &Case) -> GenericValue {
    GenericValue::Map(BTreeMap::from([
        ("id".to_owned(), GenericValue::String(case.id.to_owned())),
        (
            "pinned".to_owned(),
            generic(json!({ "lat": case.lat, "lon": case.lon })),
        ),
        (
            "from_text".to_owned(),
            GenericValue::String(case.text.to_owned()),
        ),
        ("from_jsonb".to_owned(), generic(case.jsonb.clone())),
    ]))
}

async fn within_a_km(client: &Client, field: Geo, center: GeoPoint) -> Vec<String> {
    let mut ids: Vec<String> = Place::query()
        .filter(field.within(Distance::km(1.0), center))
        .send(client)
        .await
        .unwrap()
        .hits
        .into_iter()
        .map(|hit| hit.id)
        .collect();
    ids.sort();
    ids
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs Docker (starts an OpenSearch container)"]
async fn a_geo_point_reads_back_where_opensearch_indexed_it() {
    let (_container, base) = start_opensearch().await;
    let sink = sink(&base);
    let cases = cases();
    let docs: Vec<(&str, GenericValue)> = cases.iter().map(|c| (c.id, document(c))).collect();
    seed_many(&sink, &places_mapping(), &docs).await;
    refresh(&base, &Place::physical_index()).await;
    let client = Client::connect(&base).unwrap();

    for case in &cases {
        let place = Place::get(&client, case.id).await.unwrap().unwrap();
        assert_eq!(place.id, case.id);
        let fields = [
            ("pinned", place.pinned, Place::pinned()),
            ("from_text", place.from_text, Place::from_text()),
            ("from_jsonb", place.from_jsonb, Place::from_jsonb()),
        ];
        for (name, point, handle) in fields {
            let point = point.unwrap_or_else(|| panic!("{}: `{name}` read as null", case.id));
            assert!(
                (point.lat - case.lat).abs() < 1e-9 && (point.lon - case.lon).abs() < 1e-9,
                "{}: `{name}` read as lat {}, lon {}; expected lat {}, lon {}",
                case.id,
                point.lat,
                point.lon,
                case.lat,
                case.lon,
            );
            assert_eq!(
                within_a_km(&client, handle.clone(), point).await,
                [case.id],
                "{}: `{name}` is not where OpenSearch indexed it",
                case.id,
            );
            let swapped = GeoPoint::new(point.lon.clamp(-90.0, 90.0), point.lat);
            assert!(
                within_a_km(&client, handle, swapped).await.is_empty(),
                "{}: `{name}` also matches with lat and lon swapped",
                case.id,
            );
        }
    }
}
