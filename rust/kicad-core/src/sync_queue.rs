// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/core/include/core/sync_queue.h`: `SYNC_QUEUE<T>`, a first-in first-out
//! queue whose every operation holds a mutex.
//!
//! Portable code has no threads: the queue's owner has exclusive (`&mut`) access, so
//! the mutex has nothing to exclude and is not modelled. The queue operations are
//! ported as written over a `std::queue` (`std::deque`) equivalent. A host sharing a
//! queue between threads wraps it in its own lock.

use std::collections::VecDeque;

/// The queue could not grow (the reference throws `bad_alloc`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AllocationError;

/// `SYNC_QUEUE<T>`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SyncQueue<T> {
    queue: VecDeque<T>,
}

impl<T> SyncQueue<T> {
    /// `SYNC_QUEUE()`: empty.
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    /// `push( T const& aValue )`: append a copy.
    ///
    /// # Errors
    /// [`AllocationError`]; the queue is then unchanged.
    pub fn push(&mut self, value: &T) -> Result<(), AllocationError>
    where
        T: Clone,
    {
        self.move_push(value.clone())
    }

    /// `move_push( T&& aValue )`: append the value itself.
    ///
    /// # Errors
    /// [`AllocationError`]; the queue is then unchanged.
    pub fn move_push(&mut self, value: T) -> Result<(), AllocationError> {
        self.queue.try_reserve(1).map_err(|_| AllocationError)?;
        self.queue.push_back(value);
        Ok(())
    }

    /// `pop( T& aReceiver )`: move the front value into `receiver` and remove it.
    /// An empty queue leaves `receiver` untouched. Returns true if a value was popped.
    pub fn pop(&mut self, receiver: &mut T) -> bool {
        match self.queue.pop_front() {
            None => false,
            Some(front) => {
                *receiver = front;
                true
            }
        }
    }

    /// `empty()`.
    pub fn empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// `size()` (`size_t`).
    pub fn size(&self) -> u64 {
        self.queue.len() as u64
    }

    /// `clear()`: pop (destroy) values from the front until empty.
    pub fn clear(&mut self) {
        while self.queue.pop_front().is_some() {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fifo_and_untouched_receiver() -> Result<(), AllocationError> {
        let mut queue = SyncQueue::new();
        let mut receiver = String::from("keep");
        assert!(!queue.pop(&mut receiver));
        assert_eq!(receiver, "keep");
        queue.push(&String::from("a"))?;
        queue.move_push(String::from("b"))?;
        assert_eq!(queue.size(), 2);
        assert!(queue.pop(&mut receiver));
        assert_eq!(receiver, "a");
        queue.clear();
        assert!(queue.empty());
        Ok(())
    }
}
