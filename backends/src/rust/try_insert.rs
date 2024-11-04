// smoelius: Reevaluate whether this is the right approach. For an alternative, see
// `core::SourceFile::new`. It avoids `entry(key.clone())`.

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

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;
    use std::{collections::BTreeMap, path::PathBuf};

    #[test]
    fn or_try_insert_with() {
        let mut map = BTreeMap::new();
        let path_buf = PathBuf::from("/");
        let _: &mut bool = map
            .entry(path_buf.clone())
            .or_try_insert_with(|| -> Result<bool> { Ok(true) })
            .unwrap();

        // smoelius: Ensure `path_buf` is in `map`.
        assert!(map.contains_key(&path_buf));

        // smoelius: Ensure a second call to `or_try_insert_with` does not invoke the closure.
        let _: &mut bool = map
            .entry(path_buf)
            .or_try_insert_with(|| -> Result<bool> { panic!() })
            .unwrap();
    }
}
