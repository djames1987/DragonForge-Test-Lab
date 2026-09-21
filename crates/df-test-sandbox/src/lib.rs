#[cfg(not(windows))]
use std::env;
use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command},
};
use thiserror::Error;
use uuid::Uuid;

pub const DEFAULT_CONTAINER_IMAGE: &str = "dragonforge/test-lab-rust:0.4.0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxMode {
    Native,
    Docker,
    Podman,
}

impl SandboxMode {
    pub fn parse(value: &str) -> Result<Self, SandboxError> {
        match value {
            "native" => Ok(Self::Native),
            "docker" => Ok(Self::Docker),
            "podman" => Ok(Self::Podman),
            _ => Err(SandboxError::UnknownMode(value.to_owned())),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }

    pub fn container_program(self) -> Option<&'static str> {
        match self {
            Self::Native => None,
            Self::Docker => Some("docker"),
            Self::Podman => Some("podman"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxLimits {
    pub max_memory_mib: u64,
    pub max_processes: u32,
}

impl SandboxLimits {
    pub fn validate(self) -> Result<Self, SandboxError> {
        if self.max_memory_mib == 0 || self.max_processes == 0 {
            return Err(SandboxError::InvalidLimits);
        }
        Ok(self)
    }

    pub fn memory_bytes(self) -> usize {
        let bytes = self.max_memory_mib.saturating_mul(1024 * 1024);
        usize::try_from(bytes).unwrap_or(usize::MAX)
    }
}

#[derive(Debug)]
pub struct SandboxedCommand {
    pub program: String,
    pub args: Vec<String>,
    pub current_dir: PathBuf,
    pub container_cleanup: Option<ContainerCleanup>,
}

#[derive(Debug)]
pub struct ContainerCleanup {
    runtime: String,
    name: String,
}

impl ContainerCleanup {
    pub fn force_remove(&self) -> Result<(), SandboxError> {
        let output = Command::new(&self.runtime)
            .args(["rm", "-f", &self.name])
            .output()
            .map_err(|source| SandboxError::SpawnRuntime {
                program: self.runtime.clone(),
                source,
            })?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("No such container") || stderr.contains("no container with name or ID") {
            return Ok(());
        }

        Err(SandboxError::ContainerCleanupFailed {
            runtime: self.runtime.clone(),
            name: self.name.clone(),
            output: stderr.trim().to_owned(),
        })
    }
}

pub fn sandbox_project_command(
    mode: SandboxMode,
    program: &str,
    args: &[String],
    repository_dir: &Path,
    limits: SandboxLimits,
) -> Result<SandboxedCommand, SandboxError> {
    let limits = limits.validate()?;
    match mode {
        SandboxMode::Native => Ok(SandboxedCommand {
            program: program.to_owned(),
            args: args.to_vec(),
            current_dir: repository_dir.to_path_buf(),
            container_cleanup: None,
        }),
        SandboxMode::Docker | SandboxMode::Podman => {
            if program != "cargo" {
                return Err(SandboxError::UnsupportedContainerProgram(
                    program.to_owned(),
                ));
            }

            let runtime = mode
                .container_program()
                .expect("container mode must have a runtime program");
            let mount = format!(
                "type=bind,source={},target=/workspace",
                repository_dir.to_string_lossy()
            );
            let container_name = format!("dragonforge-testlab-{}", Uuid::new_v4());

            let mut wrapped = vec![
                "run".into(),
                "--rm".into(),
                "--name".into(),
                container_name.clone(),
                "--init".into(),
                "--cap-drop=ALL".into(),
                "--security-opt=no-new-privileges".into(),
                "--memory".into(),
                format!("{}m", limits.max_memory_mib),
                "--pids-limit".into(),
                limits.max_processes.to_string(),
                "--mount".into(),
                mount,
                "-w".into(),
                "/workspace".into(),
                DEFAULT_CONTAINER_IMAGE.into(),
                "cargo".into(),
            ];
            wrapped.extend(args.iter().cloned());

            Ok(SandboxedCommand {
                program: runtime.into(),
                args: wrapped,
                current_dir: repository_dir.to_path_buf(),
                container_cleanup: Some(ContainerCleanup {
                    runtime: runtime.into(),
                    name: container_name,
                }),
            })
        }
    }
}

pub fn current_worker_identity() -> Result<String, SandboxError> {
    #[cfg(windows)]
    {
        windows::current_username()
    }

    #[cfg(not(windows))]
    {
        env::var("USER").map_err(|_| SandboxError::WorkerIdentityUnavailable)
    }
}

pub fn verify_worker_identity(expected: &str) -> Result<(), SandboxError> {
    let actual = current_worker_identity()?;
    let matches = if cfg!(windows) {
        actual.eq_ignore_ascii_case(expected)
    } else {
        actual == expected
    };

    if matches {
        Ok(())
    } else {
        Err(SandboxError::WorkerIdentityMismatch {
            expected: expected.to_owned(),
            actual,
        })
    }
}

pub fn runtime_version(mode: SandboxMode) -> Result<Option<String>, SandboxError> {
    let Some(program) = mode.container_program() else {
        return Ok(None);
    };

    let output = Command::new(program)
        .arg("--version")
        .output()
        .map_err(|source| SandboxError::SpawnRuntime {
            program: program.to_owned(),
            source,
        })?;

    if !output.status.success() {
        return Err(SandboxError::RuntimeUnavailable(program.to_owned()));
    }

    Ok(Some(
        String::from_utf8_lossy(&output.stdout).trim().to_owned(),
    ))
}

pub fn verify_container_image(mode: SandboxMode) -> Result<(), SandboxError> {
    let Some(program) = mode.container_program() else {
        return Ok(());
    };

    let output = Command::new(program)
        .args(["image", "inspect", DEFAULT_CONTAINER_IMAGE])
        .output()
        .map_err(|source| SandboxError::SpawnRuntime {
            program: program.to_owned(),
            source,
        })?;

    if output.status.success() {
        Ok(())
    } else {
        Err(SandboxError::ContainerImageUnavailable {
            runtime: program.to_owned(),
            image: DEFAULT_CONTAINER_IMAGE.to_owned(),
        })
    }
}

pub struct ProcessTreeGuard {
    #[cfg(windows)]
    inner: windows::JobGuard,
    #[cfg(not(windows))]
    _mode: SandboxMode,
}

impl ProcessTreeGuard {
    pub fn new(mode: SandboxMode, limits: SandboxLimits) -> Result<Self, SandboxError> {
        let limits = limits.validate()?;

        #[cfg(windows)]
        {
            let _ = mode;
            Ok(Self {
                inner: windows::JobGuard::new(limits)?,
            })
        }

        #[cfg(not(windows))]
        {
            if mode == SandboxMode::Native {
                return Err(SandboxError::NativeContainmentUnsupported);
            }
            let _ = limits;
            Ok(Self { _mode: mode })
        }
    }

    pub fn prepare_command(&self, command: &mut Command) -> Result<(), SandboxError> {
        #[cfg(windows)]
        {
            self.inner.prepare(command);
        }

        #[cfg(not(windows))]
        {
            let _ = command;
        }

        Ok(())
    }

    pub fn attach(&self, child: &mut Child) -> Result<(), SandboxError> {
        #[cfg(windows)]
        {
            if let Err(error) = self.inner.attach_and_resume(child) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        }

        #[cfg(not(windows))]
        {
            let _ = child;
        }

        Ok(())
    }

    pub fn terminate_tree(&self) -> Result<bool, SandboxError> {
        #[cfg(windows)]
        {
            self.inner.terminate()?;
            Ok(true)
        }

        #[cfg(not(windows))]
        {
            Ok(false)
        }
    }

    pub fn mechanism(&self) -> &'static str {
        #[cfg(windows)]
        {
            "windows_job_object"
        }

        #[cfg(not(windows))]
        {
            "container_runtime"
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::{SandboxError, SandboxLimits};
    use std::{
        ffi::c_void,
        io,
        mem::size_of,
        os::windows::{io::AsRawHandle, process::CommandExt},
        process::{Child, Command},
        ptr,
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD,
                THREADENTRY32,
            },
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_JOB_MEMORY,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Threading::{OpenThread, ResumeThread, CREATE_SUSPENDED, THREAD_SUSPEND_RESUME},
            WindowsProgramming::GetUserNameW,
        },
    };

    pub fn current_username() -> Result<String, SandboxError> {
        let mut buffer = [0_u16; 257];
        let mut length = buffer.len() as u32;
        let result = unsafe { GetUserNameW(buffer.as_mut_ptr(), &mut length) };
        if result == 0 {
            return Err(SandboxError::WindowsIdentity(io::Error::last_os_error()));
        }

        let length_without_nul = length.saturating_sub(1) as usize;
        Ok(String::from_utf16_lossy(&buffer[..length_without_nul]))
    }

    pub struct JobGuard {
        handle: HANDLE,
    }

    impl JobGuard {
        pub fn new(limits: SandboxLimits) -> Result<Self, SandboxError> {
            let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
            if handle.is_null() {
                return Err(SandboxError::WindowsJob(io::Error::last_os_error()));
            }

            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
                | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
                | JOB_OBJECT_LIMIT_JOB_MEMORY;
            info.BasicLimitInformation.ActiveProcessLimit = limits.max_processes;
            info.JobMemoryLimit = limits.memory_bytes();

            let configured = unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&info as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast::<c_void>(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };

            if configured == 0 {
                let error = io::Error::last_os_error();
                unsafe {
                    CloseHandle(handle);
                }
                return Err(SandboxError::WindowsJob(error));
            }

            Ok(Self { handle })
        }

        pub fn prepare(&self, command: &mut Command) {
            command.creation_flags(CREATE_SUSPENDED);
        }

        pub fn attach_and_resume(&self, child: &Child) -> Result<(), SandboxError> {
            let process = child.as_raw_handle() as HANDLE;
            let assigned = unsafe { AssignProcessToJobObject(self.handle, process) };
            if assigned == 0 {
                return Err(SandboxError::WindowsJob(io::Error::last_os_error()));
            }

            resume_process_threads(child.id())?;
            Ok(())
        }

        pub fn terminate(&self) -> Result<(), SandboxError> {
            let terminated = unsafe { TerminateJobObject(self.handle, 1) };
            if terminated == 0 {
                return Err(SandboxError::WindowsJob(io::Error::last_os_error()));
            }
            Ok(())
        }
    }

    fn resume_process_threads(process_id: u32) -> Result<(), SandboxError> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(SandboxError::WindowsJob(io::Error::last_os_error()));
        }

        let mut entry = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        let mut found = false;
        let mut has_entry = unsafe { Thread32First(snapshot, &mut entry) } != 0;

        while has_entry {
            if entry.th32OwnerProcessID == process_id {
                let thread = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                if thread.is_null() {
                    let error = io::Error::last_os_error();
                    unsafe {
                        CloseHandle(snapshot);
                    }
                    return Err(SandboxError::WindowsJob(error));
                }

                let resumed = unsafe { ResumeThread(thread) };
                unsafe {
                    CloseHandle(thread);
                }
                if resumed == u32::MAX {
                    let error = io::Error::last_os_error();
                    unsafe {
                        CloseHandle(snapshot);
                    }
                    return Err(SandboxError::WindowsJob(error));
                }
                found = true;
            }

            has_entry = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
        }

        unsafe {
            CloseHandle(snapshot);
        }

        if !found {
            return Err(SandboxError::WindowsJob(io::Error::new(
                io::ErrorKind::NotFound,
                "suspended child process had no discoverable thread",
            )));
        }

        Ok(())
    }

    impl Drop for JobGuard {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.handle);
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("unknown sandbox mode: {0}")]
    UnknownMode(String),
    #[error("sandbox memory and process limits must both be greater than zero")]
    InvalidLimits,
    #[error("container sandbox can only wrap fixed Cargo project actions, got {0}")]
    UnsupportedContainerProgram(String),
    #[error("worker identity could not be determined")]
    WorkerIdentityUnavailable,
    #[error("worker identity mismatch: expected {expected}, running as {actual}")]
    WorkerIdentityMismatch { expected: String, actual: String },
    #[cfg(windows)]
    #[error("failed to read Windows worker identity: {0}")]
    WindowsIdentity(io::Error),
    #[error("failed to start container runtime {program}: {source}")]
    SpawnRuntime {
        program: String,
        #[source]
        source: io::Error,
    },
    #[error("container runtime is unavailable: {0}")]
    RuntimeUnavailable(String),
    #[error("container image {image} is unavailable in {runtime}; build it with scripts/build-sandbox-image.ps1")]
    ContainerImageUnavailable { runtime: String, image: String },
    #[error("failed to remove sandbox container {name} with {runtime}: {output}")]
    ContainerCleanupFailed {
        runtime: String,
        name: String,
        output: String,
    },
    #[error("native process-tree containment is not yet supported on this platform; use docker or podman")]
    NativeContainmentUnsupported,
    #[cfg(windows)]
    #[error("Windows Job Object operation failed: {0}")]
    WindowsJob(io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_modes() {
        assert_eq!(SandboxMode::parse("native").unwrap(), SandboxMode::Native);
        assert_eq!(SandboxMode::parse("docker").unwrap(), SandboxMode::Docker);
        assert_eq!(SandboxMode::parse("podman").unwrap(), SandboxMode::Podman);
        assert!(SandboxMode::parse("shell").is_err());
    }

    #[test]
    fn container_command_is_fixed_and_resource_bounded() {
        let repo = Path::new("C:/lab/repo");
        let command = sandbox_project_command(
            SandboxMode::Docker,
            "cargo",
            &["test".into(), "--workspace".into()],
            repo,
            SandboxLimits {
                max_memory_mib: 1024,
                max_processes: 32,
            },
        )
        .unwrap();

        assert_eq!(command.program, "docker");
        assert!(command.args.contains(&"--name".into()));
        assert!(command
            .args
            .iter()
            .any(|value| value.starts_with("dragonforge-testlab-")));
        assert!(command.container_cleanup.is_some());
        assert!(command.args.contains(&"--cap-drop=ALL".into()));
        assert!(command
            .args
            .contains(&"--security-opt=no-new-privileges".into()));
        assert!(command.args.contains(&"1024m".into()));
        assert!(command.args.contains(&"32".into()));
        assert_eq!(command.args.last().unwrap(), "--workspace");
    }

    #[test]
    fn current_worker_identity_matches_itself() {
        let identity = current_worker_identity().unwrap();
        assert!(!identity.is_empty());
        verify_worker_identity(&identity).unwrap();
    }

    #[test]
    fn zero_resource_limits_are_rejected() {
        assert!(matches!(
            SandboxLimits {
                max_memory_mib: 0,
                max_processes: 1
            }
            .validate(),
            Err(SandboxError::InvalidLimits)
        ));
        assert!(matches!(
            SandboxLimits {
                max_memory_mib: 1,
                max_processes: 0
            }
            .validate(),
            Err(SandboxError::InvalidLimits)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn windows_job_object_contains_real_child_process() {
        let limits = SandboxLimits {
            max_memory_mib: 512,
            max_processes: 8,
        };
        let guard = ProcessTreeGuard::new(SandboxMode::Native, limits).unwrap();
        let mut command = Command::new("rustc");
        command.arg("--version");
        command.stdin(std::process::Stdio::null());
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        guard.prepare_command(&mut command).unwrap();

        let mut child = command.spawn().unwrap();
        guard.attach(&mut child).unwrap();
        let status = child.wait().unwrap();
        assert!(status.success());
    }

    #[test]
    fn container_mode_rejects_non_cargo_programs() {
        let result = sandbox_project_command(
            SandboxMode::Docker,
            "git",
            &[],
            Path::new("."),
            SandboxLimits {
                max_memory_mib: 512,
                max_processes: 8,
            },
        );
        assert!(matches!(
            result,
            Err(SandboxError::UnsupportedContainerProgram(_))
        ));
    }
}
