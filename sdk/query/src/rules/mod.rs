//! Read-time fields: fields whose value depends on something known only when a
//! document is read, such as the clock, the caller or a rate.
//!
//! flusso computes everything it can when it writes a document. What is left
//! is what cannot be stored: a contract is `expiring` from a moment that
//! arrives with no write at all. A root declares such fields, names the
//! request-time values they read as its **context**, and gets them on every
//! document it fetches. There are two kinds.
//!
//! # Rule fields
//!
//! A [`Rules<T>`] is an ordered list of *condition → outcome* arms; the first
//! arm whose [`Condition`] holds decides the value. An outcome is a fixed
//! value or a stored field. One list drives four things, none of them a
//! script:
//!
//! - the **value** on a fetched document, decided in Rust on its `_source`;
//! - a **filter** on the value ([`RuleField::eq`] and friends), a plain query
//!   over the stored fields the rules read;
//! - a **sort** ([`RuleField::asc`]), a `function_score` whose weights are the
//!   value's rank;
//! - **counts** per value ([`RuleField::counts`]), one `_msearch`.
//!
//! Conditions are built from the rule handles ([`RuleKeyword`], [`RuleBool`],
//! [`RuleNumber`], [`RuleDate`]), which `#[derive(FlussoRoot)]` generates
//! behind `Root::doc()` for exactly the fields a rule can read, with exactly
//! the tests a rule can make. Every test renders as a query *and* is decided
//! on a document, and the two agree, so a listing never shows a value its own
//! filter does not select. Anything the two could disagree on (analyzed text,
//! a 32-bit float, a `nested` array) has no handle, so such a rule does not
//! compile.
//!
//! # Computed fields
//!
//! A computed field is a plain function over the stored document and the
//! context. It can do anything Rust can, and gives the value only: there is
//! nothing to filter or sort on.
//!
//! # Example
//!
//! The derive wires both kinds from attributes; this is what a root with them
//! looks like (`ignore`d here because it needs a `flusso.toml`):
//!
//! ```ignore
//! use flusso_query::rules::Rules;
//!
//! struct Viewing { now: DateTime<Utc>, viewer: String }
//!
//! #[derive(Deserialize, FlussoRoot)]
//! #[flusso(index = "contracts", context = Viewing)]
//! struct Contract {
//!     id: String,
//!     #[flusso(rules = visible_state)]
//!     state: ContractState,
//!     #[flusso(computed = days_left)]
//!     days_left: i64,
//! }
//!
//! fn visible_state(cx: &Viewing) -> Rules<ContractState> {
//!     let doc = Contract::doc();
//!     Rules::when(doc.signed_at().missing())
//!         .then(ContractState::Draft)
//!         .when(doc.ends_at().lt(cx.now))
//!         .then(ContractState::Closed)
//!         .otherwise(doc.state())
//! }
//!
//! #[derive(Deserialize)]
//! struct Term { ends_at: DateTime<Utc> }
//!
//! fn days_left(term: &Term, cx: &Viewing) -> i64 {
//!     (term.ends_at - cx.now).num_days()
//! }
//!
//! let closed = Contract::query(&cx)
//!     .filter(Contract::state(&cx).eq(ContractState::Closed))
//!     .sort(Contract::state(&cx).asc())
//!     .send(&client)
//!     .await?;
//! ```
//!
//! The pieces work without the derive too, which is how the tests in this
//! module drive them:
//!
//! ```
//! use flusso_query::rules::{Computation, Required, RuleDate, Rules};
//! use serde_json::json;
//!
//! struct Viewing { now: &'static str }
//!
//! fn state(cx: &Viewing) -> Rules<&'static str> {
//!     Rules::when(RuleDate::<Required>::at("ends_at").lt(cx.now))
//!         .then("closed")
//!         .otherwise("open")
//! }
//!
//! let cx = Viewing { now: "2026-11-16T12:00:00Z" };
//! let computation = Computation::with(&cx).rules("state", state).finish();
//!
//! let mut source = json!({ "ends_at": "2026-11-01T00:00:00Z" });
//! computation.apply(&mut source)?;
//! assert_eq!(source["state"], "closed");
//! # Ok::<(), flusso_query::Error>(())
//! ```

mod computation;
mod condition;
mod date;
mod field;
mod handles;
mod list;

#[cfg(test)]
mod tests;

pub use computation::{Computation, ComputationBuilder};
pub use condition::Condition;
pub(crate) use condition::{invalid, refuse_invalid};
pub use field::RuleField;
pub use handles::{
    Array, Optional, Presence, Present, Required, RuleBool, RuleDate, RuleKeyword, RuleNumber,
    RuleNumberKind,
};
pub use list::{IntoOutcome, Outcome, Rules, RulesBuilder, When};
