use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::ExitStatusExt;
use std::process::{Command, ExitStatus};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Threading::*;

pub struct Child {
    process: OwnedHandle,
    job: OwnedHandle,
    pid: u32,
}

struct Attributes {
    _storage: Vec<usize>,
    list: LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl Attributes {
    fn new(job: &HANDLE, stdio: &HANDLE) -> io::Result<Self> {
        let mut size = 0;
        unsafe {
            InitializeProcThreadAttributeList(null_mut(), 2, 0, &mut size);
        }
        let mut storage = vec![0usize; size.div_ceil(size_of::<usize>())];
        let list = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(list, 2, 0, &mut size) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let attributes = Self {
            _storage: storage,
            list,
        };
        if unsafe {
            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_JOB_LIST as usize,
                job as *const HANDLE as *const _,
                size_of::<HANDLE>(),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if unsafe {
            UpdateProcThreadAttribute(
                list,
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                stdio as *const HANDLE as *const _,
                size_of::<HANDLE>(),
                null_mut(),
                null(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(attributes)
    }
}

impl Drop for Attributes {
    fn drop(&mut self) {
        unsafe {
            DeleteProcThreadAttributeList(self.list);
        }
    }
}

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut out: Vec<_> = value.encode_wide().collect();
    if out.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "zero in process argument",
        ));
    }
    out.push(0);
    Ok(out)
}

fn quote(value: &OsStr, out: &mut Vec<u16>) -> io::Result<()> {
    let encoded = wide(value)?;
    let encoded = &encoded[..encoded.len() - 1];
    if !encoded.is_empty() && !encoded.iter().any(|c| matches!(*c, 9 | 32 | 34)) {
        out.extend_from_slice(encoded);
        return Ok(());
    }
    out.push(b'"' as u16);
    let mut slashes = 0;
    for c in wide(value)?.into_iter().take_while(|c| *c != 0) {
        if c == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        out.extend(std::iter::repeat_n(
            b'\\' as u16,
            if c == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        out.push(c);
        slashes = 0;
    }
    out.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    out.push(b'"' as u16);
    Ok(())
}

pub fn spawn(command: &Command) -> io::Result<Child> {
    let raw_job = unsafe { CreateJobObjectW(null(), null()) };
    if raw_job.is_null() {
        return Err(io::Error::last_os_error());
    }
    let job = unsafe { OwnedHandle::from_raw_handle(raw_job) };
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            raw_job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as _,
            size_of_val(&limits) as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let nul = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("NUL")?;
    let mut raw_stdio = null_mut();
    if unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            nul.as_raw_handle(),
            GetCurrentProcess(),
            &mut raw_stdio,
            0,
            1,
            DUPLICATE_SAME_ACCESS,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let _stdio = unsafe { OwnedHandle::from_raw_handle(raw_stdio) };
    let attributes = Attributes::new(&raw_job, &raw_stdio)?;
    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = raw_stdio;
    startup.StartupInfo.hStdOutput = raw_stdio;
    startup.StartupInfo.hStdError = raw_stdio;
    startup.lpAttributeList = attributes.list;
    let mut line = Vec::new();
    quote(command.get_program(), &mut line)?;
    for arg in command.get_args() {
        line.push(b' ' as u16);
        quote(arg, &mut line)?;
    }
    line.push(0);
    let mut vars: BTreeMap<OsString, (OsString, OsString)> = std::env::vars_os()
        .map(|(k, v)| (OsString::from(k.to_string_lossy().to_uppercase()), (k, v)))
        .collect();
    for (key, value) in command.get_envs() {
        let upper = OsString::from(key.to_string_lossy().to_uppercase());
        if let Some(value) = value {
            vars.insert(upper, (key.to_owned(), value.to_owned()));
        } else {
            vars.remove(&upper);
        }
    }
    let mut environment = Vec::new();
    for (_, (key, value)) in vars {
        let mut name = wide(&key)?;
        name.pop();
        if name.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty environment name",
            ));
        }
        environment.extend(name);
        environment.push(b'=' as u16);
        environment.extend(wide(&value)?);
    }
    environment.push(0);
    let cwd = command
        .get_current_dir()
        .map(|p| wide(p.as_os_str()))
        .transpose()?;
    let mut info = PROCESS_INFORMATION::default();
    let ok = unsafe {
        CreateProcessW(
            null(),
            line.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_ptr().cast(),
            cwd.as_ref().map_or(null(), |p| p.as_ptr()),
            &startup.StartupInfo,
            &mut info,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    let process = unsafe { OwnedHandle::from_raw_handle(info.hProcess) };
    drop(unsafe { OwnedHandle::from_raw_handle(info.hThread) });
    Ok(Child {
        process,
        job,
        pid: info.dwProcessId,
    })
}

impl Child {
    pub fn id(&self) -> u32 {
        self.pid
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match unsafe { WaitForSingleObject(self.process.as_raw_handle(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                if unsafe { GetExitCodeProcess(self.process.as_raw_handle(), &mut code) } == 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(Some(ExitStatus::from_raw(code)))
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
    pub fn kill(&mut self) -> io::Result<()> {
        if unsafe { TerminateJobObject(self.job.as_raw_handle(), 1) } == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        if unsafe { WaitForSingleObject(self.process.as_raw_handle(), u32::MAX) } != WAIT_OBJECT_0 {
            return Err(io::Error::last_os_error());
        }
        self.try_wait()?
            .ok_or_else(|| io::Error::other("process did not finish"))
    }
}
