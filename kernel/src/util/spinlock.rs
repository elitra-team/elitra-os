
use core::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    data: core::cell::UnsafeCell<T>,
}

unsafe impl<T: Send> Send for SpinLock<T> {}
unsafe impl<T: Send> Sync for SpinLock<T> {}

pub struct SpinLockGuard<'a, T> {
    lock: &'a SpinLock<T>,
    flags: u64,
}

impl<'a, T> Drop for SpinLockGuard<'a, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
        unsafe {
            core::arch::asm!(
                "push {flags}",
                "popfq",
                flags = in(reg) self.flags,
                options(nostack),
            );
        }
    }
}

impl<'a, T> core::ops::Deref for SpinLockGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<'a, T> core::ops::DerefMut for SpinLockGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> SpinLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: core::cell::UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinLockGuard<T> {
        let flags: u64;
        unsafe {
            core::arch::asm!("pushfq; pop {}; cli", out(reg) flags, options(nostack));
        }
        while self.locked.compare_exchange_weak(
            false, true,
            Ordering::Acquire, Ordering::Relaxed,
        ).is_err() {
            core::hint::spin_loop();
        }
        SpinLockGuard { lock: self, flags }
    }

    pub fn try_lock(&self) -> Option<SpinLockGuard<T>> {
        let flags: u64;
        unsafe {
            core::arch::asm!("pushfq; pop {}", out(reg) flags, options(nostack));
        }
        if self.locked.compare_exchange(
            false, true,
            Ordering::Acquire, Ordering::Relaxed,
        ).is_ok() {
            Some(SpinLockGuard { lock: self, flags })
        } else {
            None
        }
    }
}
