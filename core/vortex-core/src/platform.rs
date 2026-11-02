//! Platform-specifieke stukjes (Windows: Job Objects, affiniteit, prioriteit, idle-tijd).
//! Op andere platforms: Linux-equivalenten waar ze bestaan, anders een eerlijke "niet beschikbaar".
// VX:W04.02

/// Prioriteitsklasse voor een proces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcPriority {
    High,
    AboveNormal,
    Normal,
    BelowNormal,
    Idle,
}

#[cfg(windows)]
#[allow(unsafe_code)] // Win32-API; elke aanroep controleert handles en returnwaarden.
mod imp {
    use super::ProcPriority;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };
    use windows_sys::Win32::System::SystemInformation::GetTickCount;
    use windows_sys::Win32::System::Threading::{
        ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS,
        NORMAL_PRIORITY_CLASS, OpenProcess, PROCESS_SET_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
        SetPriorityClass, SetProcessAffinityMask,
    };
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    fn open(pid: u32) -> Result<HANDLE, String> {
        let h = unsafe { OpenProcess(PROCESS_SET_INFORMATION | PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid) };
        if h.is_null() { Err(format!("OpenProcess({pid}) mislukt")) } else { Ok(h) }
    }

    pub fn set_affinity(pid: u32, cores: &[usize]) -> Result<(), String> {
        let mask: usize = cores.iter().filter(|c| **c < usize::BITS as usize).fold(0, |m, c| m | (1usize << c));
        let h = open(pid)?;
        let ok = unsafe { SetProcessAffinityMask(h, mask) };
        unsafe { CloseHandle(h) };
        if ok == 0 { Err("SetProcessAffinityMask mislukt".into()) } else { Ok(()) }
    }

    pub fn set_priority(pid: u32, p: ProcPriority) -> Result<(), String> {
        let class = match p {
            ProcPriority::High => HIGH_PRIORITY_CLASS,
            ProcPriority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
            ProcPriority::Normal => NORMAL_PRIORITY_CLASS,
            ProcPriority::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
            ProcPriority::Idle => IDLE_PRIORITY_CLASS,
        };
        let h = open(pid)?;
        let ok = unsafe { SetPriorityClass(h, class) };
        unsafe { CloseHandle(h) };
        if ok == 0 { Err("SetPriorityClass mislukt".into()) } else { Ok(()) }
    }

    /// Harde geheugencap via een Job Object. De job blijft open zolang de handle leeft.
    #[derive(Debug)]
    pub struct JobCap(HANDLE);
    unsafe impl Send for JobCap {}
    unsafe impl Sync for JobCap {}
    impl Drop for JobCap {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn cap_memory(pid: u32, max_bytes: u64) -> Result<JobCap, String> {
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err("CreateJobObject mislukt".into());
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        info.ProcessMemoryLimit = max_bytes as usize;
        let ok = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            unsafe { CloseHandle(job) };
            return Err("SetInformationJobObject mislukt".into());
        }
        let h = open(pid)?;
        let ok = unsafe { AssignProcessToJobObject(job, h) };
        unsafe { CloseHandle(h) };
        if ok == 0 {
            unsafe { CloseHandle(job) };
            return Err("AssignProcessToJobObject mislukt".into());
        }
        Ok(JobCap(job))
    }

    pub fn idle_secs() -> Option<u64> {
        let mut li = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
        if unsafe { GetLastInputInfo(&mut li) } == 0 {
            return None;
        }
        let now = unsafe { GetTickCount() };
        Some(u64::from(now.wrapping_sub(li.dwTime)) / 1000)
    }
}

#[cfg(unix)]
mod imp {
    use super::ProcPriority;

    pub fn set_affinity(pid: u32, cores: &[usize]) -> Result<(), String> {
        use nix::sched::{CpuSet, sched_setaffinity};
        use nix::unistd::Pid;
        let mut set = CpuSet::new();
        for c in cores {
            set.set(*c).map_err(|e| e.to_string())?;
        }
        sched_setaffinity(Pid::from_raw(pid as i32), &set).map_err(|e| e.to_string())
    }

    pub fn set_priority(_pid: u32, _p: ProcPriority) -> Result<(), String> {
        // Linux: nice-waarden vereisen libc::setpriority; Vortex draait productie op Windows.
        Err("procesprioriteit alleen op Windows geïmplementeerd".into())
    }

    #[derive(Debug)]
    pub struct JobCap;

    pub fn cap_memory(_pid: u32, _max: u64) -> Result<JobCap, String> {
        Err("harde geheugencap (Job Object) alleen op Windows; hier geldt de RSS-bewaking".into())
    }

    pub fn idle_secs() -> Option<u64> {
        None
    }
}

pub use imp::{JobCap, cap_memory, idle_secs, set_affinity, set_priority};

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn affinity_on_own_process() {
        let pid = std::process::id();
        let n = core_affinity::get_core_ids().map(|c| c.len()).unwrap_or(1);
        assert!(set_affinity(pid, &(0..n).collect::<Vec<_>>()).is_ok());
        assert!(idle_secs().is_none(), "Linux: eerlijk 'onbekend'");
        assert!(cap_memory(pid, 1 << 30).is_err());
    }
}
