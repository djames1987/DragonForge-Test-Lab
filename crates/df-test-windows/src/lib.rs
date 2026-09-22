use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
    time::Duration,
};
use thiserror::Error;
use uuid::Uuid;

pub const FIXTURE_PREFIX: &str = "DragonForge-TestLab-";
const POWERSHELL: &str = "powershell.exe";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsDoctor {
    pub powershell_version: String,
    pub elevated: bool,
    pub event_log_readable: bool,
    pub registry_hkcu_readable: bool,
    pub scm_readable: bool,
    pub windows_installer_service_present: bool,
    pub msiexec_present: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeFixtureReport {
    pub registry_round_trip: bool,
    pub process_spawn: bool,
    pub tcp_loopback: bool,
    pub udp_loopback: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivilegedFixtureReport {
    pub service_create_query_delete: bool,
    pub event_log_write: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallerInfo {
    pub path: String,
    pub size_bytes: u64,
    pub signature_status: String,
    pub signer_subject: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct WindowsIntegrationClient;

impl WindowsIntegrationClient {
    pub fn doctor(&self) -> Result<WindowsDoctor, WindowsError> {
        require_windows()?;
        let script = concat!(
            "$ErrorActionPreference='Stop';",
            "$id=[Security.Principal.WindowsIdentity]::GetCurrent();",
            "$p=New-Object Security.Principal.WindowsPrincipal($id);",
            "$elevated=$p.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator);",
            "$eventReadable=$false;try{Get-WinEvent -LogName System -MaxEvents 1 -ErrorAction Stop|Out-Null;$eventReadable=$true}catch{};",
            "$regReadable=Test-Path 'HKCU:\Software';",
            "$scmReadable=$false;try{Get-Service -ErrorAction Stop|Select-Object -First 1|Out-Null;$scmReadable=$true}catch{};",
            "$msiService=[bool](Get-Service msiserver -ErrorAction SilentlyContinue);",
            "$msiexec=[bool](Get-Command msiexec.exe -ErrorAction SilentlyContinue);",
            "[pscustomobject]@{powershell_version=$PSVersionTable.PSVersion.ToString();elevated=$elevated;",
            "event_log_readable=$eventReadable;registry_hkcu_readable=$regReadable;scm_readable=$scmReadable;",
            "windows_installer_service_present=$msiService;msiexec_present=$msiexec}|ConvertTo-Json -Compress"
        );
        let output = run_powershell(script)?;
        if !output.status.success() {
            return Err(command_failed("Windows doctor", &output));
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    pub fn run_safe_fixtures(&self) -> Result<SafeFixtureReport, WindowsError> {
        require_windows()?;
        self.registry_round_trip()?;
        process_fixture()?;
        tcp_fixture()?;
        udp_fixture()?;

        Ok(SafeFixtureReport {
            registry_round_trip: true,
            process_spawn: true,
            tcp_loopback: true,
            udp_loopback: true,
        })
    }

    pub fn run_privileged_fixtures(&self) -> Result<PrivilegedFixtureReport, WindowsError> {
        require_windows()?;
        let doctor = self.doctor()?;
        if !doctor.elevated {
            return Err(WindowsError::AdministratorRequired);
        }

        self.service_fixture()?;
        self.event_log_fixture()?;

        Ok(PrivilegedFixtureReport {
            service_create_query_delete: true,
            event_log_write: true,
        })
    }

    pub fn inspect_msi(&self, path: &Path) -> Result<InstallerInfo, WindowsError> {
        require_windows()?;
        validate_msi_path(path)?;
        let absolute = absolute_existing_file(path)?;
        let size_bytes = fs::metadata(&absolute)?.len();
        let script = format!(
            concat!(
                "$ErrorActionPreference='Stop';",
                "$sig=Get-AuthenticodeSignature -LiteralPath '{}';",
                "[pscustomobject]@{{status=$sig.Status.ToString();signer=if($sig.SignerCertificate){{$sig.SignerCertificate.Subject}}else{{$null}}}}",
                "|ConvertTo-Json -Compress"
            ),
            ps_literal_path(&absolute)
        );
        let output = run_powershell(&script)?;
        if !output.status.success() {
            return Err(command_failed("inspect MSI signature", &output));
        }

        #[derive(Deserialize)]
        struct Signature {
            status: String,
            signer: Option<String>,
        }

        let signature: Signature = serde_json::from_slice(&output.stdout)?;
        Ok(InstallerInfo {
            path: absolute.to_string_lossy().into_owned(),
            size_bytes,
            signature_status: signature.status,
            signer_subject: signature.signer,
        })
    }

    fn registry_round_trip(&self) -> Result<(), WindowsError> {
        let id = Uuid::new_v4().simple().to_string();
        let path = format!("HKCU:\\Software\\DragonForge\\TestLab\\Fixtures\\{id}");
        let script = format!(
            concat!(
                "$ErrorActionPreference='Stop';",
                "$path='{}';",
                "try{{New-Item -Path $path -Force|Out-Null;",
                "New-ItemProperty -Path $path -Name 'Value' -Value 'dragonforge-phase6' -PropertyType String -Force|Out-Null;",
                "$v=(Get-ItemProperty -Path $path -Name 'Value' -ErrorAction Stop).Value;",
                "if($v -ne 'dragonforge-phase6'){{throw 'registry round-trip mismatch'}}}}",
                "finally{{Remove-Item -Path $path -Recurse -Force -ErrorAction SilentlyContinue}}"
            ),
            ps_literal(&path)
        );
        success_powershell(&script, "registry fixture")
    }

    fn service_fixture(&self) -> Result<(), WindowsError> {
        let name = fixture_name("Service")?;
        let bin_path = PathBuf::from(r"C:\Windows\System32\notepad.exe");
        if !bin_path.is_file() {
            return Err(WindowsError::MissingSystemBinary(bin_path));
        }

        let bin_path_text = bin_path.to_string_lossy().into_owned();
        let create = Command::new("sc.exe")
            .args([
                "create",
                &name,
                "binPath=",
                &bin_path_text,
                "start=",
                "demand",
                "DisplayName=",
                &name,
            ])
            .output()?;
        if !create.status.success() {
            return Err(command_failed("create service fixture", &create));
        }

        let result = (|| {
            let query = Command::new("sc.exe").args(["query", &name]).output()?;
            if !query.status.success() {
                return Err(command_failed("query service fixture", &query));
            }
            Ok(())
        })();

        let delete = Command::new("sc.exe").args(["delete", &name]).output();
        if result.is_ok() {
            let output = delete?;
            if !output.status.success() {
                return Err(command_failed("delete service fixture", &output));
            }
        } else if let Ok(output) = delete {
            let _ = output;
        }

        result
    }

    fn event_log_fixture(&self) -> Result<(), WindowsError> {
        let description = format!("DragonForge Test Lab Phase 6 validation {}", Uuid::new_v4());
        let output = Command::new("eventcreate.exe")
            .args([
                "/T",
                "INFORMATION",
                "/ID",
                "100",
                "/L",
                "APPLICATION",
                "/SO",
                "DragonForge-TestLab",
                "/D",
                &description,
            ])
            .output()?;
        if !output.status.success() {
            return Err(command_failed("write Application Event Log fixture", &output));
        }
        Ok(())
    }
}

fn process_fixture() -> Result<(), WindowsError> {
    let output = Command::new("ping.exe")
        .args(["-n", "1", "127.0.0.1"])
        .output()?;
    if !output.status.success() {
        return Err(command_failed("process fixture", &output));
    }
    Ok(())
}

fn tcp_fixture() -> Result<(), WindowsError> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.set_nonblocking(false)?;
    let address = listener.local_addr()?;

    let client = thread::spawn(move || -> std::io::Result<()> {
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))?;
        stream.write_all(b"dragonforge")?;
        let mut response = [0_u8; 2];
        stream.read_exact(&mut response)?;
        if &response != b"ok" {
            return Err(std::io::Error::other("unexpected TCP fixture response"));
        }
        Ok(())
    });

    let (mut stream, _) = listener.accept()?;
    let mut payload = [0_u8; 11];
    stream.read_exact(&mut payload)?;
    if &payload != b"dragonforge" {
        return Err(WindowsError::FixtureMismatch("TCP payload".into()));
    }
    stream.write_all(b"ok")?;

    client
        .join()
        .map_err(|_| WindowsError::FixtureThreadPanicked)??;
    Ok(())
}

fn udp_fixture() -> Result<(), WindowsError> {
    let server = UdpSocket::bind(("127.0.0.1", 0))?;
    let client = UdpSocket::bind(("127.0.0.1", 0))?;
    server.set_read_timeout(Some(Duration::from_secs(2)))?;
    client.set_read_timeout(Some(Duration::from_secs(2)))?;

    client.send_to(b"dragonforge", server.local_addr()?)?;
    let mut buffer = [0_u8; 64];
    let (read, peer) = server.recv_from(&mut buffer)?;
    if &buffer[..read] != b"dragonforge" {
        return Err(WindowsError::FixtureMismatch("UDP payload".into()));
    }
    server.send_to(b"ok", peer)?;
    let (read, _) = client.recv_from(&mut buffer)?;
    if &buffer[..read] != b"ok" {
        return Err(WindowsError::FixtureMismatch("UDP response".into()));
    }
    Ok(())
}

fn fixture_name(kind: &str) -> Result<String, WindowsError> {
    if kind.is_empty()
        || kind.len() > 24
        || !kind
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return Err(WindowsError::InvalidFixtureKind);
    }
    Ok(format!(
        "{FIXTURE_PREFIX}{kind}-{}",
        &Uuid::new_v4().simple().to_string()[..12]
    ))
}

fn validate_msi_path(path: &Path) -> Result<(), WindowsError> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("msi") {
        return Err(WindowsError::NotMsi);
    }
    Ok(())
}

fn absolute_existing_file(path: &Path) -> Result<PathBuf, WindowsError> {
    let absolute = fs::canonicalize(path)?;
    if !absolute.is_file() {
        return Err(WindowsError::NotFile);
    }
    Ok(absolute)
}

fn run_powershell(script: &str) -> Result<Output, WindowsError> {
    Ok(Command::new(POWERSHELL)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output()?)
}

fn success_powershell(script: &str, operation: &str) -> Result<(), WindowsError> {
    let output = run_powershell(script)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_failed(operation, &output))
    }
}

fn require_windows() -> Result<(), WindowsError> {
    if cfg!(windows) {
        Ok(())
    } else {
        Err(WindowsError::WindowsRequired)
    }
}

fn ps_literal(value: &str) -> String {
    value.replace(''', "''")
}

fn ps_literal_path(path: &Path) -> String {
    ps_literal(&path.to_string_lossy())
}

fn command_failed(operation: &str, output: &Output) -> WindowsError {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    WindowsError::CommandFailed {
        operation: operation.to_owned(),
        detail: if stderr.is_empty() { stdout } else { stderr },
    }
}

#[derive(Debug, Error)]
pub enum WindowsError {
    #[error("Windows integration requires a Windows host")]
    WindowsRequired,
    #[error("administrator privileges are required for privileged Windows fixtures")]
    AdministratorRequired,
    #[error("fixture kind is invalid")]
    InvalidFixtureKind,
    #[error("installer path must have an .msi extension")]
    NotMsi,
    #[error("installer path is not a file")]
    NotFile,
    #[error("required Windows system binary was not found: {0}")]
    MissingSystemBinary(PathBuf),
    #[error("fixture data mismatch: {0}")]
    FixtureMismatch(String),
    #[error("fixture worker thread panicked")]
    FixtureThreadPanicked,
    #[error("{operation} failed: {detail}")]
    CommandFailed { operation: String, detail: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_names_are_managed_and_bounded() {
        let name = fixture_name("Service").unwrap();
        assert!(name.starts_with(FIXTURE_PREFIX));
        assert!(name.len() < 80);
    }

    #[test]
    fn fixture_kind_rejects_script_characters() {
        assert!(fixture_name("Service;whoami").is_err());
        assert!(fixture_name("Service'Bad").is_err());
    }

    #[test]
    fn msi_extension_is_required() {
        assert!(validate_msi_path(Path::new("installer.msi")).is_ok());
        assert!(validate_msi_path(Path::new("installer.MSI")).is_ok());
        assert!(validate_msi_path(Path::new("installer.exe")).is_err());
    }

    #[test]
    fn powershell_literals_escape_single_quotes() {
        assert_eq!(ps_literal("a'b"), "a''b");
    }
}
