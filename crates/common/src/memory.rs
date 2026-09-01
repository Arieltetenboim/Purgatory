//! Lightweight process memory + CPU sampling for capacity characterization.
//!
//! One syscall family per sample. Do not spawn PowerShell/CIM on the 1 Hz path.

/// Working-set sample when available.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProcessMemory {
    pub working_set_bytes: u64,
    pub peak_working_set_bytes: u64,
}

/// Cumulative process CPU time (user + kernel) when available.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProcessCpu {
    pub cpu_time_secs: f64,
    pub logical_cpus: u32,
}

/// Combined resource sample.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ProcessResources {
    pub memory: ProcessMemory,
    pub cpu: ProcessCpu,
}

/// Current process working set. `None` on unsupported platforms.
#[must_use]
pub fn current_process_memory() -> Option<ProcessMemory> {
    current_process_resources().map(|r| r.memory)
}

/// Current process memory + CPU time. `None` on unsupported platforms.
#[must_use]
pub fn current_process_resources() -> Option<ProcessResources> {
    #[cfg(windows)]
    {
        windows_impl::sample()
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Logical processor count for utilization normalization.
#[must_use]
pub fn logical_cpu_count() -> u32 {
    #[cfg(windows)]
    {
        windows_impl::logical_cpus()
    }
    #[cfg(not(windows))]
    {
        std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(1)
            .max(1)
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::{ProcessCpu, ProcessMemory, ProcessResources};

    #[allow(unsafe_code)]
    pub(super) fn logical_cpus() -> u32 {
        use windows_sys::Win32::System::SystemInformation::GetSystemInfo;
        let mut info = std::mem::MaybeUninit::uninit();
        unsafe {
            GetSystemInfo(info.as_mut_ptr());
            let info = info.assume_init();
            info.dwNumberOfProcessors.max(1)
        }
    }

    #[allow(unsafe_code)]
    pub(super) fn sample() -> Option<ProcessResources> {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::ProcessStatus::{
            GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
        };
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

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

        let mut creation = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let mut exit = creation;
        let mut kernel = creation;
        let mut user = creation;
        let times_ok =
            unsafe { GetProcessTimes(process, &mut creation, &mut exit, &mut kernel, &mut user) };
        let cpu_time_secs = if times_ok != 0 {
            filetime_to_secs(kernel) + filetime_to_secs(user)
        } else {
            0.0
        };

        Some(ProcessResources {
            memory: ProcessMemory {
                working_set_bytes: counters.WorkingSetSize as u64,
                peak_working_set_bytes: counters.PeakWorkingSetSize as u64,
            },
            cpu: ProcessCpu {
                cpu_time_secs,
                logical_cpus: logical_cpus(),
            },
        })
    }

    fn filetime_to_secs(ft: windows_sys::Win32::Foundation::FILETIME) -> f64 {
        let ticks = (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime);
        // FILETIME is 100 ns intervals.
        ticks as f64 * 1e-7
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_does_not_panic() {
        let _ = current_process_memory();
        let _ = current_process_resources();
        assert!(logical_cpu_count() >= 1);
    }

    #[cfg(windows)]
    #[test]
    fn windows_reports_nonzero_working_set_and_cpu() {
        let res = current_process_resources().expect("windows resources");
        assert!(res.memory.working_set_bytes > 0);
        assert!(res.cpu.logical_cpus >= 1);
        // Fresh process may have near-zero CPU; just ensure finite.
        assert!(res.cpu.cpu_time_secs.is_finite());
    }
}
