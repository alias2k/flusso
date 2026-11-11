//! The one Rhai engine every script runs in: the standard library minus anything
//! that makes a script impure or unbounded.
//!
//! - **Pure:** no clock (`no_time`), no imports (`no_module`), no `eval`. A
//!   script is a function of the document, so an at-least-once rebuild produces
//!   the same value.
//! - **Strict:** an undefined variable fails at compile time, and reading a
//!   property a map doesn't have fails at run time instead of yielding `()`, so a
//!   misspelled field is an error, not a silent null.
//! - **Bounded:** fixed caps on operations, call depth, expression depth and the
//!   size of strings, arrays and maps, so a runaway script fails one document
//!   instead of stalling the pipeline.
//!
//! ```
//! let engine = script::sandbox::engine();
//! assert!(engine.compile("let x = undefined_name;").is_err());
//! ```

use rhai::Engine;

/// Operations one script call may run.
pub const MAX_OPERATIONS: u64 = 1_000_000;
/// Nested function calls one script may make.
pub const MAX_CALL_LEVELS: usize = 32;
/// Expression nesting depth, at the top level and inside functions.
pub const MAX_EXPR_DEPTH: usize = 64;
/// The longest string a script may build, in bytes.
pub const MAX_STRING_SIZE: usize = 1 << 20;
/// The most elements an array (or entries a map) may hold.
pub const MAX_COLLECTION_SIZE: usize = 100_000;

/// A fresh sandboxed engine.
pub fn engine() -> Engine {
    let mut engine = Engine::new();
    engine
        .disable_symbol("eval")
        .set_strict_variables(true)
        .set_fail_on_invalid_map_property(true)
        .set_max_operations(MAX_OPERATIONS)
        .set_max_call_levels(MAX_CALL_LEVELS)
        .set_max_expr_depths(MAX_EXPR_DEPTH, MAX_EXPR_DEPTH)
        .set_max_string_size(MAX_STRING_SIZE)
        .set_max_array_size(MAX_COLLECTION_SIZE)
        .set_max_map_size(MAX_COLLECTION_SIZE);
    engine.on_print(|_| {});
    engine.on_debug(|_, _, _| {});
    engine
}
