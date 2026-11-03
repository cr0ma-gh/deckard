//! Safe wrapper around the libretro disk control interface.

use std::ffi::{CStr, c_char, c_uint};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Information about the core's disk drive state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskInfo {
    /// Total number of available disks.
    pub count: usize,
    /// Currently inserted disk index (0-based).
    pub current: usize,
    /// Friendly label for the current disk, if provided by the core.
    pub label: Option<String>,
    /// Whether the virtual disk tray is currently open/ejected.
    pub ejected: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct DiskControl {
    pub set_eject_state: unsafe extern "C" fn(bool) -> bool,
    pub get_eject_state: unsafe extern "C" fn() -> bool,
    pub get_image_index: unsafe extern "C" fn() -> c_uint,
    pub set_image_index: unsafe extern "C" fn(c_uint) -> bool,
    pub get_num_images: unsafe extern "C" fn() -> c_uint,
    pub get_image_label: Option<unsafe extern "C" fn(c_uint, *mut c_char, usize) -> bool>,
}

impl DiskControl {
    pub fn count(&self) -> usize {
        let f = self.get_num_images;
        // SAFETY: pointer verified non-null when registering the callback.
        catch_unwind(|| unsafe { f() }).unwrap_or(0) as usize
    }

    pub fn current(&self) -> usize {
        let f = self.get_image_index;
        // SAFETY: pointer verified non-null when registering the callback.
        catch_unwind(|| unsafe { f() }).unwrap_or(0) as usize
    }

    pub fn is_ejected(&self) -> bool {
        let f = self.get_eject_state;
        // SAFETY: pointer verified non-null when registering the callback.
        catch_unwind(|| unsafe { f() }).unwrap_or(false)
    }

    pub fn set_eject(&self, ejected: bool) -> bool {
        let f = self.set_eject_state;
        // SAFETY: pointer verified non-null when registering the callback.
        catch_unwind(|| unsafe { f(ejected) }).unwrap_or(false)
    }

    pub fn set_index(&self, index: usize) -> bool {
        let f = self.set_image_index;
        // SAFETY: pointer verified non-null when registering the callback.
        catch_unwind(|| unsafe { f(index as c_uint) }).unwrap_or(false)
    }

    pub fn label(&self, index: usize) -> Option<String> {
        let f = self.get_image_label?;
        let mut buf = [0u8; 256];
        // SAFETY: f is a valid C function pointer, buf is allocated on stack.
        let ok = catch_unwind(AssertUnwindSafe(|| unsafe {
            f(index as c_uint, buf.as_mut_ptr() as *mut c_char, buf.len())
        }))
        .unwrap_or(false);
        if ok {
            CStr::from_bytes_until_nul(&buf)
                .ok()
                .map(|c| c.to_string_lossy().into_owned())
        } else {
            None
        }
    }

    pub fn info(&self) -> DiskInfo {
        let count = self.count();
        let current = self.current();
        let ejected = self.is_ejected();
        let label = self.label(current);
        DiskInfo {
            count,
            current,
            label,
            ejected,
        }
    }

    /// Changes the disk index according to the libretro virtual tray protocol:
    /// Eject tray -> Set disk index -> Close tray.
    pub fn change_disk(&self, index: usize) -> bool {
        if index >= self.count() {
            return false;
        }
        if !self.is_ejected() && !self.set_eject(true) {
            return false;
        }
        if !self.set_index(index) {
            let _ = self.set_eject(false);
            return false;
        }
        self.set_eject(false)
    }
}
