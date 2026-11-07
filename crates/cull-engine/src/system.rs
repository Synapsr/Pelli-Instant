//! What the computer can take (memory, cores, efficiency cores) and how the
//! analysis workers run on it: the background mode's thread count and
//! priority (`AppSettings::background`: efficiency cores at a low priority,
//! so the computer stays fluid for Lightroom or a browser during the
//! analysis).

/// Worker threads of the background mode for `cores` logical cores when the
/// number of efficiency cores is unknown: half of them.
fn half_cores(cores: usize) -> usize {
    (cores / 2).max(2)
}

/// Efficiency cores of an Apple Silicon Mac (`hw.perflevel1.logicalcpu`);
/// `None` elsewhere or on a Mac with a single kind of core.
pub fn efficiency_cores() -> Option<usize> {
    #[cfg(target_os = "macos")]
    {
        let mut n: libc::c_int = 0;
        let mut len = std::mem::size_of::<libc::c_int>();
        let name = c"hw.perflevel1.logicalcpu";
        // SAFETY: the value is a C int; buffer and length match it.
        let ok = unsafe {
            libc::sysctlbyname(name.as_ptr(), (&mut n as *mut libc::c_int).cast(), &mut len, std::ptr::null_mut(), 0)
        };
        (ok == 0 && n > 0).then_some(n as usize)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Worker threads of the background mode: one per efficiency core on Apple
/// Silicon (that is where background-QoS threads run), half the cores
/// elsewhere.
pub fn background_threads() -> usize {
    efficiency_cores().unwrap_or_else(|| half_cores(cpu_cores())).clamp(1, 32)
}

/// Sets the priority of the calling thread for analysis work: background
/// or not (user-initiated QoS on macOS, every core). Called by each analysis
/// worker when it starts. No-op on other systems than macOS and Windows.
///
/// Background on macOS is the background QoS class, which the scheduler
/// keeps on the efficiency cores. Measured on an M4 (4 P + 6 E cores), with a
/// CPU-bound job of 4 processes in the foreground: it kept ~32% of its speed
/// beside a normal analysis, ~53% beside one at utility QoS (which still
/// takes the performance cores when they are free), ~80% beside one at
/// background QoS. Its I/O is throttled when other I/O is pending: the
/// analysis yields to an import in Lightroom too. Windows: below-normal
/// priority.
pub fn set_worker_priority(background: bool) {
    #[cfg(target_os = "macos")]
    {
        let class = if background {
            libc::qos_class_t::QOS_CLASS_BACKGROUND
        } else {
            libc::qos_class_t::QOS_CLASS_USER_INITIATED
        };
        // SAFETY: plain call on the current thread with a valid class.
        let rc = unsafe { libc::pthread_set_qos_class_self_np(class, 0) };
        if rc != 0 {
            log::debug!("pthread_set_qos_class_self_np failed: {rc}");
        }
    }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL, THREAD_PRIORITY_NORMAL,
        };
        let priority = if background { THREAD_PRIORITY_BELOW_NORMAL } else { THREAD_PRIORITY_NORMAL };
        // SAFETY: the pseudo-handle of the current thread is always valid.
        unsafe {
            SetThreadPriority(GetCurrentThread(), priority);
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = background;
    }
}

/// Total physical memory in bytes, when the OS tells us.
pub fn total_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let mut size: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let name = c"hw.memsize";
        // SAFETY: `hw.memsize` is a u64; buffer and length match it.
        let ok = unsafe {
            libc::sysctlbyname(name.as_ptr(), (&mut size as *mut u64).cast(), &mut len, std::ptr::null_mut(), 0)
        };
        (ok == 0 && size > 0).then_some(size)
    }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        // SAFETY: plain C struct, `dwLength` set as the API requires.
        unsafe {
            let mut status: MEMORYSTATUSEX = std::mem::zeroed();
            status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            (GlobalMemoryStatusEx(&mut status) != 0).then_some(status.ullTotalPhys)
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kib: u64 = text.lines().find(|l| l.starts_with("MemTotal:"))?.split_whitespace().nth(1)?.parse().ok()?;
        Some(kib * 1024)
    }
}

/// Logical cores.
pub fn cpu_cores() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

/// Keeps a long job the photographer started (analysis, Maximum pack,
/// export, upload) at full speed until it is dropped, even with the window
/// hidden: on macOS, no App Nap (it throttles the threads of an app nobody
/// looks at) and no idle system sleep; on Windows, no idle sleep. The screen
/// may still turn off. Made and dropped on the job's own thread, one per
/// thread (on Windows, dropping one ends the thread's request).
pub struct KeepAwake {
    #[cfg(target_os = "macos")]
    activity: objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2::runtime::NSObjectProtocol>>,
    /// Not `Send`: Windows ties the request to the calling thread.
    _thread: std::marker::PhantomData<*const ()>,
}

impl KeepAwake {
    /// `reason` shows in `pmset -g assertions` (macOS).
    pub fn new(reason: &str) -> KeepAwake {
        #[cfg(target_os = "macos")]
        let activity = {
            use objc2_foundation::{NSActivityOptions, NSProcessInfo, NSString};
            NSProcessInfo::processInfo()
                .beginActivityWithOptions_reason(NSActivityOptions::UserInitiated, &NSString::from_str(reason))
        };
        #[cfg(target_os = "windows")]
        {
            use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED};
            // SAFETY: plain call that only sets flags of the current thread.
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = reason;
        KeepAwake {
            #[cfg(target_os = "macos")]
            activity,
            _thread: std::marker::PhantomData,
        }
    }
}

impl Drop for KeepAwake {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        // SAFETY: `activity` is the object `beginActivityWithOptions` returned.
        unsafe {
            objc2_foundation::NSProcessInfo::processInfo().endActivity(&self.activity);
        }
        #[cfg(target_os = "windows")]
        // SAFETY: as in `new`, on the same thread (the guard is not `Send`).
        unsafe {
            windows_sys::Win32::System::Power::SetThreadExecutionState(
                windows_sys::Win32::System::Power::ES_CONTINUOUS,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computer() {
        const GIB: u64 = 1024 * 1024 * 1024;
        assert_eq!((half_cores(10), half_cores(2)), (5, 2));
        assert!(total_memory_bytes().is_some_and(|m| m > GIB));
        assert!((1..=32).contains(&background_threads()));
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert!(efficiency_cores().is_some_and(|e| e < cpu_cores()));
        }
        // Callable from any thread.
        std::thread::spawn(|| set_worker_priority(true)).join().unwrap();
    }

    #[test]
    fn keep_awake_begins_and_ends() {
        std::thread::spawn(|| drop(KeepAwake::new("test"))).join().unwrap();
    }
}
