//! What a script sees of a level: the document converted with [`to_dynamic`], plus the one
//! place the schema knows more than the value does. A `map` with `values: date` or
//! `values: timestamp` holds JSON text; its values reach a script typed (see
//! [`crate::input::temporal`]), while the document written to the sink keeps the text.
//!
//! A [`View`] is the part of a level's schema where that applies: those maps, and the
//! children that hold more of them. An index without such a map has empty views, and an
//! empty view converts exactly as [`to_dynamic`] does.

use std::collections::BTreeMap;

use kernel::{Field, FieldSource, FlussoType, GenericValue, Relation};
use rhai::{Dynamic, Map};

use crate::input::temporal;
use crate::value::to_dynamic;

/// Where in a level a value needs the schema to reach a script typed.
#[derive(Debug, Default)]
pub(crate) struct View {
    /// Map fields whose values are dates or timestamps, with that type.
    maps: Vec<(String, FlussoType)>,
    /// Groups and joins holding more of them.
    children: Vec<(String, View)>,
}

impl View {
    /// The view of a level with `fields`.
    pub(crate) fn of(fields: &[Field]) -> Self {
        let mut view = Self::default();
        for field in fields {
            let name = field.field.as_ref();
            let child = match &field.source {
                FieldSource::Column(column) => {
                    if let FlussoType::Map { values } = &column.ty
                        && matches!(**values, FlussoType::Date | FlussoType::Timestamp)
                    {
                        view.maps.push((name.to_owned(), (**values).clone()));
                    }
                    continue;
                }
                FieldSource::Group(children) => Self::of(children),
                FieldSource::Relation(Relation::Join(join)) => Self::of(&join.fields),
                FieldSource::Script(_)
                | FieldSource::Geo(_)
                | FieldSource::Relation(Relation::Aggregate(_))
                | FieldSource::Constant(_) => continue,
            };
            if !child.is_empty() {
                view.children.push((name.to_owned(), child));
            }
        }
        view
    }

    fn is_empty(&self) -> bool {
        self.maps.is_empty() && self.children.is_empty()
    }

    /// A level's value: an object, an array of them, or anything else as it is.
    pub(crate) fn value(&self, value: &GenericValue) -> Dynamic {
        match value {
            GenericValue::Map(map) if !self.is_empty() => self.object(map),
            GenericValue::Array(items) if !self.is_empty() => {
                Dynamic::from_array(items.iter().map(|item| self.value(item)).collect())
            }
            other => to_dynamic(other),
        }
    }

    /// One object of this level.
    pub(crate) fn object(&self, map: &BTreeMap<String, GenericValue>) -> Dynamic {
        Dynamic::from_map(
            map.iter()
                .map(|(key, value)| (key.as_str().into(), self.field(key, value)))
                .collect::<Map>(),
        )
    }

    fn field(&self, key: &str, value: &GenericValue) -> Dynamic {
        if self.is_empty() {
            return to_dynamic(value);
        }
        if let Some((_, ty)) = self.maps.iter().find(|(name, _)| name == key) {
            return typed_map(value, ty);
        }
        match self.children.iter().find(|(name, _)| name == key) {
            Some((_, child)) => child.value(value),
            None => to_dynamic(value),
        }
    }
}

fn typed_map(value: &GenericValue, ty: &FlussoType) -> Dynamic {
    let GenericValue::Map(map) = value else {
        return to_dynamic(value);
    };
    Dynamic::from_map(
        map.iter()
            .map(|(key, value)| {
                let typed = match value {
                    GenericValue::String(text) => temporal(text, ty)
                        .map_or_else(|| to_dynamic(value), |typed| to_dynamic(&typed)),
                    other => to_dynamic(other),
                };
                (key.as_str().into(), typed)
            })
            .collect::<Map>(),
    )
}
