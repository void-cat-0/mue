//! Sparse storage for declared style properties.

use std::any::Any;

use mue_core::prop::Prop;

/// Declared properties as `(key, Box<Prop<T>>)` pairs.
///
/// A style usually declares a handful of the ~100 properties, and builders move
/// their style by value through deeply nested expressions. Storing only the
/// declared ones keeps `Style` small instead of growing with every property,
/// which matters for stack use in debug builds.
#[derive(Default)]
pub(crate) struct PropStore {
    entries: Vec<(u16, Box<dyn Any>)>,
}

impl PropStore {
    fn position(&self, key: u16) -> Option<usize> {
        self.entries.iter().position(|(k, _)| *k == key)
    }

    pub(crate) fn contains(&self, key: u16) -> bool {
        self.position(key).is_some()
    }

    pub(crate) fn get<T: Clone + 'static>(&self, key: u16) -> Option<Prop<T>> {
        let (_, value) = &self.entries[self.position(key)?];
        Some(
            value
                .downcast_ref::<Prop<T>>()
                .expect("style property stored with its declared type")
                .clone(),
        )
    }

    pub(crate) fn take<T: 'static>(&mut self, key: u16) -> Option<Prop<T>> {
        let (_, value) = self.entries.swap_remove(self.position(key)?);
        Some(
            *value
                .downcast::<Prop<T>>()
                .expect("style property stored with its declared type"),
        )
    }

    pub(crate) fn set<T: 'static>(&mut self, key: u16, value: Prop<T>) {
        let value: Box<dyn Any> = Box::new(value);
        match self.position(key) {
            Some(index) => self.entries[index].1 = value,
            None => self.entries.push((key, value)),
        }
    }

    /// Move the entries `self` lacks out of `other`; `other` keeps the rejected ones.
    pub(crate) fn merge_missing(&mut self, other: &mut Self) {
        let mut index = 0;
        while index < other.entries.len() {
            if self.contains(other.entries[index].0) {
                index += 1;
            } else {
                self.entries.push(other.entries.swap_remove(index));
            }
        }
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}
