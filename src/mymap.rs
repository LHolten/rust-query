use std::ops::Deref;

use elsa::FrozenVec;

#[derive(Clone)]
pub struct MyMap<K, V> {
    inner: FrozenVec<Box<(K, V)>>,
}

impl<K: PartialEq, V> MyMap<K, V> {
    pub fn get_or_init(&self, k: K, f: impl FnOnce() -> V) -> &V {
        if let Some(res) = self.inner.iter().find(|x| x.0 == k) {
            return &res.1;
        }
        &self.inner.push_get(Box::new((k, f()))).1
    }
}

impl<K, V> Default for MyMap<K, V> {
    fn default() -> Self {
        Self {
            inner: Default::default(),
        }
    }
}

impl<K, V> Deref for MyMap<K, V> {
    type Target = FrozenVec<Box<(K, V)>>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
