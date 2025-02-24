// pulse - bitdrift's observability proxy
// Copyright Bitdrift, Inc. All rights reserved.
//
// Use of this source code is governed by a source available license that can be found in the
// LICENSE file or at:
// https://polyformproject.org/wp-content/uploads/2020/06/PolyForm-Shield-1.0.0.txt

#[cfg(test)]
#[path = "./lru_map_test.rs"]
mod lru_map_test;

use hashbrown::HashMap;
use hashbrown::hash_map::RawEntryMut;
use intrusive_collections::{LinkedList, LinkedListLink, UnsafeRef, intrusive_adapter};
use parking_lot::{Mutex, RwLock, RwLockReadGuard};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};
use std::marker::PhantomPinned;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

//
// MapKey
//

// Holds both the key and the LRU list link. Implements PartialEq, Eq, and Hash if K implements it.
#[derive(Debug)]
pub struct MapKey<K> {
  key: K,
  link: LinkedListLink,
  _pinned: PhantomPinned,
}

impl<K> MapKey<K> {
  const fn new(key: K) -> Self {
    Self {
      key,
      link: LinkedListLink::new(),
      _pinned: PhantomPinned,
    }
  }
}

impl<K: PartialEq> PartialEq for MapKey<K> {
  fn eq(&self, other: &Self) -> bool {
    self.key.eq(&other.key)
  }
}

impl<K: Eq> Eq for MapKey<K> {}

impl<K: Hash> Hash for MapKey<K> {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.key.hash(state);
  }
}

// Safety: The link is only manipulated under a full mutex so this structure is Sync.
unsafe impl<K: Sync> Sync for MapKey<K> {}

intrusive_adapter!(LruAdapter<K> = UnsafeRef<MapKey<K>>: MapKey<K> { link: LinkedListLink });

//
// LruShard
//

// Holds an individual map shard including the real map as well as the LRU list.
#[derive(Debug)]
struct LruShard<K, V, S = RandomState> {
  map: RwLock<HashMap<Box<MapKey<K>>, V, S>>,
  lru: Mutex<LinkedList<LruAdapter<K>>>,
}

impl<K, V, S> Drop for LruShard<K, V, S> {
  fn drop(&mut self) {
    // Clear the linked list to avoid walking the entire list for no reason. Additionally, this
    // makes it so that we don't have drop order guarantees between the list and the map.
    self.lru.lock().fast_clear();
  }
}

impl<K, V, S> LruShard<K, V, S> {
  // Create a new shard with a specific hasher.
  fn with_hasher(hash_builder: S) -> Self {
    Self {
      map: RwLock::new(HashMap::with_hasher(hash_builder)),
      lru: Mutex::new(LinkedList::new(LruAdapter::new())),
    }
  }

  // Given a key in the map, move it to most recently used.
  fn adjust_lru(&self, key: *const MapKey<K>) {
    let mut lru = self.lru.lock();
    let mut entry = unsafe {
      // Safety: The key is in the map and already initialized in the LRU.
      lru.cursor_mut_from_ptr(key)
    };
    let entry = entry.remove().unwrap();
    lru.push_back(entry);
  }
}

//
// Ref
//
// Wraps a read-only reference to a map entry. An attempt was made to implement this using the
// parking_lot mapping functionality but it doesn't appear possible to return an aggregate
// structure with multiple references, thus the use of unsafe code like the other libraries.
pub struct Ref<'a, K, V, S> {
  _guard: RwLockReadGuard<'a, HashMap<Box<MapKey<K>>, V, S>>,
  key: *const K,
  value: *const V,
}

impl<K, V, S> Ref<'_, K, V, S> {
  #[must_use]
  pub const fn key(&self) -> &K {
    unsafe {
      // Safety: We hold a read lock to the map so nothing can change.
      &*self.key
    }
  }

  #[must_use]
  pub const fn value(&self) -> &V {
    unsafe {
      // Safety: We hold a read lock to the map so nothing can change.
      &*self.value
    }
  }
}

//
// Equivalent
//

// Allows heterogeneous lookup in the LRU map. There are no blanket implementations currently so
// it must be manually implemented for all used keys.
pub trait Equivalent<T> {
  fn equivalent(&self, other: &T) -> bool;
}

//
// LruMap
//

// Implements a sharded LRU map. The implementation is inspired by DashMap.
#[derive(Debug)]
pub struct LruMap<K, V, S = RandomState> {
  shift: usize,
  hash_builder: S,
  shards: Vec<LruShard<K, V, S>>,
  len: AtomicUsize,
}

impl<K: Eq + Hash, V, S: BuildHasher + Clone> LruMap<K, V, S> {
  // Copied from dashmap.
  fn default_shard_amount() -> usize {
    static DEFAULT_SHARD_AMOUNT: OnceLock<usize> = OnceLock::new();
    *DEFAULT_SHARD_AMOUNT.get_or_init(|| {
      (std::thread::available_parallelism().map_or(1, usize::from) * 4).next_power_of_two()
    })
  }

  // Copied from dashmap.
  const fn ptr_size_bits() -> usize {
    std::mem::size_of::<usize>() * 8
  }

  // Copied from dashmap.
  const fn ncb(shard_amount: usize) -> usize {
    shard_amount.trailing_zeros() as usize
  }

  // Create a new map with a specific hasher.
  pub fn with_hasher(hash_builder: S) -> Self {
    Self::with_hasher_and_shards(hash_builder, Self::default_shard_amount())
  }

  // Create a new map with a specific hasher and number of shards.
  pub fn with_hasher_and_shards(hash_builder: S, shards: usize) -> Self {
    assert!(shards.is_power_of_two());
    let shards: Vec<_> = (0 .. shards)
      .map(|_| LruShard::with_hasher(hash_builder.clone()))
      .collect();

    Self {
      shift: Self::ptr_size_bits() - Self::ncb(shards.len()),
      hash_builder,
      shards,
      len: AtomicUsize::new(0),
    }
  }

  // Generate a hash for a key.
  fn hash_key<Q: Hash>(&self, key: &Q) -> u64
  where
    K: Equivalent<Q>,
  {
    self.hash_builder.hash_one(key)
  }

  // Fetch a shard.
  fn determine_shard(&self, hash: u64) -> &LruShard<K, V, S> {
    // TODO(mattklein123): The dashmap code first shifts off the left 7 bits, before shifting right,
    // which makes no sense to me so I removed it. The checked shift is required for the single
    // shard case for testing. Perhaps this could be optimized some other way later.
    let index = hash.checked_shr(self.shift as u32).unwrap_or(0) as usize;
    unsafe { self.shards.get_unchecked(index) }
  }

  // Fetch a map value under read lock.
  pub fn get<Q: Eq + Hash>(&self, key: &Q) -> Option<Ref<'_, K, V, S>>
  where
    K: Equivalent<Q>,
  {
    let hash = self.hash_key(key);
    let shard = self.determine_shard(hash);
    let map = shard.map.read();
    if let Some((key, value)) = map
      .raw_entry()
      .from_hash(hash, |key_to_match| key_to_match.key.equivalent(key))
    {
      shard.adjust_lru(key.as_ref());
      let key: *const K = &key.key;
      let value: *const V = value;
      Some(Ref {
        _guard: map,
        key,
        value,
      })
    } else {
      None
    }
  }

  // Under write lock, get or insert a value, and returned a mapped result given whether the
  // key already existed in the map. Additionally, if a value was inserted, attempt to evict the
  // least recently used entry.
  // TODO(mattklein123): This is a clunky replacement for the entry() API. That was deferred
  // because it ends up being tricky due to all of the wrapping involved. We can consider this
  // later.
  pub fn get_or_insert_map<ReturnType>(
    &self,
    key: K,
    make_value_func: impl FnOnce() -> V,
    existed_func: impl FnOnce(&mut K, &mut V) -> ReturnType,
    inserted_func: impl FnOnce(&mut K, &mut V) -> ReturnType,
    evict_func: impl Fn(&mut K, &mut V) -> bool,
  ) -> ReturnType
  where
    K: Equivalent<K>,
  {
    let hash = self.hash_key(&key);
    let shard = self.determine_shard(hash);
    let mut map = shard.map.write();
    let was_empty = map.is_empty();

    let (result, try_eviction) = match map
      .raw_entry_mut()
      .from_hash(hash, |key_to_match| key_to_match.key == key)
    {
      RawEntryMut::Occupied(mut e) => {
        shard.adjust_lru(e.key().as_ref());
        let (key, value) = e.get_key_value_mut();
        (existed_func(&mut key.key, value), false)
      },
      RawEntryMut::Vacant(v) => {
        let key = Box::new(MapKey::new(key));
        let mut lru = shard.lru.lock();
        lru.push_back(unsafe {
          // Safety: Ownership of the list entry is within the map, so we must use UnsafeRef for
          // the list itself.
          UnsafeRef::from_raw(key.as_ref())
        });
        let (key, value) = v.insert_hashed_nocheck(hash, key, make_value_func());
        self.len.fetch_add(1, Ordering::SeqCst);
        (inserted_func(&mut key.key, value), true)
      },
    };

    // In order to amortize eviction with inserts, every time we insert we see if we need to
    // evict. For every potential insert, we evict up to 2 entries. This is because during
    // cache warm up we may add a lot of entries with no evictions, and then at steady state we will
    // never be able to "burn down" the stale entries.
    // TODO(mattklein123): This is not necessarily the best thing to do (for example we could
    // wait until we insert X elements and then try to clean X elements, wait until the map is
    // above some size threshold, etc.), but it's clearly better than an async sweep loop that
    // operates over giant maps so we can start here.
    if !was_empty && try_eviction {
      let mut lru = shard.lru.lock();
      Self::try_evict(&mut map, &mut lru, &self.len, evict_func, Some(2));
    }

    result
  }

  // Get the length of the map.
  pub fn len(&self) -> usize {
    // Note: The following debug assert will need to be commented out if running any stress tests
    // with multiple threads in debug mode.
    debug_assert_eq!(
      self.len.load(Ordering::SeqCst),
      self
        .shards
        .iter()
        .map(|map| map.map.read().len())
        .sum::<usize>()
    );
    self.len.load(Ordering::SeqCst)
  }

  // See if the map is empty.
  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }

  // Attempt to evict entries from the LRU.
  fn try_evict(
    map: &mut HashMap<Box<MapKey<K>>, V, S>,
    lru: &mut LinkedList<LruAdapter<K>>,
    len: &AtomicUsize,
    evict_func: impl Fn(&mut K, &mut V) -> bool,
    max_evictions: Option<u64>,
  ) -> u64 {
    let mut num_evictions = 0;
    while let Some(front) = lru.front().get() {
      let oldest_entry = map.raw_entry_mut().from_key(front);
      if let RawEntryMut::Occupied(mut e) = oldest_entry {
        let (key, value) = e.get_key_value_mut();
        if evict_func(&mut key.key, value) {
          lru.pop_front();
          e.remove();
          debug_assert!(len.load(Ordering::SeqCst) > 0);
          len.fetch_sub(1, Ordering::SeqCst);
          num_evictions += 1;

          if max_evictions.is_some_and(|max_evictions| num_evictions >= max_evictions) {
            break;
          }
        } else {
          break;
        }
      } else {
        unreachable!()
      }
    }

    num_evictions
  }

  // Iterate LRU entries in each shard, attempting to evict them using an eviction function.
  // Iteration stops when the eviction function returns false under the assumption that no further
  // evictions will succeed at this time.
  pub fn try_evict_lru(&self, evict_func: impl Fn(&mut K, &mut V) -> bool) -> u64 {
    let mut num_evictions = 0;
    for shard in &self.shards {
      let mut map = shard.map.write();
      let mut lru = shard.lru.lock();
      num_evictions += Self::try_evict(&mut map, &mut lru, &self.len, &evict_func, None);
    }
    num_evictions
  }

  // Iterate over every entry in the map.
  // TODO(mattklein123): This was simpler than implementing a real iterator. We can consider that
  // in the future. Calling this function should be removed anyway per the other comment at the
  // only call site.
  pub fn iterate(&self, mut iterate_func: impl FnMut(&K, &V)) {
    for shard in &self.shards {
      for (key, value) in &*shard.map.read() {
        iterate_func(&key.key, value);
      }
    }
  }
}
