// TODO: This will probably need to move into the Common library,
// or at least some version of it.

use core::{sync::atomic::{AtomicU8, AtomicBool, Ordering, AtomicPtr}, ops::{Deref, DerefMut}, ptr::null_mut};

use crate::alloc::HeapBox;

pub mod status {
    /// Kernel is working, and should be allowed exclusive access,
    /// if it doesn't have it already.
    pub const KERNEL_ACCESS: u8 = 0;

    /// Userspace is working, and should be allowed exclusive access,
    /// if it doesn't have it already.
    pub const USERSPACE_ACCESS: u8 = 1;

    /// The future has completed (on either side), but the payload
    /// is no longer accessible.
    pub const COMPLETED: u8 = 2;

    /// This future encountered an error, and will never reach the
    /// completed stage. The payload is no longer accessible.
    pub const ERROR: u8 = 3;

    /// Used to signify a handle that will only ever pend error or completed
    pub const INVALID: u8 = 4;
}

// ------------------ | FUTURE BOX | ------------------------

// This gets leaked
#[repr(C)]
pub struct FutureBox<T> {
    // TODO: Should these fields be one atomic u32?

    // Current status. Should only be updated by the holder of
    // the exclusive token
    status: AtomicU8,

    // Reference count, including exclusive and shared handles
    refcnt: AtomicU8,

    // Is the exclusive handle taken?
    ex_taken: AtomicBool,

    // TODO: This is a HeapBox<T>.
    payload: AtomicPtr<T>,
}

impl<T> Drop for FutureBoxExHdl<T> {
    fn drop(&mut self) {
        let drop_fb = {
            let fb = unsafe { &*self.fb };
            let pre_refs = fb.refcnt.fetch_sub(1, Ordering::SeqCst);

            // TODO(AJM): I don't think we should ever just "drop" an exclusive handle
            // For now, always mark the state as ERROR and drop the payload in this
            // case.
            fb.status.store(status::ERROR, Ordering::SeqCst);
            // Go ahead and drop the payload
            let _ = unsafe { HeapBox::from_leaked(self.payload) };
            fb.payload.store(null_mut(), Ordering::SeqCst);

            // Release our exlusive status
            fb.ex_taken.store(false, Ordering::SeqCst);
            debug_assert!(pre_refs != 0);
            pre_refs <= 1
        };

        // Split off, to avoid reference to self.fb being live
        // SAFETY: This arm only executes if we were the LAST handle to know
        // about this futurebox.
        if drop_fb {
            // We are responsible for dropping the payload, and the futurebox
            if self.payload != null_mut() {
                let _ = unsafe { HeapBox::from_leaked(self.payload) };
            }
            let _ = unsafe { HeapBox::from_leaked(self.fb) };
        }
    }
}

// This represents shared access to the FutureBox, and
// exclusive access to the payload
pub struct FutureBoxExHdl<T> {
    fb: *mut FutureBox<T>,
    // Store the payload handle here, so we don't have to double deref
    payload: *mut T,
}

impl<T> FutureBoxExHdl<T> {
    // TODO: I might want methods at some point that get BACK a handle too.
    // Example: using a single buffer for Transfer traits. For now, just expect the user
    // to allocate two buffers in that case.
    fn convert_to_monitor(self) -> FutureBoxPendHdl<T> {
        let ret = FutureBoxPendHdl {
            fb: self.fb,
            awaiting: status::INVALID,
        };
        // Forget the ex handle, so we don't mess with the refcounts
        core::mem::forget(self);
        ret
    }

    pub fn release_to_userspace(self) -> FutureBoxPendHdl<T> {
        {
            let fb = unsafe { &*self.fb };
            fb.status.store(status::USERSPACE_ACCESS, Ordering::SeqCst);
            fb.ex_taken.store(false, Ordering::SeqCst);
        }
        self.convert_to_monitor()
    }

    pub fn release_to_kernel(self) -> FutureBoxPendHdl<T> {
        {
            let fb = unsafe { &*self.fb };
            fb.status.store(status::KERNEL_ACCESS, Ordering::SeqCst);
            fb.ex_taken.store(false, Ordering::SeqCst);
        }
        self.convert_to_monitor()
    }

    pub fn release_to_error(self) {
        let fb = unsafe { &*self.fb };
        fb.status.store(status::ERROR, Ordering::SeqCst);
        // Go ahead and drop the payload
        let _ = unsafe { HeapBox::from_leaked(self.payload) };
        fb.payload.store(null_mut(), Ordering::SeqCst);

        fb.ex_taken.store(false, Ordering::SeqCst);
    }

    pub fn release_to_complete(self) {
        let fb = unsafe { &*self.fb };
        fb.status.store(status::ERROR, Ordering::SeqCst);
        // Go ahead and drop the payload
        let _ = unsafe { HeapBox::from_leaked(self.payload) };
        fb.payload.store(null_mut(), Ordering::SeqCst);

        fb.ex_taken.store(false, Ordering::SeqCst);
    }
}

impl<T> Deref for FutureBoxExHdl<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: We have exclusive access for as long as this handle exists
        unsafe {
            &*self.payload
        }
    }
}

impl<T> DerefMut for FutureBoxExHdl<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: We have exclusive access for as long as this handle exists
        unsafe {
            &mut *self.payload
        }
    }
}

// This represents shared access to the FutureBox, and
// NO access to the payload
pub struct FutureBoxPendHdl<T> {
    fb: *mut FutureBox<T>,
    awaiting: u8,
}

impl<T> FutureBoxPendHdl<T> {
    pub fn is_complete(&self) -> Result<bool, ()> {
        let fb = unsafe { &*self.fb };
        match fb.status.load(Ordering::SeqCst) {
            status::COMPLETED => Ok(true),
            status::ERROR => Err(()),
            _ => Ok(false),
        }
    }

    pub fn try_upgrade(&self) -> Result<Option<FutureBoxExHdl<T>>, ()> {
        let fb = unsafe { &*self.fb };
        let was_ex = fb.ex_taken.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst);
        match was_ex {
            Ok(_) => {
                // We have exclusive access, see if we are in the right mode
                match fb.status.load(Ordering::SeqCst) {
                    status::ERROR => {
                        // It's never gunna work out...
                        fb.ex_taken.store(false, Ordering::SeqCst);
                        return Err(());
                    }
                    n if n == self.awaiting => {
                        // Yup!
                        let fbeh = FutureBoxExHdl {
                            fb: self.fb,
                            payload: fb.payload.load(Ordering::SeqCst),
                        };
                        fb.refcnt.fetch_add(1, Ordering::SeqCst);
                        Ok(Some(fbeh))
                    }
                    _ => {
                        // Nope. Release exclusive access
                        fb.ex_taken.store(false, Ordering::SeqCst);
                        Ok(None)
                    }
                }
            }
            Err(_) => {
                // It failed. Someone else has exclusive access.
                return Ok(None);
            }
        }
    }
}

// ------------------ | FUTURE ARRAY | ------------------------
// TODO
