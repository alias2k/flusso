//! Compiling every script of a deployment once, and running them over a built document.
//!
//! [`Scripts::compile`] walks each index schema into a **plan**: per object level, the script
//! fields to compute, the input-only (`emit: false`) fields to strip, and the child levels
//! (groups and joins) that hold more of either. A level with none is not in the plan, so a
//! document of an index without scripts or input-only fields is left untouched.
//!
//! [`Scripts::apply`] runs the plan over one document, level by level:
//!
//! 1. Every script of the level runs against the level as it was built (`this`) and the whole
//!    document as it was built (`root`). Scripts never see another script's output.
//! 2. Child levels are processed the same way (each element of a nested array on its own).
//! 3. The outputs are written in, and the input-only fields stripped.
//!
//! The first failure stops the document and is returned as a [`BuildFailure`] carrying the
//! field path (`variants[2].grossPrice`) and, for a script error, the line, column and source
//! line.
//!
//! ```
//! use std::collections::BTreeMap;
//! use kernel::{GenericValue, IndexName};
//! use script::Scripts;
//!
//! # fn demo(spec: &source::SourceSpec) -> Result<(), Box<dyn std::error::Error>> {
//! let scripts = Scripts::compile(spec)?;
//! let index = IndexName::try_new("users")?;
//! let mut body = GenericValue::Map(BTreeMap::new());
//! scripts.apply(&index, &mut body)?;
//! # Ok(())
//! # }
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use kernel::{Field, FieldSource, GenericValue, IndexName, Relation, Script};
use rhai::{AST, CallFnOptions, Dynamic, Engine, EvalAltResult, ParseError, Position, Scope};
use source::SourceSpec;
use source::document::{BuildFailure, ScriptPosition};

use crate::sandbox;
use crate::value::{Output, from_dynamic, to_dynamic};

/// The wrapper every script is compiled in: a function, so `this` can be bound
/// to the object at the field's level and `root` passed in. Kept on the
/// script's first line so line numbers match the schema; columns on line 1 are
/// shifted back by its length.
const PREFIX: &str = "fn __flusso(root) { ";
const ENTRY: &str = "__flusso";

/// Where a script's printed output goes.
pub type Printer = Arc<dyn Fn(&ScriptContext<'_>, &str) + Send + Sync>;

/// Which script printed.
#[derive(Debug, Clone, Copy)]
pub struct ScriptContext<'a> {
    /// The index whose script printed.
    pub index: &'a str,
    /// The document's id, as the sink writes it; empty when the caller gave none.
    pub document: &'a str,
    /// The printing field's path, e.g. `variants[2].gross`.
    pub field: &'a str,
}

#[derive(Default)]
struct Running {
    index: String,
    document: String,
    field: String,
}

thread_local! {
    /// The script running on this thread, so a print callback (which Rhai calls
    /// with the text only) can say who printed.
    static RUNNING: std::cell::RefCell<Running> = std::cell::RefCell::default();
}

fn with_running(f: impl FnOnce(&ScriptContext<'_>)) {
    RUNNING.with_borrow(|running| {
        f(&ScriptContext {
            index: &running.index,
            document: &running.document,
            field: &running.field,
        });
    });
}

fn set_running(update: impl FnOnce(&mut Running)) {
    RUNNING.with_borrow_mut(update);
}

fn emit_print(printer: Option<&Printer>, text: &str) {
    with_running(|context| match printer {
        Some(printer) => printer(context, text),
        None => tracing::debug!(
            target: "script",
            index = context.index,
            id = context.document,
            field = context.field,
            "{text}"
        ),
    });
}

/// Every script of a deployment, compiled, ready to run over built documents.
pub struct Scripts {
    engine: Engine,
    plans: BTreeMap<IndexName, IndexPlan>,
}

impl fmt::Debug for Scripts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Scripts")
            .field("indexes", &self.plans.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

struct IndexPlan {
    root: Level,
    /// Whether any script reads `root`; if none does, it is never built.
    reads_root: bool,
}

#[derive(Default)]
struct Level {
    scripts: Vec<CompiledScript>,
    hidden: Vec<String>,
    children: Vec<(String, Level)>,
}

impl Level {
    fn is_empty(&self) -> bool {
        self.scripts.is_empty() && self.hidden.is_empty() && self.children.is_empty()
    }
}

struct CompiledScript {
    name: String,
    ast: AST,
    spec: Script,
    /// The source's lines, for the excerpt under a runtime error.
    lines: Vec<String>,
}

/// One script that does not compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    /// The index the script belongs to.
    pub index: IndexName,
    /// The field, the parser's message, and where in the script.
    pub failure: BuildFailure,
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "index `{}`, field {}", self.index, self.failure)
    }
}

/// Every script that does not compile, so all of them can be fixed in one go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileErrors(pub Vec<CompileError>);

impl fmt::Display for CompileErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.0.len();
        writeln!(
            f,
            "{count} script{} failed to compile:",
            if count == 1 { "" } else { "s" }
        )?;
        for error in &self.0 {
            writeln!(f, "\n{error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for CompileErrors {}

impl Scripts {
    /// Compile every script field of every index in `spec`, reporting every
    /// script that fails rather than the first. Prints go to `tracing` at
    /// `debug` under the `script` target.
    pub fn compile(spec: &SourceSpec) -> Result<Self, CompileErrors> {
        Self::compile_with_printer(spec, None)
    }

    /// [`compile`](Self::compile), sending each script's `print`/`debug`
    /// output to `printer` instead of `tracing`.
    pub fn compile_with_printer(
        spec: &SourceSpec,
        printer: Option<Printer>,
    ) -> Result<Self, CompileErrors> {
        let engine = sandbox::engine();
        let mut plans = BTreeMap::new();
        let mut errors = Vec::new();
        for (index, schema) in spec.indexes() {
            let mut reads_root = false;
            let root = plan_level(
                &engine,
                &schema.fields,
                "",
                &mut reads_root,
                &mut |failure| {
                    errors.push(CompileError {
                        index: index.clone(),
                        failure,
                    });
                },
            );
            if !root.is_empty() {
                plans.insert(index.clone(), IndexPlan { root, reads_root });
            }
        }
        if !errors.is_empty() {
            return Err(CompileErrors(errors));
        }
        let mut scripts = Self { engine, plans };
        scripts.route_prints(printer);
        Ok(scripts)
    }

    /// Whether any index has a script or an input-only field: with none, the
    /// builder needs no wrapping.
    pub fn is_empty(&self) -> bool {
        self.plans.is_empty()
    }

    /// Run `index`'s scripts over one built document, then strip its
    /// input-only fields. Leaves the body untouched for an index with neither.
    pub fn apply(&self, index: &IndexName, body: &mut GenericValue) -> Result<(), BuildFailure> {
        self.apply_document(index, "", body)
    }

    /// [`apply`](Self::apply), naming the document so what a script prints says
    /// which document it ran on.
    pub fn apply_document(
        &self,
        index: &IndexName,
        document: &str,
        body: &mut GenericValue,
    ) -> Result<(), BuildFailure> {
        let Some(plan) = self.plans.get(index) else {
            return Ok(());
        };
        set_running(|running| {
            running.index.clear();
            running.index.push_str(index.as_ref());
            running.document.clear();
            running.document.push_str(document);
        });
        // Shared, so each call passes a pointer rather than a deep copy; read-only,
        // so no script can change what the next one reads.
        let root = if plan.reads_root {
            to_dynamic(body).into_read_only().into_shared()
        } else {
            Dynamic::UNIT
        };
        let run = Run {
            engine: &self.engine,
            root: &root,
        };
        run.value(&plan.root, body, "")
    }

    fn route_prints(&mut self, printer: Option<Printer>) {
        let print = printer.clone();
        self.engine
            .on_print(move |text| emit_print(print.as_ref(), text));
        self.engine
            .on_debug(move |text, _, _| emit_print(printer.as_ref(), text));
    }
}

fn plan_level(
    engine: &Engine,
    fields: &[Field],
    prefix: &str,
    reads_root: &mut bool,
    report: &mut dyn FnMut(BuildFailure),
) -> Level {
    let mut level = Level::default();
    for field in fields {
        let name = field.field.as_ref().to_owned();
        if !field.emit {
            level.hidden.push(name);
            continue;
        }
        match &field.source {
            FieldSource::Script(spec) => {
                let path = format!("{prefix}{name}");
                match compile_one(engine, &spec.source) {
                    Ok(ast) => {
                        *reads_root |= spec.source.contains("root");
                        level.scripts.push(CompiledScript {
                            name,
                            ast,
                            spec: spec.clone(),
                            lines: spec.source.lines().map(str::to_owned).collect(),
                        });
                    }
                    Err(error) => report(parse_failure(path, &spec.source, &error)),
                }
            }
            FieldSource::Group(children) => {
                let child = plan_level(
                    engine,
                    children,
                    &format!("{prefix}{name}."),
                    reads_root,
                    report,
                );
                if !child.is_empty() {
                    level.children.push((name, child));
                }
            }
            FieldSource::Relation(Relation::Join(join)) => {
                let marker = if join.kind.is_to_many() { "[]" } else { "" };
                let child = plan_level(
                    engine,
                    &join.fields,
                    &format!("{prefix}{name}{marker}."),
                    reads_root,
                    report,
                );
                if !child.is_empty() {
                    level.children.push((name, child));
                }
            }
            FieldSource::Column(_)
            | FieldSource::Geo(_)
            | FieldSource::Relation(Relation::Aggregate(_))
            | FieldSource::Constant(_) => {}
        }
    }
    level
}

fn compile_one(engine: &Engine, source: &str) -> Result<AST, ParseError> {
    engine.compile(format!("{PREFIX}{source}\n}}"))
}

fn parse_failure(field: String, source: &str, error: &ParseError) -> BuildFailure {
    BuildFailure {
        field,
        message: error.0.to_string(),
        position: position(source.lines(), error.1),
    }
}

/// Map a position in the wrapped script back onto the user's source.
fn position<'a>(lines: impl Iterator<Item = &'a str>, at: Position) -> Option<ScriptPosition> {
    let lines: Vec<&str> = lines.collect();
    let wrapped_line = at.line()?;
    let last = lines.len().max(1);
    let line = wrapped_line.min(last);
    let excerpt = lines.get(line - 1).copied().unwrap_or_default().to_owned();
    let column = match at.position() {
        Some(column) if wrapped_line == 1 => column.saturating_sub(PREFIX.len()).max(1),
        Some(column) if wrapped_line <= last => column,
        _ => excerpt.chars().count() + 1,
    };
    Some(ScriptPosition {
        line,
        column,
        excerpt,
    })
}

struct Run<'a> {
    engine: &'a Engine,
    root: &'a Dynamic,
}

impl Run<'_> {
    /// One level's value: an object, an array of them (a to-many join), or
    /// null (an absent to-one join).
    fn value(
        &self,
        level: &Level,
        value: &mut GenericValue,
        prefix: &str,
    ) -> Result<(), BuildFailure> {
        match value {
            GenericValue::Map(map) => self.object(level, map, prefix),
            GenericValue::Array(items) => {
                let base = prefix.strip_suffix('.').unwrap_or(prefix);
                for (i, item) in items.iter_mut().enumerate() {
                    if let GenericValue::Map(map) = item {
                        self.object(level, map, &format!("{base}[{i}]."))?;
                    }
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn object(
        &self,
        level: &Level,
        map: &mut BTreeMap<String, GenericValue>,
        prefix: &str,
    ) -> Result<(), BuildFailure> {
        let mut outputs = Vec::with_capacity(level.scripts.len());
        if !level.scripts.is_empty() {
            let this = Dynamic::from_map(
                map.iter()
                    .map(|(key, value)| (key.as_str().into(), to_dynamic(value)))
                    .collect(),
            );
            for script in &level.scripts {
                let path = format!("{prefix}{}", script.name);
                let value = self.eval(script, this.clone(), &path)?;
                outputs.push((script.name.clone(), value));
            }
        }
        for (name, child) in &level.children {
            if let Some(value) = map.get_mut(name) {
                self.value(child, value, &format!("{prefix}{name}."))?;
            }
        }
        for (name, value) in outputs {
            map.insert(name, value);
        }
        for name in &level.hidden {
            map.remove(name);
        }
        Ok(())
    }

    fn eval(
        &self,
        script: &CompiledScript,
        mut this: Dynamic,
        path: &str,
    ) -> Result<GenericValue, BuildFailure> {
        set_running(|running| {
            running.field.clear();
            running.field.push_str(path);
        });
        let options = CallFnOptions::new()
            .eval_ast(false)
            .bind_this_ptr(&mut this);
        let result = self.engine.call_fn_with_options::<Dynamic>(
            options,
            &mut Scope::new(),
            &script.ast,
            ENTRY,
            (self.root.clone(),),
        );
        let value = result.map_err(|error| runtime_failure(path, script, *error))?;
        let output = Output {
            ty: &script.spec.ty,
            nullable: script.spec.nullable,
            array: script.spec.array,
            variants: &script.spec.enum_order,
        };
        from_dynamic(value, &output).map_err(|message| BuildFailure {
            field: path.to_owned(),
            message,
            position: None,
        })
    }
}

fn runtime_failure(path: &str, script: &CompiledScript, error: EvalAltResult) -> BuildFailure {
    let mut inner = innermost(error);
    let at = inner.take_position();
    BuildFailure {
        field: path.to_owned(),
        message: inner.to_string(),
        position: position(script.lines.iter().map(String::as_str), at),
    }
}

/// The error a script actually raised, unwrapped from the call into the
/// wrapper function.
fn innermost(error: EvalAltResult) -> EvalAltResult {
    match error {
        EvalAltResult::ErrorInFunctionCall(_, _, inner, _) => innermost(*inner),
        other => other,
    }
}

#[cfg(test)]
mod tests;
