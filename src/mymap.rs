use std::ops::{Deref, DerefMut};

#[derive(Clone)]
pub struct MyMap<K, V> {
    inner: Vec<(K, V)>,
}

impl<K: PartialEq, V> MyMap<K, V> {
    pub fn get_or_init(&mut self, k: K, f: impl FnOnce() -> V) -> &V {
        if let Some(res) = self.inner.iter().position(|x| x.0 == k) {
            return &self.inner[res].1;
        }
        self.inner.push((k, f()));
        &self.inner.last().unwrap().1
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
    type Target = Vec<(K, V)>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<K, V> DerefMut for MyMap<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
