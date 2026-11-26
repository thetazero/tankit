//! The last few results of something slow to work out.

use std::sync::{Arc, Mutex, OnceLock};

/// The results for the last `N` keys asked for. Each is worked out only
/// once, however many threads ask for it together.
pub struct Memo<K, V, const N: usize> {
    /// The last asked for first.
    kept: Mutex<Vec<(K, Arc<OnceLock<V>>)>>,
}

impl<K: PartialEq + Clone, V: Clone, const N: usize> Memo<K, V, N> {
    pub const fn new() -> Self {
        Memo {
            kept: Mutex::new(Vec::new()),
        }
    }

    pub fn get(&self, key: &K, make: impl FnOnce() -> V) -> V {
        let slot = {
            let mut kept = self.kept.lock().unwrap();
            let found = kept.iter().position(|(k, _)| k == key);
            let entry = match found {
                Some(i) => kept.remove(i),
                None => (key.clone(), Arc::default()),
            };
            let slot = entry.1.clone();
            kept.insert(0, entry);
            kept.truncate(N);
            slot
        };
        // (Not under the lock: others can be worked out meanwhile.)
        slot.get_or_init(make).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn keeps_the_last_asked_for() {
        let memo: Memo<u32, u32, 2> = Memo::new();
        let made = Cell::new(0);
        let get = |k: u32| {
            memo.get(&k, || {
                made.set(made.get() + 1);
                k * 10
            })
        };
        assert_eq!([get(1), get(2), get(1), get(3)], [10, 20, 10, 30]);
        assert_eq!(made.get(), 3);
        // 2 went, asked for longest ago.
        assert_eq!((get(1), made.get()), (10, 3));
        assert_eq!((get(2), made.get()), (20, 4));
    }
}
