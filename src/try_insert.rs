use std::collections::btree_map::Entry;

pub(crate) trait TryInsert<'a, V, E> {
    fn or_try_insert_with<F>(self, default: F) -> Result<&'a mut V, E>
    where
        F: FnOnce() -> Result<V, E>;
}

impl<'a, K: Ord, V, E> TryInsert<'a, V, E> for Entry<'a, K, V> {
    fn or_try_insert_with<F>(self, default: F) -> Result<&'a mut V, E>
    where
        F: FnOnce() -> Result<V, E>,
    {
        match self {
            Entry::Occupied(entry) => Ok(entry.into_mut()),
            Entry::Vacant(entry) => {
                let value = default()?;
                Ok(entry.insert(value))
            }
        }
    }
}
