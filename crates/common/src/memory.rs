//! Lightweight process memory sampling for load tests (Phase 5.7).
//!
//! One syscall per sample. Do not spawn PowerShell/CIM on the 1 Hz path.

/// Working-set sample when available.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessMemory {
    pub working_set_bytes: u64,
}

/// Current process working set. `None` on unsupported platforms.
#[must_use]
pub fn current_process_memory() -> Option<ProcessMemory> {
    #[cfg(windows)]
    {
        windows_impl::working_set()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::ProcessMemory;

    #[allow(unsafe_code)]
    pub(super) fn working_set() -> Option<ProcessMemory> {
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::GetCurrentProcess;

        // GetCurrentProcess returns a pseudo-handle; do not CloseHandle it.
        let process = unsafe { GetCurrentProcess() };
        let mut counters = PROCESS_MEMORY_COUNTERS {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            PageFaultCount: 0,
            PeakWorkingSetSize: 0,
            WorkingSetSize: 0,
            QuotaPeakPagedPoolUsage: 0,
            QuotaPagedPoolUsage: 0,
            QuotaPeakNonPagedPoolUsage: 0,
            QuotaNonPagedPoolUsage: 0,
            PagefileUsage: 0,
            PeakPagefileUsage: 0,
        };
        let ok = unsafe {
            GetProcessMemoryInfo(
                process,
                &mut counters,
                std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
            )
        };
        if ok == 0 {
            return None;
        }
        Some(ProcessMemory {
            working_set_bytes: counters.WorkingSetSize as u64,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_does_not_panic() {
        let _ = current_process_memory();
    }

    #[cfg(windows)]
    #[test]
    fn windows_reports_nonzero_working_set() {
        let mem = current_process_memory().expect("windows working set");
        assert!(mem.working_set_bytes > 0);
    }
}
