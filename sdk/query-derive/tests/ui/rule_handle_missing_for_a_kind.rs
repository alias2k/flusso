// One field of every kind a rule cannot read: a `nested` array (`has_many`,
// `many_to_many`), a map, `json`, a geo point, a constant stored unindexed, a
// 32-bit float, `binary`, a `custom` mapping and an unindexed join.
use flusso_query::FlussoRoot;

struct Viewing;

#[derive(serde::Deserialize, FlussoRoot)]
#[flusso(index = "boundary", context = Viewing)]
struct EveryKind {
    id: String,
}

fn main() {
    let doc = EveryKind::doc();
    // The `has_many` named `ne` is left out: `doc.ne(..)` is `PartialEq`'s.
    let _ = doc.mm();
    let _ = doc.mp();
    let _ = doc.mpn();
    let _ = doc.js();
    let _ = doc.jsa();
    let _ = doc.gp();
    let _ = doc.cm();
    let _ = doc.co();
    let _ = doc.fl();
    let _ = doc.bi();
    let _ = doc.cu();
    let _ = doc.hu();
}
