//! Writing a root's read-time fields into each fetched document.

use std::fmt;
use std::sync::Arc;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::list::{Plan, Rules};
use crate::{Error, Result};

type Step = Arc<dyn Fn(&mut Value) -> Result<()> + Send + Sync>;

fn failed(key: &str, reason: impl fmt::Display) -> Error {
    Error::ReadTimeField(format!("`{key}`: {reason}"))
}

/// Everything a root computes on a fetched document, bound to one context.
///
/// Built once per search by the derive, applied to each document's `_source`
/// before it decodes. Rule fields are decided first, all from the stored
/// document; computed fields run after, in declaration order, each reading the
/// document with every field written before it.
#[derive(Clone, Default)]
pub struct Computation {
    rules: Vec<(String, Arc<Plan>)>,
    computed: Vec<Step>,
    error: Option<String>,
}

impl fmt::Debug for Computation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Computation")
            .field("rules", &self.rules.len())
            .field("computed", &self.computed.len())
            .field("error", &self.error)
            .finish()
    }
}

impl Computation {
    /// Nothing to compute: what a root without read-time fields uses.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }

    /// Start the computation of a root whose context is `cx`.
    #[must_use]
    pub fn with<C>(cx: &C) -> ComputationBuilder<'_, C> {
        ComputationBuilder {
            cx,
            shared: None,
            built: Self::default(),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rules.is_empty() && self.computed.is_empty() && self.error.is_none()
    }

    /// Write every read-time field into one document's `_source`.
    pub fn apply(&self, source: &mut Value) -> Result<()> {
        if let Some(reason) = &self.error {
            return Err(Error::ReadTimeField(reason.clone()));
        }
        let decided = self
            .rules
            .iter()
            .map(|(key, plan)| match plan.value(source) {
                Ok(value) => Ok((key.clone(), value)),
                Err(reason) => Err(failed(key, reason)),
            })
            .collect::<Result<Vec<_>>>()?;
        if let Some(object) = source.as_object_mut() {
            object.extend(decided);
        }
        self.computed.iter().try_for_each(|step| step(source))
    }

    /// [`apply`](Self::apply) to every hit of a `_search` response.
    pub(crate) fn apply_response(&self, response: &mut Value) -> Result<()> {
        let hits = response
            .get_mut("hits")
            .and_then(|hits| hits.get_mut("hits"))
            .and_then(Value::as_array_mut);
        for hit in hits.into_iter().flatten() {
            if let Some(source) = hit.get_mut("_source") {
                self.apply(source)?;
            }
        }
        Ok(())
    }
}

/// Collects a root's read-time fields for one context. The derive drives it.
pub struct ComputationBuilder<'a, C> {
    cx: &'a C,
    /// The context as the computed functions share it, cloned once on the
    /// first computed field.
    shared: Option<Arc<C>>,
    built: Computation,
}

impl<C> fmt::Debug for ComputationBuilder<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComputationBuilder")
            .field("built", &self.built)
            .finish_non_exhaustive()
    }
}

impl<C> ComputationBuilder<'_, C> {
    /// Add a rule field written under `key`, decided by the rules `build`
    /// returns for this context.
    #[must_use]
    pub fn rules<T, F>(mut self, key: &str, build: F) -> Self
    where
        T: Serialize,
        F: FnOnce(&C) -> Rules<T>,
    {
        match Plan::new(&build(self.cx)) {
            Ok(plan) => self.built.rules.push((key.to_owned(), Arc::new(plan))),
            Err(reason) => {
                self.built
                    .error
                    .get_or_insert_with(|| format!("`{key}`: {reason}"));
            }
        }
        self
    }

    /// Add a computed field written under `key`: `compute` reads whatever its
    /// first argument deserializes from the document.
    #[must_use]
    pub fn computed<I, T, F>(mut self, key: &str, compute: F) -> Self
    where
        C: Clone + Send + Sync + 'static,
        I: DeserializeOwned,
        T: Serialize,
        F: Fn(&I, &C) -> T + Send + Sync + 'static,
    {
        let cx = Arc::clone(self.shared.get_or_insert_with(|| Arc::new(self.cx.clone())));
        let key = key.to_owned();
        self.built
            .computed
            .push(Arc::new(move |source: &mut Value| {
                let input = I::deserialize(&*source).map_err(|error| {
                    failed(&key, format_args!("its input does not read: {error}"))
                })?;
                let value = serde_json::to_value(compute(&input, &cx)).map_err(|error| {
                    failed(&key, format_args!("its value does not serialize: {error}"))
                })?;
                if let Some(object) = source.as_object_mut() {
                    object.insert(key.clone(), value);
                }
                Ok(())
            }));
        self
    }

    /// The finished computation.
    #[must_use]
    pub fn finish(self) -> Computation {
        self.built
    }
}
