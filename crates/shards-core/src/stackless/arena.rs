//! Generation-checked frame addresses. No pointers into the arena escape a borrow.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Handle {
  index: usize,
  generation: u64,
}

struct Slot<T> {
  generation: u64,
  value: Option<T>,
}

pub(crate) struct Arena<T> {
  slots: Vec<Slot<T>>,
  free: Vec<usize>,
}

impl<T> Default for Arena<T> {
  fn default() -> Self {
    Self {
      slots: Vec::new(),
      free: Vec::new(),
    }
  }
}

impl<T> Arena<T> {
  /// Reserve a known subtree without a transient doubling allocation.
  pub fn reserve(&mut self, additional: usize) {
    self
      .slots
      .reserve_exact(additional.saturating_sub(self.free.len()));
  }

  pub fn insert(&mut self, value: T) -> Handle {
    let index = self.free.pop().unwrap_or_else(|| {
      self.slots.push(Slot {
        generation: 0,
        value: None,
      });
      self.slots.len() - 1
    });
    let slot = &mut self.slots[index];
    slot.value = Some(value);
    Handle {
      index,
      generation: slot.generation,
    }
  }

  pub fn get(&self, handle: Handle) -> Option<&T> {
    let slot = self.slots.get(handle.index)?;
    (slot.generation == handle.generation)
      .then_some(slot.value.as_ref())
      .flatten()
  }

  pub fn get_mut(&mut self, handle: Handle) -> Option<&mut T> {
    let slot = self.slots.get_mut(handle.index)?;
    (slot.generation == handle.generation)
      .then_some(slot.value.as_mut())
      .flatten()
  }

  pub fn remove(&mut self, handle: Handle) -> Option<T> {
    let slot = self.slots.get_mut(handle.index)?;
    if slot.generation != handle.generation {
      return None;
    }
    let value = slot.value.take()?;
    // A generation can never wrap and revive a stale handle. Retire on overflow.
    if let Some(next) = slot.generation.checked_add(1) {
      slot.generation = next;
      self.free.push(handle.index);
    }
    Some(value)
  }

  pub fn values(&self) -> impl Iterator<Item = &T> {
    self.slots.iter().filter_map(|s| s.value.as_ref())
  }

  pub fn capacity_bytes(&self) -> usize {
    self.slots.capacity() * std::mem::size_of::<Slot<T>>()
      + self.free.capacity() * std::mem::size_of::<usize>()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn stale_handles_cannot_read_mutate_or_remove_reused_slots() {
    let mut arena = Arena::default();
    let old = arena.insert(String::from("old"));
    assert_eq!(arena.remove(old).as_deref(), Some("old"));
    // Moving the backing allocation must not revive the removed handle.
    arena.reserve(128);
    let new = arena.insert(String::from("new"));
    assert_eq!(old.index, new.index);
    assert!(arena.get(old).is_none());
    assert!(arena.get_mut(old).is_none());
    assert!(arena.remove(old).is_none());
    assert_eq!(arena.get(new).map(String::as_str), Some("new"));
    for _ in 0..1000 {
      let h = arena.insert(vec![1u8; 64].len().to_string());
      arena.remove(h).unwrap();
      assert!(arena.get(h).is_none());
    }
    assert_eq!(arena.values().count(), 1);
  }

  #[test]
  fn exhausted_generation_retires_slot() {
    let mut arena = Arena::default();
    let mut h = arena.insert(7);
    arena.slots[h.index].generation = u64::MAX;
    h.generation = u64::MAX;
    assert_eq!(arena.remove(h), Some(7));
    let fresh = arena.insert(8);
    assert_ne!(fresh.index, h.index);
    assert!(arena.get(h).is_none());
  }
}
