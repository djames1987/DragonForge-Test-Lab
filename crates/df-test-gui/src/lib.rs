use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;

pub const MANAGED_WINDOW_PREFIX: &str = "DragonForge-";
pub const PHASE7_FIXTURE_TITLE: &str = "DragonForge-GUI-Fixture";
const POWERSHELL: &str = "powershell.exe";
const FIXTURE_SCRIPT_NAME: &str = "phase7-gui-fixture.ps1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiDoctor {
    pub user_interactive: bool,
    pub session_name: String,
    pub ui_automation_available: bool,
    pub drawing_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiPlan {
    pub window_title: String,
    pub actions: Vec<GuiAction>,
}

impl GuiPlan {
    pub fn validate(&self) -> Result<(), GuiError> {
        validate_window_title(&self.window_title)?;
        if self.actions.is_empty() || self.actions.len() > 128 {
            return Err(GuiError::InvalidActionCount);
        }
        for action in &self.actions {
            action.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GuiAction {
    WaitForWindow {
        timeout_ms: u64,
    },
    SetValue {
        automation_id: String,
        value: String,
    },
    Invoke {
        automation_id: String,
    },
    AssertValue {
        automation_id: String,
        expected: String,
    },
    Screenshot {
        name: String,
    },
}

impl GuiAction {
    fn validate(&self) -> Result<(), GuiError> {
        match self {
            Self::WaitForWindow { timeout_ms } => {
                if !(100..=120_000).contains(timeout_ms) {
                    return Err(GuiError::InvalidTimeout);
                }
            }
            Self::SetValue {
                automation_id,
                value,
            } => {
                validate_automation_id(automation_id)?;
                if value.len() > 4096 {
                    return Err(GuiError::ValueTooLong);
                }
            }
            Self::Invoke { automation_id } => validate_automation_id(automation_id)?,
            Self::AssertValue {
                automation_id,
                expected,
            } => {
                validate_automation_id(automation_id)?;
                if expected.len() > 4096 {
                    return Err(GuiError::ValueTooLong);
                }
            }
            Self::Screenshot { name } => validate_artifact_name(name)?,
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiActionReport {
    pub index: usize,
    pub action: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiRunReport {
    pub window_title: String,
    pub passed: bool,
    pub actions: Vec<GuiActionReport>,
    pub artifact_directory: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrashReport {
    pub fixture: String,
    pub expected_exit_code: i32,
    pub observed_exit_code: Option<i32>,
    pub captured: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GuiAutomationClient;

impl GuiAutomationClient {
    pub fn doctor(&self) -> Result<GuiDoctor, GuiError> {
        require_windows()?;
        let script = concat!(
            "$ErrorActionPreference='Stop';",
            "$uia=$false;try{Add-Type -AssemblyName UIAutomationClient -ErrorAction Stop;Add-Type -AssemblyName UIAutomationTypes -ErrorAction Stop;$uia=$true}catch{};",
            "$drawing=$false;try{Add-Type -AssemblyName System.Drawing -ErrorAction Stop;$drawing=$true}catch{};",
            "[pscustomobject]@{user_interactive=[Environment]::UserInteractive;",
            "session_name=[string]$env:SESSIONNAME;ui_automation_available=$uia;drawing_available=$drawing}",
            "|ConvertTo-Json -Compress"
        );
        let output = run_powershell(script)?;
        if !output.status.success() {
            return Err(command_failed("GUI doctor", &output));
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    pub fn run_plan(&self, plan: &GuiPlan, artifact_dir: &Path) -> Result<GuiRunReport, GuiError> {
        require_windows()?;
        plan.validate()?;
        fs::create_dir_all(artifact_dir)?;
        let artifact_dir = fs::canonicalize(artifact_dir)?;

        let mut reports = Vec::with_capacity(plan.actions.len());
        for (index, action) in plan.actions.iter().enumerate() {
            let result = self.run_action(&plan.window_title, action, &artifact_dir);
            match result {
                Ok(detail) => reports.push(GuiActionReport {
                    index,
                    action: action_name(action).into(),
                    passed: true,
                    detail,
                }),
                Err(error) => {
                    reports.push(GuiActionReport {
                        index,
                        action: action_name(action).into(),
                        passed: false,
                        detail: error.to_string(),
                    });
                    return Ok(GuiRunReport {
                        window_title: plan.window_title.clone(),
                        passed: false,
                        actions: reports,
                        artifact_directory: artifact_dir.to_string_lossy().into_owned(),
                    });
                }
            }
        }

        Ok(GuiRunReport {
            window_title: plan.window_title.clone(),
            passed: true,
            actions: reports,
            artifact_directory: artifact_dir.to_string_lossy().into_owned(),
        })
    }

    pub fn run_phase7_fixture(
        &self,
        fixture_script: &Path,
        artifact_dir: &Path,
    ) -> Result<(GuiRunReport, CrashReport), GuiError> {
        require_windows()?;
        validate_fixture_script(fixture_script)?;
        fs::create_dir_all(artifact_dir)?;

        let canonical_script = fs::canonicalize(fixture_script)?;
        let mut child = Command::new(POWERSHELL)
            .args([
                "-NoLogo",
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&canonical_script)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;

        let plan = GuiPlan {
            window_title: PHASE7_FIXTURE_TITLE.into(),
            actions: vec![
                GuiAction::WaitForWindow { timeout_ms: 10_000 },
                GuiAction::SetValue {
                    automation_id: "inputBox".into(),
                    value: "DragonForge Phase 7".into(),
                },
                GuiAction::Invoke {
                    automation_id: "applyButton".into(),
                },
                GuiAction::AssertValue {
                    automation_id: "outputBox".into(),
                    expected: "DragonForge Phase 7".into(),
                },
                GuiAction::Screenshot {
                    name: "phase7-fixture.png".into(),
                },
            ],
        };

        let report = self.run_plan(&plan, artifact_dir)?;
        if !report.passed {
            terminate_child(&mut child);
            return Ok((
                report,
                CrashReport {
                    fixture: PHASE7_FIXTURE_TITLE.into(),
                    expected_exit_code: 23,
                    observed_exit_code: None,
                    captured: false,
                },
            ));
        }

        self.invoke_fixture_crash_button()?;

        let observed = wait_for_exit(&mut child, Duration::from_secs(10))?;
        let crash = CrashReport {
            fixture: PHASE7_FIXTURE_TITLE.into(),
            expected_exit_code: 23,
            observed_exit_code: observed,
            captured: observed == Some(23),
        };
        fs::write(
            artifact_dir.join("phase7-crash-report.json"),
            serde_json::to_vec_pretty(&crash)?,
        )?;

        if !crash.captured {
            return Err(GuiError::UnexpectedFixtureExit(observed));
        }

        Ok((report, crash))
    }

    fn invoke_fixture_crash_button(&self) -> Result<(), GuiError> {
        let script = element_script(
            PHASE7_FIXTURE_TITLE,
            "crashButton",
            "try{$pattern=$control.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern);([System.Windows.Automation.InvokePattern]$pattern).Invoke()}catch [System.Windows.Automation.ElementNotAvailableException]{}",
        );
        let output = run_powershell(&script)?;
        if output.status.success() {
            Ok(())
        } else {
            let detail = String::from_utf8_lossy(&output.stderr);
            if detail.contains("ElementNotAvailable") {
                Ok(())
            } else {
                Err(command_failed("invoke crash fixture control", &output))
            }
        }
    }

    fn run_action(
        &self,
        title: &str,
        action: &GuiAction,
        artifact_dir: &Path,
    ) -> Result<String, GuiError> {
        match action {
            GuiAction::WaitForWindow { timeout_ms } => {
                let script = wait_window_script(title, *timeout_ms);
                success_powershell(&script, "wait for GUI window")?;
                Ok(format!("window appeared within {timeout_ms}ms"))
            }
            GuiAction::SetValue {
                automation_id,
                value,
            } => {
                let script = element_script(
                    title,
                    automation_id,
                    &format!(
                        "$pattern=$control.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern);([System.Windows.Automation.ValuePattern]$pattern).SetValue('{}')",
                        ps_literal(value)
                    ),
                );
                success_powershell(&script, "set UI Automation value")?;
                Ok(format!("set value on {automation_id}"))
            }
            GuiAction::Invoke { automation_id } => {
                let script = element_script(
                    title,
                    automation_id,
                    "$pattern=$control.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern);([System.Windows.Automation.InvokePattern]$pattern).Invoke()",
                );
                success_powershell(&script, "invoke UI Automation control")?;
                Ok(format!("invoked {automation_id}"))
            }
            GuiAction::AssertValue {
                automation_id,
                expected,
            } => {
                let body = format!(
                    "$deadline=[DateTime]::UtcNow.AddSeconds(3);$actual='';do{{$pattern=$control.GetCurrentPattern([System.Windows.Automation.ValuePattern]::Pattern);$actual=([System.Windows.Automation.ValuePattern]$pattern).Current.Value;if($actual -eq '{}'){{break}};Start-Sleep -Milliseconds 50}}while([DateTime]::UtcNow -lt $deadline);if($actual -ne '{}'){{throw ('value mismatch after wait: '+$actual)}}",
                    ps_literal(expected),
                    ps_literal(expected)
                );
                let script = element_script(title, automation_id, &body);
                success_powershell(&script, "assert UI Automation value")?;
                Ok(format!("value matched on {automation_id}"))
            }
            GuiAction::Screenshot { name } => {
                let path = artifact_dir.join(name);
                ensure_descendant(artifact_dir, &path)?;
                let script = screenshot_script(title, &path);
                success_powershell(&script, "capture GUI screenshot")?;
                if !path.is_file() {
                    return Err(GuiError::ScreenshotMissing(path));
                }
                Ok(format!("screenshot={}", path.display()))
            }
        }
    }
}

fn action_name(action: &GuiAction) -> &'static str {
    match action {
        GuiAction::WaitForWindow { .. } => "wait_for_window",
        GuiAction::SetValue { .. } => "set_value",
        GuiAction::Invoke { .. } => "invoke",
        GuiAction::AssertValue { .. } => "assert_value",
        GuiAction::Screenshot { .. } => "screenshot",
    }
}

fn wait_window_script(title: &str, timeout_ms: u64) -> String {
    format!(
        concat!(
            "$ErrorActionPreference='Stop';Add-Type -AssemblyName UIAutomationClient;Add-Type -AssemblyName UIAutomationTypes;",
            "$deadline=[DateTime]::UtcNow.AddMilliseconds({timeout});$window=$null;",
            "do{{$window=[System.Windows.Automation.AutomationElement]::RootElement.FindFirst(",
            "[System.Windows.Automation.TreeScope]::Children,",
            "([System.Windows.Automation.PropertyCondition]::new(",
            "[System.Windows.Automation.AutomationElement]::NameProperty,'{title}')));",
            "if($window){{break}};Start-Sleep -Milliseconds 100}}while([DateTime]::UtcNow -lt $deadline);",
            "if(-not $window){{throw 'managed window not found'}}"
        ),
        timeout = timeout_ms,
        title = ps_literal(title)
    )
}

fn element_script(title: &str, automation_id: &str, body: &str) -> String {
    format!(
        concat!(
            "$ErrorActionPreference='Stop';Add-Type -AssemblyName UIAutomationClient;Add-Type -AssemblyName UIAutomationTypes;",
            "$window=[System.Windows.Automation.AutomationElement]::RootElement.FindFirst(",
            "[System.Windows.Automation.TreeScope]::Children,",
            "([System.Windows.Automation.PropertyCondition]::new(",
            "[System.Windows.Automation.AutomationElement]::NameProperty,'{title}')));",
            "if(-not $window){{throw 'managed window not found'}};",
            "$idCondition=([System.Windows.Automation.PropertyCondition]::new(",
            "[System.Windows.Automation.AutomationElement]::AutomationIdProperty,'{id}'));",
            "$control=$window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$idCondition);",
            "if(-not $control){{$nameCondition=([System.Windows.Automation.PropertyCondition]::new(",
            "[System.Windows.Automation.AutomationElement]::NameProperty,'{id}'));",
            "$control=$window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$nameCondition)}};",
            "if(-not $control){{$helpCondition=([System.Windows.Automation.PropertyCondition]::new(",
            "[System.Windows.Automation.AutomationElement]::HelpTextProperty,'{id}'));",
            "$control=$window.FindFirst([System.Windows.Automation.TreeScope]::Descendants,$helpCondition)}};",
            "if(-not $control){{$children=$window.FindAll([System.Windows.Automation.TreeScope]::Descendants,",
            "[System.Windows.Automation.Condition]::TrueCondition);",
            "$seen=@($children|ForEach-Object{{($_.Current.AutomationId+'|'+$_.Current.Name+'|'+$_.Current.HelpText)}})-join ', ';",
            "throw ('automation control not found: {id}; visible controls: '+$seen)}};",
            "{body}"
        ),
        title = ps_literal(title),
        id = ps_literal(automation_id),
        body = body
    )
}

fn screenshot_script(title: &str, path: &Path) -> String {
    format!(
        concat!(
            "$ErrorActionPreference='Stop';Add-Type -AssemblyName UIAutomationClient;Add-Type -AssemblyName UIAutomationTypes;",
            "Add-Type -AssemblyName System.Drawing;",
            "$window=[System.Windows.Automation.AutomationElement]::RootElement.FindFirst(",
            "[System.Windows.Automation.TreeScope]::Children,",
            "([System.Windows.Automation.PropertyCondition]::new(",
            "[System.Windows.Automation.AutomationElement]::NameProperty,'{title}')));",
            "if(-not $window){{throw 'managed window not found'}};",
            "$r=$window.Current.BoundingRectangle;",
            "$w=[Math]::Max(1,[int][Math]::Ceiling($r.Width));",
            "$h=[Math]::Max(1,[int][Math]::Ceiling($r.Height));",
            "$bmp=New-Object System.Drawing.Bitmap($w,$h);",
            "$g=[System.Drawing.Graphics]::FromImage($bmp);",
            "try{{$g.CopyFromScreen([int]$r.Left,[int]$r.Top,0,0,$bmp.Size);",
            "$bmp.Save('{path}',[System.Drawing.Imaging.ImageFormat]::Png)}}",
            "finally{{$g.Dispose();$bmp.Dispose()}}"
        ),
        title = ps_literal(title),
        path = ps_literal_path(path)
    )
}

fn validate_window_title(title: &str) -> Result<(), GuiError> {
    let valid = title.starts_with(MANAGED_WINDOW_PREFIX)
        && title.len() <= 120
        && title
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | ':'));
    if valid {
        Ok(())
    } else {
        Err(GuiError::InvalidWindowTitle)
    }
}

fn validate_automation_id(id: &str) -> Result<(), GuiError> {
    let valid = !id.is_empty()
        && id.len() <= 96
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(())
    } else {
        Err(GuiError::InvalidAutomationId)
    }
}

fn validate_artifact_name(name: &str) -> Result<(), GuiError> {
    let valid = !name.is_empty()
        && name.len() <= 96
        && name.ends_with(".png")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && !name.contains("..");
    if valid {
        Ok(())
    } else {
        Err(GuiError::InvalidArtifactName)
    }
}

fn validate_fixture_script(path: &Path) -> Result<(), GuiError> {
    if path.file_name().and_then(|value| value.to_str()) != Some(FIXTURE_SCRIPT_NAME) {
        return Err(GuiError::InvalidFixtureScript);
    }
    if !path.is_file() {
        return Err(GuiError::InvalidFixtureScript);
    }
    Ok(())
}

fn ensure_descendant(root: &Path, path: &Path) -> Result<(), GuiError> {
    let parent = path.parent().ok_or(GuiError::InvalidArtifactPath)?;
    let canonical_parent = fs::canonicalize(parent)?;
    if canonical_parent == root {
        Ok(())
    } else {
        Err(GuiError::InvalidArtifactPath)
    }
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<Option<i32>, GuiError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.code());
        }
        if Instant::now() >= deadline {
            terminate_child(child);
            return Err(GuiError::FixtureExitTimeout);
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn terminate_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn run_powershell(script: &str) -> Result<Output, GuiError> {
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

fn success_powershell(script: &str, operation: &str) -> Result<(), GuiError> {
    let output = run_powershell(script)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(command_failed(operation, &output))
    }
}

fn require_windows() -> Result<(), GuiError> {
    if cfg!(windows) {
        Ok(())
    } else {
        Err(GuiError::WindowsRequired)
    }
}

fn ps_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn ps_literal_path(path: &Path) -> String {
    ps_literal(&path.to_string_lossy())
}

fn command_failed(operation: &str, output: &Output) -> GuiError {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    GuiError::CommandFailed {
        operation: operation.to_owned(),
        detail: if stderr.is_empty() { stdout } else { stderr },
    }
}

#[derive(Debug, Error)]
pub enum GuiError {
    #[error("GUI automation requires a Windows host")]
    WindowsRequired,
    #[error("managed window title is invalid or outside the DragonForge-* namespace")]
    InvalidWindowTitle,
    #[error("automation id is invalid")]
    InvalidAutomationId,
    #[error("GUI action count must be between 1 and 128")]
    InvalidActionCount,
    #[error("GUI wait timeout is outside the supported range")]
    InvalidTimeout,
    #[error("GUI value exceeds the supported length")]
    ValueTooLong,
    #[error("screenshot artifact name is invalid")]
    InvalidArtifactName,
    #[error("screenshot artifact path escaped the configured directory")]
    InvalidArtifactPath,
    #[error("Phase 7 fixture script path is invalid")]
    InvalidFixtureScript,
    #[error("fixture process did not exit within the crash-capture timeout")]
    FixtureExitTimeout,
    #[error("fixture exited with an unexpected code: {0:?}")]
    UnexpectedFixtureExit(Option<i32>),
    #[error("expected screenshot was not created: {0}")]
    ScreenshotMissing(PathBuf),
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
    fn managed_titles_are_namespaced() {
        assert!(validate_window_title("DragonForge-GUI-Fixture").is_ok());
        assert!(validate_window_title("Notepad").is_err());
        assert!(validate_window_title("DragonForge-X';calc").is_err());
    }

    #[test]
    fn automation_ids_reject_script_characters() {
        assert!(validate_automation_id("inputBox").is_ok());
        assert!(validate_automation_id("input;whoami").is_err());
        assert!(validate_automation_id("input'bad").is_err());
    }

    #[test]
    fn screenshot_names_are_leaf_png_files() {
        assert!(validate_artifact_name("capture.png").is_ok());
        assert!(validate_artifact_name("../capture.png").is_err());
        assert!(validate_artifact_name("capture.jpg").is_err());
    }

    #[test]
    fn plan_rejects_unmanaged_windows() {
        let plan = GuiPlan {
            window_title: "Calculator".into(),
            actions: vec![GuiAction::WaitForWindow { timeout_ms: 1_000 }],
        };
        assert!(plan.validate().is_err());
    }

    #[test]
    fn powershell_literals_escape_single_quotes() {
        assert_eq!(ps_literal("a'b"), "a''b");
    }
}
