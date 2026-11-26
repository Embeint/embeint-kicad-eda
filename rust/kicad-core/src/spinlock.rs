// SPDX-License-Identifier: GPL-3.0-or-later
//! Port of `libs/core/include/core/spinlock.h`: `KISPINLOCK`, a trivial spinlock over
//! an atomic flag.
//!
//! Portable code has no threads, so a lock has exactly one user at a time and the
//! acquire/release orderings have nothing to order. The flag operations are ported as
//! written. `lock()` on a held lock spins until another thread unlocks it; with no
//! other thread that never happens, so the port refuses it with
//! [`SpinLockError::WouldSpinForever`] instead of looping. A host that shares a lock
//! between threads uses its own synchronisation primitive.

/// `KISPINLOCK` failures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpinLockError {
    /// `lock()` on a held lock with no other thread to release it: the reference
    /// never returns.
    WouldSpinForever,
}

/// `KISPINLOCK`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct KiSpinLock {
    lock: bool,
}

impl KiSpinLock {
    /// `KISPINLOCK()`: unlocked.
    pub fn new() -> Self {
        Self { lock: false }
    }

    /// `lock()`: `while( m_lock.exchange( true ) );`.
    ///
    /// # Errors
    /// [`SpinLockError::WouldSpinForever`] when already held; nothing changes.
    pub fn lock(&mut self) -> Result<(), SpinLockError> {
        if core::mem::replace(&mut self.lock, true) {
            return Err(SpinLockError::WouldSpinForever);
        }
        Ok(())
    }

    /// `try_lock()`: `!m_lock.exchange( true )`.
    pub fn try_lock(&mut self) -> bool {
        !core::mem::replace(&mut self.lock, true)
    }

    /// `unlock()`: `m_lock.store( false )`, held or not.
    pub fn unlock(&mut self) {
        self.lock = false;
    }

    /// `test()`: `m_lock.load()`.
    pub fn test(&self) -> bool {
        self.lock
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exchange_semantics() {
        let mut lock = KiSpinLock::new();
        assert!(!lock.test());
        assert_eq!(lock.lock(), Ok(()));
        assert!(!lock.try_lock());
        assert_eq!(lock.lock(), Err(SpinLockError::WouldSpinForever));
        assert!(lock.test());
        lock.unlock();
        lock.unlock();
        assert!(lock.try_lock());
    }
}
