// ── Engram: Process Memory Hardening (§10.7) ────────────────────────────────
//
// Defense-in-depth for sensitive data in process memory:
//   - Disable core dumps (prevent forensic extraction of keys/memories)
//   - mlock() for key material (prevent swap-to-disk)
//   - Advice for secure allocations
//
// These are best-effort: failures are logged but non-fatal, since the
// application must still function on systems without these capabilities
// (e.g. containers, sandboxes without CAP_IPC_LOCK).

use log::{info, warn};

/// Apply all available process-level hardening measures.
/// Call once at application startup.
pub fn harden_process() {
    disable_core_dumps();
    info!("[security] Process memory hardening applied");
}

/// Disable core dumps to prevent key material from leaking to disk.
/// Uses `setrlimit(RLIMIT_CORE, 0)` on Unix systems.
fn disable_core_dumps() {
    #[cfg(unix)]
    {
        use std::io;
        // SAFETY: setrlimit is a standard POSIX function.
        // Setting RLIMIT_CORE to 0 is safe and widely used.
        let result = unsafe {
            let rlimit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            libc::setrlimit(libc::RLIMIT_CORE, &rlimit)
        };
        if result == 0 {
            info!("[security] Core dumps disabled (RLIMIT_CORE=0)");
        } else {
            warn!(
                "[security] Could not disable core dumps: {}",
                io::Error::last_os_error()
            );
        }
    }
    #[cfg(not(unix))]
    {
        warn!("[security] Core dump disabling not supported on this platform");
    }
}

/// Lock a memory region to prevent it from being swapped to disk.
/// Returns true if mlock succeeded.
///
/// # Safety
/// The caller must ensure `ptr` and `len` describe a valid memory region.
///
/// Best-effort: fails silently if CAP_IPC_LOCK is not available.
#[cfg(unix)]
pub unsafe fn mlock_region(ptr: *const u8, len: usize) -> bool {
    if len == 0 {
        return true;
    }
    let result = libc::mlock(ptr as *const libc::c_void, len);
    if result != 0 {
        warn!(
            "[security] mlock failed for {} bytes: {} (need CAP_IPC_LOCK?)",
            len,
            std::io::Error::last_os_error()
        );
        false
    } else {
        true
    }
}

/// Unlock a previously mlocked memory region.
///
/// # Safety
/// The caller must ensure `ptr` and `len` describe a valid memory region
/// that was previously passed to `mlock_region`.
#[cfg(unix)]
pub unsafe fn munlock_region(ptr: *const u8, len: usize) {
    if len > 0 {
        libc::munlock(ptr as *const libc::c_void, len);
    }
}

/// Securely zero a byte slice.
/// Uses `zeroize` which is resistant to compiler optimizations that
/// might elide the zeroing.
pub fn secure_zero(data: &mut [u8]) {
    use zeroize::Zeroize;
    data.zeroize();
}

/// A guard that mlocks memory on creation and munlocks + zeroizes on drop.
/// Use for short-lived key material that must not touch swap.
#[cfg(unix)]
pub struct MlockedBuffer {
    buf: Vec<u8>,
    locked: bool,
}

#[cfg(unix)]
impl MlockedBuffer {
    /// Create a new mlocked buffer of the given size.
    pub fn new(size: usize) -> Self {
        let mut buf = vec![0u8; size];
        // SAFETY: buf is a valid heap allocation of `size` bytes.
        let locked = unsafe { mlock_region(buf.as_ptr(), buf.len()) };
        if !locked {
            warn!("[security] MlockedBuffer: could not mlock {} bytes", size);
        }
        // Zero contents on creation (the vec is already zeroed from vec![0u8; size]
        // but we explicitly zero for defense-in-depth).
        for byte in buf.iter_mut() {
            *byte = 0;
        }
        Self { buf, locked }
    }

    /// Get a mutable reference to the buffer contents.
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.buf
    }

    /// Get a read-only reference to the buffer contents.
    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }
}

#[cfg(unix)]
impl Drop for MlockedBuffer {
    fn drop(&mut self) {
        // Zeroize bytes before unlock
        secure_zero(&mut self.buf);

        if self.locked {
            // SAFETY: buf is the same allocation that was mlocked.
            unsafe {
                munlock_region(self.buf.as_ptr(), self.buf.len());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_zero() {
        let mut data = vec![0xAB_u8; 32];
        secure_zero(&mut data);
        assert!(data.iter().all(|&b| b == 0));
    }

    #[cfg(unix)]
    #[test]
    fn test_mlocked_buffer() {
        let mut buf = MlockedBuffer::new(64);
        // Write to it
        buf.as_mut_slice()[0] = 42;
        assert_eq!(buf.as_slice()[0], 42);
        // Drop will zeroize + munlock
    }
}
