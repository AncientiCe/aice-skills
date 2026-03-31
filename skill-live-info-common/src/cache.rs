use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Debug)]
pub struct CachedValue<V> {
    pub value: V,
    pub as_of: SystemTime,
    fresh_until: Instant,
    stale_until: Instant,
}

impl<V> CachedValue<V> {
    pub fn is_fresh(&self) -> bool {
        Instant::now() <= self.fresh_until
    }

    pub fn is_stale_eligible(&self) -> bool {
        Instant::now() <= self.stale_until
    }
}

#[derive(Debug)]
pub struct TtlCache<K, V> {
    entries: Mutex<HashMap<K, CachedValue<V>>>,
}

impl<K, V> Default for TtlCache<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K, V> TtlCache<K, V> {
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }
}

impl<K, V> TtlCache<K, V>
where
    K: Eq + Hash,
    V: Clone,
{
    pub fn get_fresh(&self, key: &K) -> Option<CachedValue<V>> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;
        if entry.is_fresh() {
            Some(entry.clone())
        } else {
            None
        }
    }

    pub fn get_stale_eligible(&self, key: &K) -> Option<CachedValue<V>> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;
        if entry.is_stale_eligible() {
            Some(entry.clone())
        } else {
            None
        }
    }

    pub fn insert(
        &self,
        key: K,
        value: V,
        fresh_ttl: Duration,
        stale_ttl: Duration,
    ) -> CachedValue<V> {
        let now = Instant::now();
        let stale_ttl = stale_ttl.max(fresh_ttl);
        let cached = CachedValue {
            value,
            as_of: SystemTime::now(),
            fresh_until: now + fresh_ttl,
            stale_until: now + stale_ttl,
        };
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(key, cached.clone());
        }
        cached
    }
}
