use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

const MAX_TEXT_BYTES: u64 = 4096;
const MAX_DEVICE_ENTRIES: usize = 64;
const MAX_PLAN_PROBES: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArmCapability {
    Arm32,
    Arm64,
    RaspberryPi,
    Gpio,
    I2c,
    Spi,
    Uart,
    ThermalSensor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareProbe {
    BoardModel,
    CpuTemperature,
    GpioControllers,
    I2cBuses,
    SpiDevices,
    SerialDevices,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArmInventory {
    pub os: String,
    pub arch: String,
    pub board_model: Option<String>,
    pub capabilities: BTreeSet<ArmCapability>,
}

impl ArmInventory {
    pub fn is_arm(&self) -> bool {
        self.capabilities.contains(&ArmCapability::Arm32)
            || self.capabilities.contains(&ArmCapability::Arm64)
    }

    pub fn is_raspberry_pi(&self) -> bool {
        self.capabilities.contains(&ArmCapability::RaspberryPi)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub probe: HardwareProbe,
    pub available: bool,
    pub values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HardwarePlan {
    pub probes: Vec<HardwareProbe>,
}

impl HardwarePlan {
    pub fn validate(&self) -> Result<(), ArmError> {
        if self.probes.is_empty() || self.probes.len() > MAX_PLAN_PROBES {
            return Err(ArmError::InvalidPlan);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ArmInspector {
    root: PathBuf,
    os: String,
    arch: String,
}

impl ArmInspector {
    pub fn host() -> Self {
        Self {
            root: PathBuf::from("/"),
            os: std::env::consts::OS.to_owned(),
            arch: std::env::consts::ARCH.to_owned(),
        }
    }

    pub fn with_root(root: impl Into<PathBuf>, os: impl Into<String>, arch: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            os: os.into(),
            arch: arch.into(),
        }
    }

    pub fn inventory(&self) -> Result<ArmInventory, ArmError> {
        let mut capabilities = BTreeSet::new();
        match self.arch.as_str() {
            "aarch64" | "arm64" => {
                capabilities.insert(ArmCapability::Arm64);
            }
            value if value.starts_with("arm") => {
                capabilities.insert(ArmCapability::Arm32);
            }
            _ => {}
        }

        let board_model = read_optional_text(&self.root.join("proc/device-tree/model"))?;
        if board_model
            .as_deref()
            .map(|value| value.to_ascii_lowercase().contains("raspberry pi"))
            .unwrap_or(false)
        {
            capabilities.insert(ArmCapability::RaspberryPi);
        }

        if directory_has_entries(&self.root.join("sys/class/gpio"), None)? {
            capabilities.insert(ArmCapability::Gpio);
        }
        if directory_has_entries(&self.root.join("dev"), Some("i2c-"))? {
            capabilities.insert(ArmCapability::I2c);
        }
        if directory_has_entries(&self.root.join("dev"), Some("spidev"))? {
            capabilities.insert(ArmCapability::Spi);
        }
        if directory_has_any_prefix(&self.root.join("dev"), &["serial", "ttyAMA", "ttyS"])? {
            capabilities.insert(ArmCapability::Uart);
        }
        if self
            .root
            .join("sys/class/thermal/thermal_zone0/temp")
            .is_file()
        {
            capabilities.insert(ArmCapability::ThermalSensor);
        }

        Ok(ArmInventory {
            os: self.os.clone(),
            arch: self.arch.clone(),
            board_model,
            capabilities,
        })
    }

    pub fn run_plan(&self, plan: &HardwarePlan) -> Result<Vec<ProbeResult>, ArmError> {
        plan.validate()?;
        plan.probes.iter().copied().map(|probe| self.run_probe(probe)).collect()
    }

    pub fn run_probe(&self, probe: HardwareProbe) -> Result<ProbeResult, ArmError> {
        match probe {
            HardwareProbe::BoardModel => {
                let value = read_optional_text(&self.root.join("proc/device-tree/model"))?;
                Ok(ProbeResult {
                    probe,
                    available: value.is_some(),
                    values: value.into_iter().collect(),
                })
            }
            HardwareProbe::CpuTemperature => {
                let path = self.root.join("sys/class/thermal/thermal_zone0/temp");
                let value = read_optional_text(&path)?;
                let values = if let Some(raw) = value {
                    let milli_celsius = raw
                        .trim()
                        .parse::<i64>()
                        .map_err(|_| ArmError::InvalidTemperature)?;
                    if !(-100_000..=200_000).contains(&milli_celsius) {
                        return Err(ArmError::InvalidTemperature);
                    }
                    vec![milli_celsius.to_string()]
                } else {
                    Vec::new()
                };
                Ok(ProbeResult {
                    probe,
                    available: !values.is_empty(),
                    values,
                })
            }
            HardwareProbe::GpioControllers => Ok(device_probe(
                probe,
                &self.root.join("sys/class/gpio"),
                None,
            )?),
            HardwareProbe::I2cBuses => Ok(device_probe(
                probe,
                &self.root.join("dev"),
                Some("i2c-"),
            )?),
            HardwareProbe::SpiDevices => Ok(device_probe(
                probe,
                &self.root.join("dev"),
                Some("spidev"),
            )?),
            HardwareProbe::SerialDevices => Ok(device_probe_any_prefix(
                probe,
                &self.root.join("dev"),
                &["serial", "ttyAMA", "ttyS"],
            )?),
        }
    }
}

pub fn run_phase20_fixture() -> Result<Phase20FixtureReport, ArmError> {
    let root = std::env::temp_dir().join(format!(
        "dragonforge-phase20-arm-fixture-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);

    fs::create_dir_all(root.join("proc/device-tree"))?;
    fs::create_dir_all(root.join("sys/class/gpio/gpiochip0"))?;
    fs::create_dir_all(root.join("sys/class/thermal/thermal_zone0"))?;
    fs::create_dir_all(root.join("dev"))?;
    fs::write(root.join("proc/device-tree/model"), b"Raspberry Pi 5 Model B Rev 1.0\0")?;
    fs::write(root.join("sys/class/thermal/thermal_zone0/temp"), b"42500\n")?;
    fs::write(root.join("dev/i2c-1"), b"")?;
    fs::write(root.join("dev/spidev0.0"), b"")?;
    fs::write(root.join("dev/serial0"), b"")?;

    let inspector = ArmInspector::with_root(&root, "linux", "aarch64");
    let inventory = inspector.inventory()?;
    let plan = HardwarePlan {
        probes: vec![
            HardwareProbe::BoardModel,
            HardwareProbe::CpuTemperature,
            HardwareProbe::GpioControllers,
            HardwareProbe::I2cBuses,
            HardwareProbe::SpiDevices,
            HardwareProbe::SerialDevices,
        ],
    };
    let results = inspector.run_plan(&plan)?;
    let report = Phase20FixtureReport {
        arm64_detected: inventory.capabilities.contains(&ArmCapability::Arm64),
        raspberry_pi_detected: inventory.is_raspberry_pi(),
        gpio_detected: inventory.capabilities.contains(&ArmCapability::Gpio),
        i2c_detected: inventory.capabilities.contains(&ArmCapability::I2c),
        spi_detected: inventory.capabilities.contains(&ArmCapability::Spi),
        uart_detected: inventory.capabilities.contains(&ArmCapability::Uart),
        thermal_detected: inventory.capabilities.contains(&ArmCapability::ThermalSensor),
        all_probes_bounded: results.len() == 6
            && results.iter().all(|result| result.values.len() <= MAX_DEVICE_ENTRIES),
    };

    let _ = fs::remove_dir_all(&root);
    Ok(report)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phase20FixtureReport {
    pub arm64_detected: bool,
    pub raspberry_pi_detected: bool,
    pub gpio_detected: bool,
    pub i2c_detected: bool,
    pub spi_detected: bool,
    pub uart_detected: bool,
    pub thermal_detected: bool,
    pub all_probes_bounded: bool,
}

fn read_optional_text(path: &Path) -> Result<Option<String>, ArmError> {
    if !path.is_file() {
        return Ok(None);
    }
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_TEXT_BYTES {
        return Err(ArmError::InputTooLarge);
    }
    let bytes = fs::read(path)?;
    let value = String::from_utf8(bytes).map_err(|_| ArmError::InvalidUtf8)?;
    let value = value.trim_matches(char::from(0)).trim().to_owned();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn directory_has_entries(path: &Path, prefix: Option<&str>) -> Result<bool, ArmError> {
    Ok(!list_entries(path, prefix)?.is_empty())
}

fn directory_has_any_prefix(path: &Path, prefixes: &[&str]) -> Result<bool, ArmError> {
    Ok(!list_entries_any_prefix(path, prefixes)?.is_empty())
}

fn device_probe(
    probe: HardwareProbe,
    path: &Path,
    prefix: Option<&str>,
) -> Result<ProbeResult, ArmError> {
    let values = list_entries(path, prefix)?;
    Ok(ProbeResult {
        probe,
        available: !values.is_empty(),
        values,
    })
}

fn device_probe_any_prefix(
    probe: HardwareProbe,
    path: &Path,
    prefixes: &[&str],
) -> Result<ProbeResult, ArmError> {
    let values = list_entries_any_prefix(path, prefixes)?;
    Ok(ProbeResult {
        probe,
        available: !values.is_empty(),
        values,
    })
}

fn list_entries(path: &Path, prefix: Option<&str>) -> Result<Vec<String>, ArmError> {
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if prefix.map(|value| name.starts_with(value)).unwrap_or(true) {
            values.push(name.into_owned());
            if values.len() >= MAX_DEVICE_ENTRIES {
                break;
            }
        }
    }
    values.sort();
    Ok(values)
}

fn list_entries_any_prefix(path: &Path, prefixes: &[&str]) -> Result<Vec<String>, ArmError> {
    if !path.is_dir() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if prefixes.iter().any(|prefix| name.starts_with(prefix)) {
            values.push(name.into_owned());
            if values.len() >= MAX_DEVICE_ENTRIES {
                break;
            }
        }
    }
    values.sort();
    Ok(values)
}

#[derive(Debug, Error)]
pub enum ArmError {
    #[error("hardware probe plan must contain between 1 and {MAX_PLAN_PROBES} probes")]
    InvalidPlan,
    #[error("hardware metadata exceeds the bounded input limit")]
    InputTooLarge,
    #[error("hardware metadata is not valid UTF-8")]
    InvalidUtf8,
    #[error("CPU temperature value is invalid or outside the accepted range")]
    InvalidTemperature,
    #[error("hardware inventory I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase20_fixture_detects_typed_raspberry_pi_capabilities() {
        let report = run_phase20_fixture().unwrap();
        assert!(report.arm64_detected);
        assert!(report.raspberry_pi_detected);
        assert!(report.gpio_detected);
        assert!(report.i2c_detected);
        assert!(report.spi_detected);
        assert!(report.uart_detected);
        assert!(report.thermal_detected);
        assert!(report.all_probes_bounded);
    }

    #[test]
    fn plan_rejects_empty_and_oversized_probe_sets() {
        assert!(HardwarePlan { probes: vec![] }.validate().is_err());
        assert!(HardwarePlan {
            probes: vec![HardwareProbe::BoardModel; MAX_PLAN_PROBES + 1]
        }
        .validate()
        .is_err());
    }

    #[test]
    fn non_arm_architecture_does_not_claim_arm_capability() {
        let root = std::env::temp_dir().join(format!(
            "dragonforge-phase20-nonarm-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let inventory = ArmInspector::with_root(&root, "linux", "x86_64")
            .inventory()
            .unwrap();
        assert!(!inventory.is_arm());
        let _ = fs::remove_dir_all(root);
    }
}
