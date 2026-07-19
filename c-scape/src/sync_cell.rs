//! `core::cell::SyncUnsafeCell` is still unstable (rust#95439); this is
//! the same ten lines, locally. delete when the std type stabilizes.
use core::cell::UnsafeCell;

#[repr(transparent)]
pub struct SyncUnsafeCell<T: ?Sized>(UnsafeCell<T>);

unsafe impl<T: ?Sized> Sync for SyncUnsafeCell<T> {}

impl<T> SyncUnsafeCell<T> {
    pub const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }
}

impl<T: ?Sized> SyncUnsafeCell<T> {
    pub const fn get(&self) -> *mut T {
        self.0.get()
    }
}
