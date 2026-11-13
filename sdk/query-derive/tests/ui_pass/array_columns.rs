use flusso_query::{FlussoFragment, FlussoRoot, FlussoValue, Sortable};

// Array columns (issue #276): a `Vec` of the element, an `Option` of one, and
// a `FlussoValue` newtype over a `Vec` (or over such a newtype), on a root and
// in a fragment; a `json` array as a `Vec` of anything. Filters take the
// element.

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[flusso(keyword)]
#[serde(rename_all = "snake_case")]
enum Tag {
    Urgent,
    Fragile,
}

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
#[serde(transparent)]
struct Tags(Vec<Tag>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Codes(Vec<String>);

#[derive(serde::Serialize, serde::Deserialize, FlussoValue)]
struct Forwarded(Codes);

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct Order {
    id: String,
    tags: Option<Vec<Tag>>,
    labels: Vec<String>,
    scores: Option<Vec<i32>>,
    notes: Option<Vec<serde_json::Value>>,
    lines: Vec<Line>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Line {
    sku: String,
    codes: Codes,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct OrderWithNewtypes {
    id: String,
    tags: Option<Tags>,
    labels: Codes,
    lines: Vec<LineWithVec>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct LineWithVec {
    codes: Vec<String>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct Tagged {
    tags: Option<Tags>,
    notes: Option<serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct OrderWithRawNotes {
    id: String,
    notes: Option<serde_json::Value>,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct OrderWithFragment {
    id: String,
    #[serde(flatten)]
    tagged: Tagged,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct OrderWithForwarded {
    id: String,
    labels: Forwarded,
    notes: Option<Vec<Codes>>,
    lines: Vec<LineWithForwarded>,
}

#[derive(serde::Deserialize, FlussoFragment)]
struct LineWithForwarded {
    codes: Forwarded,
}

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "lists")]
struct OrderWithForwardedNotes {
    id: String,
    notes: Option<Forwarded>,
}

fn main() {
    let _ = Order::tags().any_of([Tag::Urgent]);
    let _ = Order::tags().eq(Tag::Fragile);
    let _ = Order::scores().gte(3);
    let _ = flusso_order_query::Lines::codes().eq("a");
    let _ = (Order::tags().asc(), Order::scores().desc());
    let _ = OrderWithNewtypes::tags().any_of([Tag::Urgent]);
    let _ = OrderWithFragment::tags();
}
