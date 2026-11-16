//! flusso replaces Rhai's `sort()` / `sort_desc()` (with no comparator) to add the date types.
//! For every other element type the copy must do exactly what Rhai's own does, so a Rhai bump
//! that changes the original fails here: each case runs in the sandbox and in a plain
//! `Engine::new()`, and both outcomes (the array, or the error) must be the same text.

#![allow(clippy::unwrap_used, unused_crate_dependencies)]

/// The outcome of `script` as text: its value's debug form, or its error.
fn outcome(engine: &rhai::Engine, script: &str) -> String {
    match engine.eval::<rhai::Dynamic>(script) {
        Ok(value) => format!("ok {value:?}"),
        Err(error) => format!("err {error}"),
    }
}

#[test]
fn sorting_without_a_comparator_matches_rhai_for_every_element_type() {
    let elements = [
        "1",
        "2",
        "-3",
        "1.5",
        "0.5",
        "parse_decimal(\"2.5\")",
        "parse_decimal(\"1\")",
        "'b'",
        "'a'",
        "\"b\"",
        "\"a\"",
        "true",
        "false",
        "()",
        "#{a: 1}",
        "[1]",
        "blob(1)",
        "Fn(\"f\")",
    ];
    let ours = script::sandbox::engine();
    let rhai = rhai::Engine::new();
    let mut cases = 0;
    let mut arrays: Vec<String> = vec![String::new()];
    for a in elements {
        arrays.push(a.to_owned());
        for b in elements {
            arrays.push(format!("{a}, {b}"));
            arrays.push(format!("{a}, {b}, {a}"));
        }
    }
    for items in &arrays {
        for method in ["sort", "sort_desc"] {
            let script = format!("let x = [{items}]; x.{method}(); x");
            assert_eq!(outcome(&ours, &script), outcome(&rhai, &script), "{script}");
            cases += 1;
        }
    }
    assert!(cases > 1_000, "{cases}");
}
