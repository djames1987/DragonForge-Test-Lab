use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use thiserror::Error;
use uuid::Uuid;

pub const VM_NAME_PREFIX: &str = "DragonForge-";
pub const DEFAULT_BASELINE_CHECKPOINT: &str = "DragonForge-Baseline";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestOs {
    Windows,
    Linux,
}

impl GuestOs {
    pub fn parse(value: &str) -> Result<Self, VmError> {
        match value {
            "windows" => Ok(Self::Windows),
            "linux" => Ok(Self::Linux),
            _ => Err(VmError::InvalidGuestOs(value.to_owned())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VmLabConfig {
    pub root: PathBuf,
    pub base_image_root: PathBuf,
    pub switch_name: String,
}

impl VmLabConfig {
    pub fn under(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            base_image_root: root.join("images"),
            root,
            switch_name: "Default Switch".into(),
        }
    }

    pub fn vm_root(&self) -> PathBuf {
        self.root.join("vms")
    }
}

#[derive(Debug, Clone)]
pub struct VmCreateSpec {
    pub name: String,
    pub guest_os: GuestOs,
    pub base_vhdx: PathBuf,
    pub memory_mib: u64,
    pub processors: u32,
    pub switch_name: String,
}

impl VmCreateSpec {
    pub fn validate(&self, config: &VmLabConfig) -> Result<(), VmError> {
        validate_vm_name(&self.name)?;
        validate_switch_name(&self.switch_name)?;
        if self.memory_mib < 1024 || self.memory_mib > 131_072 {
            return Err(VmError::InvalidMemory(self.memory_mib));
        }
        if self.processors == 0 || self.processors > 64 {
            return Err(VmError::InvalidProcessorCount(self.processors));
        }
        validate_base_image(&config.base_image_root, &self.base_vhdx)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmSummary {
    pub name: String,
    pub state: String,
    pub status: String,
    pub generation: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmLabDoctor {
    pub hyper_v_enabled: bool,
    pub hyper_v_module: bool,
    pub vmms_running: bool,
    pub switches: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct HyperVClient {
    powershell: String,
}

impl Default for HyperVClient {
    fn default() -> Self {
        Self {
            powershell: "powershell.exe".into(),
        }
    }
}

impl HyperVClient {
    pub fn doctor(&self) -> Result<VmLabDoctor, VmError> {
        require_windows()?;
        let script = concat!(
            "$ErrorActionPreference='Stop';",
            "$module=[bool](Get-Module -ListAvailable Hyper-V);",
            "$vmms=Get-Service vmms -ErrorAction SilentlyContinue;",
            "$vmhost=$null;try{$vmhost=Get-VMHost -ErrorAction Stop}catch{};",
            "$switches=@(Get-VMSwitch -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Name);",
            "[pscustomobject]@{hyper_v_enabled=($null -ne $vmhost);",
            "hyper_v_module=$module;vmms_running=($vmms.Status -eq 'Running');switches=$switches}",
            "| ConvertTo-Json -Compress"
        );
        let output = self.run_script(script)?;
        if !output.status.success() {
            return Err(command_failed("Hyper-V doctor", &output));
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    pub fn list_managed(&self) -> Result<Vec<VmSummary>, VmError> {
        require_windows()?;
        let pattern = format!("{}*", VM_NAME_PREFIX);
        let script = format!(
            "$ErrorActionPreference='Stop';@((Get-VM -Name '{}' -ErrorAction SilentlyContinue)|ForEach-Object{{[pscustomobject]@{{name=$_.Name;state=$_.State.ToString();status=$_.Status;generation=[uint32]$_.Generation}}}})|ConvertTo-Json -Compress",
            ps_literal(&pattern)
        );
        let output = self.run_script(&script)?;
        if !output.status.success() {
            return Err(command_failed("list managed VMs", &output));
        }
        parse_json_array(&output.stdout)
    }

    pub fn create_from_base(
        &self,
        config: &VmLabConfig,
        spec: &VmCreateSpec,
    ) -> Result<(), VmError> {
        require_windows()?;
        spec.validate(config)?;

        let vm_root = absolute_path(&config.vm_root())?;
        fs::create_dir_all(&vm_root)?;
        fs::create_dir_all(&config.base_image_root)?;

        let canonical_root = fs::canonicalize(&config.base_image_root)?;
        let canonical_base = fs::canonicalize(&spec.base_vhdx)?;
        if !canonical_base.is_file() || !is_descendant(&canonical_root, &canonical_base) {
            return Err(VmError::BaseImageOutsideRoot);
        }
        let base = absolute_path(&spec.base_vhdx)?;

        let instance_dir = vm_root.join(&spec.name);
        let disk_dir = instance_dir.join("Virtual Hard Disks");
        fs::create_dir_all(&disk_dir)?;
        let child_disk = disk_dir.join("os.vhdx");

        if child_disk.exists() {
            return Err(VmError::VmStorageExists(child_disk));
        }

        let secure_boot = match spec.guest_os {
            GuestOs::Windows => "$true",
            GuestOs::Linux => "$false",
        };
        let script = format!(
            concat!(
                "$ErrorActionPreference='Stop';",
                "if(Get-VM -Name '{name}' -ErrorAction SilentlyContinue){{throw 'managed VM already exists'}};",
                "New-VHD -Path '{child}' -ParentPath '{base}' -Differencing | Out-Null;",
                "New-VM -Name '{name}' -Generation 2 -MemoryStartupBytes {memory}MB -VHDPath '{child}' -Path '{vmroot}' -SwitchName '{switch}' | Out-Null;",
                "Set-VMProcessor -VMName '{name}' -Count {cpus};",
                "Set-VMFirmware -VMName '{name}' -EnableSecureBoot {secure_boot};",
                "Set-VM -Name '{name}' -AutomaticStartAction Nothing -AutomaticStopAction ShutDown -CheckpointType Standard;"
            ),
            name = ps_literal(&spec.name),
            child = ps_literal_path(&child_disk),
            base = ps_literal_path(&base),
            memory = spec.memory_mib,
            vmroot = ps_literal_path(&vm_root),
            switch = ps_literal(&spec.switch_name),
            cpus = spec.processors,
            secure_boot = secure_boot,
        );
        let output = self.run_script(&script)?;
        if !output.status.success() {
            let cleanup = format!(
                "$vm=Get-VM -Name '{}' -ErrorAction SilentlyContinue;if($vm){{Stop-VM -VM $vm -TurnOff -ErrorAction SilentlyContinue;Remove-VM -VM $vm -Force -ErrorAction SilentlyContinue}}",
                ps_literal(&spec.name)
            );
            let _ = self.run_script(&cleanup);
            let _ = fs::remove_dir_all(&instance_dir);
            return Err(command_failed("create VM", &output));
        }
        Ok(())
    }

    pub fn start(&self, name: &str) -> Result<(), VmError> {
        self.vm_action(name, "Start-VM -Name '{name}' | Out-Null", "start VM")
    }

    pub fn stop(&self, name: &str) -> Result<(), VmError> {
        self.vm_action(
            name,
            "Stop-VM -Name '{name}' -TurnOff | Out-Null",
            "stop VM",
        )
    }

    pub fn checkpoint(&self, name: &str, checkpoint: &str) -> Result<(), VmError> {
        validate_vm_name(name)?;
        validate_checkpoint_name(checkpoint)?;
        let script = format!(
            "$ErrorActionPreference='Stop';Checkpoint-VM -Name '{}' -SnapshotName '{}' | Out-Null",
            ps_literal(name),
            ps_literal(checkpoint)
        );
        self.success(&script, "checkpoint VM")
    }

    pub fn restore(&self, name: &str, checkpoint: &str) -> Result<(), VmError> {
        validate_vm_name(name)?;
        validate_checkpoint_name(checkpoint)?;
        let script = format!(
            concat!(
                "$ErrorActionPreference='Stop';",
                "Stop-VM -Name '{name}' -TurnOff -ErrorAction SilentlyContinue;",
                "Restore-VMCheckpoint -VMName '{name}' -Name '{checkpoint}' -Confirm:$false;",
                "Start-VM -Name '{name}' | Out-Null"
            ),
            name = ps_literal(name),
            checkpoint = ps_literal(checkpoint),
        );
        self.success(&script, "restore VM checkpoint")
    }

    pub fn ensure_clean_baseline(&self, name: &str) -> Result<(), VmError> {
        validate_vm_name(name)?;
        let checkpoint = DEFAULT_BASELINE_CHECKPOINT;
        let script = format!(
            concat!(
                "$ErrorActionPreference='Stop';",
                "$vm=Get-VM -Name '{name}' -ErrorAction Stop;",
                "if($vm.State -ne 'Off'){{Stop-VM -Name '{name}' -Force -TurnOff | Out-Null}};",
                "Get-VMCheckpoint -VMName '{name}' -Name '{checkpoint}' -ErrorAction SilentlyContinue | Remove-VMCheckpoint -Confirm:$false -ErrorAction SilentlyContinue;",
                "Checkpoint-VM -Name '{name}' -SnapshotName '{checkpoint}' | Out-Null"
            ),
            name = ps_literal(name),
            checkpoint = ps_literal(checkpoint)
        );
        self.success(&script, "create clean baseline")
    }

    pub fn destroy_managed(&self, config: &VmLabConfig, name: &str) -> Result<(), VmError> {
        validate_vm_name(name)?;
        let script = format!(
            "$ErrorActionPreference='Stop';Stop-VM -Name '{}' -TurnOff -ErrorAction SilentlyContinue;Remove-VM -Name '{}' -Force",
            ps_literal(name),
            ps_literal(name)
        );
        self.success(&script, "destroy VM")?;

        let vm_dir = absolute_path(&config.vm_root())?.join(name);
        if vm_dir.exists() {
            fs::remove_dir_all(vm_dir)?;
        }
        Ok(())
    }

    fn vm_action(&self, name: &str, template: &str, operation: &'static str) -> Result<(), VmError> {
        validate_vm_name(name)?;
        let command = template.replace("{name}", &ps_literal(name));
        let script = format!("$ErrorActionPreference='Stop';{command}");
        self.success(&script, operation)
    }

    fn success(&self, script: &str, operation: &'static str) -> Result<(), VmError> {
        require_windows()?;
        let output = self.run_script(script)?;
        if output.status.success() {
            Ok(())
        } else {
            Err(command_failed(operation, &output))
        }
    }

    fn run_script(&self, script: &str) -> Result<Output, VmError> {
        Command::new(&self.powershell)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-Command",
                script,
            ])
            .output()
            .map_err(|source| VmError::Spawn {
                program: self.powershell.clone(),
                source,
            })
    }
}

fn require_windows() -> Result<(), VmError> {
    if cfg!(windows) {
        Ok(())
    } else {
        Err(VmError::HyperVUnsupported)
    }
}

fn validate_vm_name(name: &str) -> Result<(), VmError> {
    if !name.starts_with(VM_NAME_PREFIX)
        || name.len() > 80
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        return Err(VmError::InvalidVmName);
    }
    Ok(())
}

fn validate_checkpoint_name(name: &str) -> Result<(), VmError> {
    if name.is_empty()
        || name.len() > 80
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(VmError::InvalidCheckpointName);
    }
    Ok(())
}

fn validate_switch_name(name: &str) -> Result<(), VmError> {
    if name.is_empty()
        || name.len() > 80
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.'))
    {
        return Err(VmError::InvalidSwitchName);
    }
    Ok(())
}

fn validate_base_image(root: &Path, path: &Path) -> Result<(), VmError> {
    let root = fs::canonicalize(root).map_err(|_| VmError::BaseImageRootMissing)?;
    let image = absolute_existing_file(path)?;
    if !is_descendant(&root, &image) {
        return Err(VmError::BaseImageOutsideRoot);
    }
    let extension = image
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("vhdx") {
        return Err(VmError::InvalidBaseImage);
    }
    Ok(())
}

fn absolute_existing_file(path: &Path) -> Result<PathBuf, VmError> {
    let canonical = fs::canonicalize(path)?;
    if !canonical.is_file() {
        return Err(VmError::InvalidBaseImage);
    }
    Ok(canonical)
}

fn absolute_path(path: &Path) -> Result<PathBuf, VmError> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn is_descendant(root: &Path, path: &Path) -> bool {
    path.starts_with(root)
}

fn ps_literal(value: &str) -> String {
    value.replace(''', "''")
}

fn ps_literal_path(path: &Path) -> String {
    ps_literal(&path.to_string_lossy())
}

fn command_failed(operation: &'static str, output: &Output) -> VmError {
    VmError::CommandFailed {
        operation,
        output: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .trim()
        .to_owned(),
    }
}

fn parse_json_array<T>(bytes: &[u8]) -> Result<Vec<T>, VmError>
where
    T: for<'de> Deserialize<'de>,
{
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Vec::new());
    }

    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    if value.is_array() {
        Ok(serde_json::from_value(value)?)
    } else {
        Ok(vec![serde_json::from_value(value)?])
    }
}

#[derive(Debug, Error)]
pub enum VmError {
    #[error("Hyper-V VM Lab is supported only on Windows hosts")]
    HyperVUnsupported,
    #[error("invalid VM name; managed names must begin with DragonForge- and contain only letters, numbers, '-' or '_'")]
    InvalidVmName,
    #[error("invalid checkpoint name")]
    InvalidCheckpointName,
    #[error("invalid Hyper-V switch name")]
    InvalidSwitchName,
    #[error("unknown guest OS: {0}")]
    InvalidGuestOs(String),
    #[error("invalid VM memory value: {0} MiB")]
    InvalidMemory(u64),
    #[error("invalid VM processor count: {0}")]
    InvalidProcessorCount(u32),
    #[error("configured base image root does not exist")]
    BaseImageRootMissing,
    #[error("base VHDX must exist beneath the configured base image root")]
    BaseImageOutsideRoot,
    #[error("base image must be an existing .vhdx file")]
    InvalidBaseImage,
    #[error("VM storage already exists: {0}")]
    VmStorageExists(PathBuf),
    #[error("failed to start {program}: {source}")]
    Spawn {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{operation} failed: {output}")]
    CommandFailed {
        operation: &'static str,
        output: String,
    },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_managed_vm_names_are_allowed() {
        assert!(validate_vm_name("DragonForge-Win11-01").is_ok());
        assert!(validate_vm_name("DragonForge_Linux_01").is_err());
        assert!(validate_vm_name("Other-VM").is_err());
        assert!(validate_vm_name("DragonForge-bad name").is_err());
    }

    #[test]
    fn names_and_switches_reject_script_characters() {
        assert!(validate_checkpoint_name("clean.baseline-1").is_ok());
        assert!(validate_checkpoint_name("bad';Remove-VM").is_err());
        assert!(validate_switch_name("Default Switch").is_ok());
        assert!(validate_switch_name("x';whoami").is_err());
    }

    #[test]
    fn guest_os_is_typed() {
        assert_eq!(GuestOs::parse("windows").unwrap(), GuestOs::Windows);
        assert_eq!(GuestOs::parse("linux").unwrap(), GuestOs::Linux);
        assert!(GuestOs::parse("other").is_err());
    }

    #[test]
    fn powershell_literals_escape_single_quotes() {
        assert_eq!(ps_literal("O'Brien"), "O''Brien");
    }
}
