# DragonForge Test Lab — Complete Setup Guide

This guide is for someone starting with a clean Windows PC and little or no prior Hyper-V or Rust experience. It walks from host preparation through a working DragonForge Test Lab checkout, Windows and Ubuntu golden images, disposable Hyper-V test VMs, container support, validation, and the optional service, MCP, dashboard, installer, and release-engineering features.

> Current baseline: DragonForge Test Lab 0.23.0 / Phase 22. Phase 20 ARM/Raspberry Pi support is implemented but still requires qualification on a real ARM/Raspberry Pi host.

---

## 1. What you are building

~~~mermaid
flowchart TB
    Host["Windows Host<br/>DragonForge Test Lab"]
    Repo["Rust workspace + CLI"]
    HV["Hyper-V"]
    WinBase["Immutable Windows base VHDX"]
    LinuxBase["Immutable Ubuntu base VHDX"]
    WinVM["Disposable Windows VM"]
    LinuxVM["Disposable Ubuntu VM"]
    Docker["Docker/Podman sandbox"]
    GitHub["GitHub"]
    Dash["Read-only dashboard"]
    MCP["Loopback MCP gateway"]

    Host --> Repo
    Host --> HV
    HV --> WinBase
    HV --> LinuxBase
    WinBase --> WinVM
    LinuxBase --> LinuxVM
    Repo --> Docker
    Repo --> GitHub
    Repo --> Dash
    Repo --> MCP
~~~

DragonForge is intentionally not a remote shell. Jobs use typed operations and remain subject to controller, policy, agent, executor, sandbox, resource, identity, and audit boundaries.

---

# Part I — Prepare the Windows host

## 2. Hardware and Windows requirements

Microsoft requires a supported Windows edition, a 64-bit processor with SLAT, hardware virtualization enabled in firmware, and sufficient memory.

Minimum practical lab:

- Windows 10/11 Pro, Enterprise, or Education
- 64-bit Intel or AMD CPU with virtualization and SLAT
- 16 GB RAM
- 150 GB free SSD space

Recommended:

- Windows 11 Pro/Enterprise
- 8-core or better CPU
- 32 GB RAM or more
- 300+ GB free SSD/NVMe
- Wired Ethernet for distributed/network testing

The host must have enough memory for Windows plus every VM running at the same time.

Official references:

- https://learn.microsoft.com/windows-server/virtualization/hyper-v/get-started/install-hyper-v
- https://learn.microsoft.com/windows-server/virtualization/hyper-v/host-hardware-requirements

## 3. Enable virtualization in BIOS/UEFI

Look for settings named Intel Virtualization Technology, Intel VT-x, AMD-V, SVM Mode, or similar.

After enabling it, boot Windows and run:

~~~powershell
systeminfo.exe
~~~

Check the Hyper-V requirements near the bottom.

## 4. Confirm Windows edition

~~~powershell
Get-ComputerInfo -Property WindowsProductName,WindowsVersion,OsBuildNumber
~~~

Windows Home does not provide the normal supported client Hyper-V feature.

## 5. Enable Hyper-V

Open PowerShell as Administrator:

~~~powershell
Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V -All
~~~

Restart when prompted.

Verify:

~~~powershell
Get-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V-All
Get-Service vmms
Get-Module -ListAvailable Hyper-V
~~~

Expected: feature enabled, VMMS running, Hyper-V module present.

## 6. Give your normal user Hyper-V permission

DragonForge should not need to run permanently as Administrator just to manage VMs.

From elevated PowerShell:

~~~powershell
Get-LocalGroupMember -Group "Hyper-V Administrators"
Add-LocalGroupMember -Group "Hyper-V Administrators" -Member "COMPUTERNAME\UserName"
~~~

Sign out and back in. Then, from normal PowerShell:

~~~powershell
Get-VMHost
Get-VM
~~~

If both work without Access Denied, the account is ready.

## 7. Install Windows developer prerequisites

### Git

~~~powershell
winget install --id Git.Git -e
git --version
~~~

### Visual Studio Build Tools

Install Build Tools for Visual Studio 2022 and select:

- Desktop development with C++
- MSVC C++ build tools
- Windows 10/11 SDK

This supplies the native linker used by Rust's MSVC target.

### Rust

~~~powershell
winget install --id Rustlang.Rustup -e
~~~

Open a new terminal:

~~~powershell
rustup default stable
rustup component add rustfmt clippy llvm-tools-preview
rustc --version
cargo --version
~~~

### GitHub CLI

~~~powershell
winget install --id GitHub.cli -e
gh --version
gh auth login
~~~

For a private repository, authenticate an account that has access.

### Python

Install a current Python 3 release and verify:

~~~powershell
python --version
~~~

### Optional Docker Desktop

Docker or Podman is required for container sandboxing and some qualification lanes.

Official guide:

https://docs.docker.com/desktop/setup/install/windows-install/

Verify:

~~~powershell
docker version
~~~

Docker access is highly privileged. Treat it like administrative access to the lab host.

---

# Part II — Get DragonForge running on the host

## 8. Clone the repository

Recommended location:

    C:\DragonForge-Test-Lab

~~~powershell
cd C:\
gh repo clone djames1987/DragonForge-Test-Lab
cd C:\DragonForge-Test-Lab
git switch main
git pull --ff-only origin main
git status
~~~

## 9. Build and run the doctor

~~~powershell
cargo build --workspace
cargo run -p dragonforge-test-lab -- doctor
~~~

A healthy current checkout should report:

    phase=22
    status=local_worker_ready

## 10. Run the basic quality gates

~~~powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
~~~

If these pass, the source tree and toolchain are healthy.

---

# Part III — Build the Hyper-V lab

## 11. Create VM storage

Do not put VM disks in the Git repo.

Recommended layout:

    C:\DragonForge-Test-Lab-VMs\
      images\
        windows-base.vhdx
        linux-base.vhdx
      vms\

Create it:

~~~powershell
New-Item -ItemType Directory -Force C:\DragonForge-Test-Lab-VMs | Out-Null
New-Item -ItemType Directory -Force C:\DragonForge-Test-Lab-VMs\images | Out-Null
New-Item -ItemType Directory -Force C:\DragonForge-Test-Lab-VMs\vms | Out-Null
~~~

Your normal DragonForge operator needs Modify access to this tree.

## 12. Choose a virtual switch

~~~powershell
Get-VMSwitch | Format-Table Name,SwitchType
~~~

For first setup, use Default Switch. It is the easiest option and normally gives NAT/internet access.

Optional internal switch:

~~~powershell
New-VMSwitch -Name "DragonForge-TestLab-Internal" -SwitchType Internal
~~~

Optional isolated private switch:

~~~powershell
New-VMSwitch -Name "DragonForge-TestLab-Isolated" -SwitchType Private
~~~

Use an isolated switch for tests that should not reach the host LAN or internet.

---

# Part IV — Create the Windows golden image

## 13. Download Windows media

Use installation media you are licensed to use:

https://www.microsoft.com/software-download/windows11

## 14. Create the Windows preparation VM

Use Hyper-V Manager:

1. New → Virtual Machine.
2. Name: DragonForge-Windows-Base-Prep.
3. Generation 2.
4. 4096 MB minimum, 8192 MB recommended.
5. 2–4 virtual processors.
6. Default Switch.
7. 80 GB dynamic VHDX.
8. Mount the Windows ISO.
9. Secure Boot enabled with the Microsoft Windows template.

## 15. Install Windows

Use a lab-only local account where practical.

Do not put personal browser sessions, password-manager logins, production cloud accounts, SSH private keys, or release-signing keys into the golden image.

Run Windows Update until fully current.

## 16. Install guest development tools

Inside the guest install:

- Git
- Visual Studio Build Tools 2022 + Desktop development with C++
- Rust/rustup
- rustfmt
- clippy
- llvm-tools-preview
- GitHub CLI
- Python 3

Example:

~~~powershell
winget install --id Git.Git -e
winget install --id Rustlang.Rustup -e
winget install --id GitHub.cli -e
rustup default stable
rustup component add rustfmt clippy llvm-tools-preview
~~~

Do not authenticate GitHub in the parent image. Authenticate disposable clones instead.

## 17. Optional Windows guest tools

Install only if needed:

- Docker Desktop
- Windows SDK / SignTool
- PowerShell 7
- Sysinternals

## 18. Clean and shut down the Windows parent

Before freezing it:

1. Remove temporary downloads.
2. Empty the Recycle Bin.
3. Remove temporary credentials.
4. Remove copied private keys.
5. Run Windows Update one final time.
6. Shut down completely.

Optional:

~~~powershell
cleanmgr.exe
~~~

If you need many simultaneously running Windows clones with unique machine identity, use a Sysprep/unattend workflow before freezing the parent. For a small private disposable lab, a non-generalized parent is simpler.

## 19. Promote the Windows VHDX

Make sure the VM is Off.

Identify its VHDX in VM settings, then remove only the temporary VM registration:

~~~powershell
Remove-VM -Name "DragonForge-Windows-Base-Prep"
~~~

Move or copy the prepared VHDX to:

    C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx

Optional read-only safeguard:

~~~powershell
attrib +R C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx
~~~

Critical rule: once differencing children exist, do not boot, mount-write, compact, resize, or otherwise modify the parent VHDX.

---

# Part V — Create the Ubuntu golden image

## 20. Download Ubuntu Server

Use a current Ubuntu Server LTS ISO.

Official Hyper-V guide:

https://ubuntu.com/server/docs/how-to/virtualisation/ubuntu-on-hyper-v/

Ubuntu Server 24.04 LTS is a conservative baseline with broad compatibility.

## 21. Create the Ubuntu preparation VM

In Hyper-V Manager:

1. New → Virtual Machine.
2. Name: DragonForge-Ubuntu-Base-Prep.
3. Generation 2.
4. 4096 MB minimum, 8192 MB recommended.
5. 2–4 vCPUs.
6. Default Switch.
7. 60–80 GB dynamic VHDX.
8. Mount the Ubuntu Server ISO.

Before boot, set:

    Security → Secure Boot Template → Microsoft UEFI Certificate Authority

Ubuntu's official Hyper-V guide calls out this template for manual ISO installs.

## 22. Install Ubuntu Server

During install:

- hostname: dragonforge-base
- user: dragonforge
- enable OpenSSH if desired
- do not import production SSH credentials

After first boot:

~~~bash
sudo apt update
sudo apt full-upgrade -y
sudo reboot
~~~

## 23. Install Ubuntu build prerequisites

~~~bash
sudo apt update
sudo apt install -y build-essential ca-certificates curl git pkg-config libssl-dev clang llvm cmake python3 python3-pip openssh-server
~~~

## 24. Install Rust

~~~bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup default stable
rustup component add rustfmt clippy llvm-tools-preview
rustc --version
cargo --version
~~~

## 25. Install GitHub CLI

Use GitHub's maintained Debian repository. Current commands are maintained here:

https://github.com/cli/cli/blob/trunk/docs/install_linux.md

After installing:

~~~bash
gh --version
~~~

Do not authenticate your personal account in the golden image.

## 26. Install Docker Engine

Use Docker's official Ubuntu instructions:

https://docs.docker.com/engine/install/ubuntu/

After installing Docker Engine:

~~~bash
sudo docker run hello-world
~~~

If you deliberately allow the lab user to run Docker without sudo:

~~~bash
sudo usermod -aG docker "$USER"
~~~

Log out and back in, then:

~~~bash
docker version
~~~

Docker-group membership is effectively privileged access to the machine.

## 27. Prepare Ubuntu for cloning

Before shutdown:

~~~bash
sudo cloud-init clean --logs --machine-id || true
sudo rm -f /etc/ssh/ssh_host_*
sudo rm -rf /tmp/*
sudo rm -rf /var/tmp/*
history -c
sudo poweroff
~~~

This prevents every clone from inheriting the same machine ID and SSH host identity.

## 28. Promote the Ubuntu VHDX

Make sure the VM is Off.

~~~powershell
Remove-VM -Name "DragonForge-Ubuntu-Base-Prep"
~~~

Move/copy its VHDX to:

    C:\DragonForge-Test-Lab-VMs\images\linux-base.vhdx

Optional:

~~~powershell
attrib +R C:\DragonForge-Test-Lab-VMs\images\linux-base.vhdx
~~~

Never modify this parent after differencing children exist.

---

# Part VI — Validate Hyper-V and create disposable VMs

## 29. Hyper-V readiness

~~~powershell
cd C:\DragonForge-Test-Lab
.\scripts\check-hyperv-host.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs -SwitchName "Default Switch"
~~~

Expected:

    status=hyperv_host_ready

## 30. DragonForge VM doctor

~~~powershell
cargo run -p dragonforge-test-lab -- vm-doctor --vm-root C:\DragonForge-Test-Lab-VMs --image-root C:\DragonForge-Test-Lab-VMs\images --switch "Default Switch"
~~~

Expected:

    status=vm_lab_ready

## 31. Create a Windows disposable VM

~~~powershell
cargo run -p dragonforge-test-lab -- vm-create --name DragonForge-Windows-Test-01 --guest-os windows --base-vhdx C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx --vm-root C:\DragonForge-Test-Lab-VMs --image-root C:\DragonForge-Test-Lab-VMs\images --switch "Default Switch" --memory-mib 8192 --processors 4
cargo run -p dragonforge-test-lab -- vm-start --name DragonForge-Windows-Test-01
~~~

DragonForge creates a differencing disk. It does not modify the parent.

## 32. Create an Ubuntu disposable VM

~~~powershell
cargo run -p dragonforge-test-lab -- vm-create --name DragonForge-Ubuntu-Test-01 --guest-os linux --base-vhdx C:\DragonForge-Test-Lab-VMs\images\linux-base.vhdx --vm-root C:\DragonForge-Test-Lab-VMs --image-root C:\DragonForge-Test-Lab-VMs\images --switch "Default Switch" --memory-mib 8192 --processors 4
cargo run -p dragonforge-test-lab -- vm-start --name DragonForge-Ubuntu-Test-01
~~~

The generic DragonForge Linux profile disables Secure Boot for compatibility.

## 33. Give clones unique names

Windows:

~~~powershell
Rename-Computer -NewName "DF-WIN-01" -Restart
~~~

Ubuntu:

~~~bash
sudo hostnamectl set-hostname df-linux-01
hostnamectl
cat /etc/machine-id
ls -l /etc/ssh/ssh_host_*
~~~

A cleaned Ubuntu parent should generate fresh machine identity and SSH host keys.

## 34. Authenticate and clone inside disposable VMs

Windows:

~~~powershell
gh auth login
gh repo clone djames1987/DragonForge-Test-Lab
cd DragonForge-Test-Lab
cargo run -p dragonforge-test-lab -- doctor
~~~

Ubuntu:

~~~bash
gh auth login
gh repo clone djames1987/DragonForge-Test-Lab
cd DragonForge-Test-Lab
cargo run -p dragonforge-test-lab -- doctor
~~~

## 35. Create baseline checkpoints

After per-instance setup, shut each VM down cleanly.

~~~powershell
cargo run -p dragonforge-test-lab -- vm-baseline --name DragonForge-Windows-Test-01
cargo run -p dragonforge-test-lab -- vm-baseline --name DragonForge-Ubuntu-Test-01
~~~

Checkpoint name:

    DragonForge-Baseline

## 36. Restore before destructive tests

~~~powershell
cargo run -p dragonforge-test-lab -- vm-restore --name DragonForge-Windows-Test-01
~~~

Restore powers the VM off, restores DragonForge-Baseline, and starts it.

## 37. Destroy disposable VMs

~~~powershell
cargo run -p dragonforge-test-lab -- vm-destroy --name DragonForge-Windows-Test-01 --vm-root C:\DragonForge-Test-Lab-VMs --confirm
~~~

Managed names must begin with DragonForge-, and destroy requires --confirm.

---

# Part VII — Run major validation lanes

## 38. VM lifecycle

~~~powershell
.\scripts\test-phase4.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs -SwitchName "Default Switch" -BaseVhdx C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx -GuestOs windows
~~~

## 39. Windows integration

~~~powershell
.\scripts\check-windows-integration.ps1
.\scripts\test-phase6.ps1
~~~

Optional privileged service/Event Log lane:

~~~powershell
.\scripts\test-phase6.ps1 -IncludePrivileged
~~~

Run privileged/destructive lanes in a disposable VM where practical.

## 40. GUI automation

~~~powershell
.\scripts\check-gui-host.ps1
.\scripts\test-phase7.ps1
~~~

## 41. Linux qualification

Inside the Ubuntu clone:

~~~bash
cd ~/DragonForge-Test-Lab
bash ./scripts/test-phase19-linux.sh --revision main --install-tools
~~~

Optional extended nightly lanes:

~~~bash
bash ./scripts/test-phase19-linux.sh --revision main --install-tools --include-nightly --fuzz-seconds 30
~~~

---

# Part VIII — Optional advanced services

## 42. Dashboard

~~~powershell
$env:DRAGONFORGE_DASHBOARD_TOKEN = "<at-least-32-random-characters>"
cargo run -p dragonforge-test-lab -- dashboard-serve
~~~

Open:

    http://127.0.0.1:8788/#token=<your-token>

The dashboard is loopback-only and read-only.

## 43. MCP gateway

~~~powershell
$env:DRAGONFORGE_MCP_TOKEN = "<at-least-32-random-characters>"
$env:DRAGONFORGE_MCP_ALLOWED_REPOSITORY_PREFIXES = "https://github.com/djames1987/DragonForge-Test-Lab"
cargo run -p dragonforge-test-lab -- mcp-doctor
cargo run -p dragonforge-test-lab -- mcp-serve
~~~

Endpoint:

    http://127.0.0.1:45890/mcp

The MCP gateway exposes bounded DragonForge tools only. It is not a generic command-execution service.

## 44. mTLS/node identity

Run the built-in real mutual-TLS fixture:

~~~powershell
cargo run -p dragonforge-test-lab -- identity-doctor
cargo run -p dragonforge-test-lab -- identity-fixture
~~~

Persistent worker configuration shape:

~~~json
{
  "worker_id": "dragonforge-worker-01",
  "controller": "127.0.0.1:45900",
  "controller_server_name": "localhost",
  "heartbeat_seconds": 15,
  "max_parallel_jobs": 2,
  "state_path": ".dragonforge-test-lab/worker-state.json",
  "ca_cert_path": "certs/ca.pem",
  "client_cert_path": "certs/worker.pem",
  "client_key_path": "certs/worker-key.pem"
}
~~~

Never commit worker private keys.

## 45. Worker service

~~~powershell
cargo run -p dragonforge-test-lab -- worker-service-doctor
cargo run -p dragonforge-test-lab -- worker-service-fixture
~~~

Foreground worker:

~~~powershell
cargo run -p dragonforge-test-lab -- worker-service-run --config .\examples\phase13-worker.json
~~~

Drain and resume:

~~~powershell
cargo run -p dragonforge-test-lab -- worker-service-drain --config .\examples\phase13-worker.json
cargo run -p dragonforge-test-lab -- worker-service-resume --config .\examples\phase13-worker.json
~~~

The service stays outbound-only.

---

# Part IX — Installer and release engineering

## 46. Windows package/install lifecycle

~~~powershell
.\scripts\package-release-windows.ps1 -Version 0.23.0
.\scripts\install-windows.ps1 -PackageRoot <package-directory>
.\scripts\rollback-windows.ps1
.\scripts\uninstall-windows.ps1
.\scripts\uninstall-windows.ps1 -Purge
~~~

Use elevated PowerShell for install/rollback/uninstall and perform lifecycle testing in a disposable VM restored to DragonForge-Baseline.

## 47. Linux package/install lifecycle

~~~bash
bash ./scripts/package-release-linux.sh 0.23.0
sudo bash ./scripts/install-linux.sh --package-root <package-directory>
sudo bash ./scripts/rollback-linux.sh
sudo bash ./scripts/uninstall-linux.sh
sudo bash ./scripts/uninstall-linux.sh --purge
~~~

## 48. Release qualification

Windows:

~~~powershell
.\scripts\test-phase22.ps1 -InstallTools
~~~

Linux:

~~~bash
bash ./scripts/test-phase22-linux.sh --install-tools
~~~

These cover formatting, strict Clippy, tests, release channel policy, cargo-audit, cargo-deny, CycloneDX SBOM generation, archives, checksums, bundle assembly, and verification.

## 49. Signing

Linux minisign:

~~~bash
export DRAGONFORGE_MINISIGN_SECRET_KEY="$HOME/.dragonforge-signing/dragonforge-release.key"
bash ./scripts/sign-release-linux.sh <artifact>
~~~

Windows Authenticode:

~~~powershell
$env:DRAGONFORGE_WINDOWS_SIGN_CERT_PATH = "C:\path\signing.pfx"
$env:DRAGONFORGE_WINDOWS_SIGN_CERT_PASSWORD = "<password>"
.\scripts\sign-release-windows.ps1 <binary.exe>
~~~

Optional RFC3161 timestamp:

~~~powershell
$env:DRAGONFORGE_WINDOWS_TIMESTAMP_URL = "<timestamp-service-url>"
~~~

Self-signed development certificates are for local qualification. Public releases should use a publicly trusted signing identity or managed signing service.

---

# Part X — Suggested normal workflow

~~~mermaid
flowchart LR
    A["Pull main"] --> B["Develop"]
    B --> C["fmt / clippy / tests"]
    C --> D["Run targeted validation"]
    D --> E["Restore disposable VM"]
    E --> F["Run destructive/integration test"]
    F --> G["Collect logs/artifacts"]
~~~

Typical host commands:

~~~powershell
git pull --ff-only origin main
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
~~~

---

# Part XI — Backups

## 50. Back up golden images

At minimum:

    C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx
    C:\DragonForge-Test-Lab-VMs\images\linux-base.vhdx

Do not rely on Hyper-V checkpoints as your only backup.

## 51. Back up signing material separately

Minisign:

    dragonforge-release.key   secret
    dragonforge-release.pub   public, but back it up

Development Authenticode:

    dragonforge-dev-signing.pfx   secret/private key
    dragonforge-dev-signing.cer   public certificate

Keep private-key backups offline and encrypted. Preserve passphrases separately and securely.

---

# Part XII — Troubleshooting

## 52. Hyper-V says virtualization is unavailable

~~~powershell
systeminfo.exe
~~~

Verify virtualization is enabled in BIOS/UEFI and that the Windows edition supports Hyper-V.

## 53. DragonForge cannot manage VMs

~~~powershell
Get-LocalGroupMember -Group "Hyper-V Administrators"
~~~

After adding an account, sign out and back in.

## 54. Base VHDX problems

Check that the parent:

- still exists at the same path;
- has not been modified;
- has not been moved;
- remains readable by the operator.

A differencing disk depends on its exact parent chain.

## 55. Ubuntu will not boot during preparation

Use:

    Security → Secure Boot Template → Microsoft UEFI Certificate Authority

for the manually installed Generation 2 Ubuntu prep VM.

## 56. Ubuntu clones share identity

~~~bash
sudo cloud-init clean --logs --machine-id
sudo rm -f /etc/ssh/ssh_host_*
sudo poweroff
~~~

Then rebuild the immutable parent.

## 57. GitHub authentication fails

~~~powershell
gh auth status
~~~

or:

~~~bash
gh auth status
~~~

The authenticated GitHub account must have access to the private repository.

## 58. Rust linker errors on Windows

Confirm Visual Studio C++ Build Tools and a Windows SDK are installed.

~~~powershell
rustup show
rustc -vV
cargo -vV
~~~

The normal Windows target should be x86_64-pc-windows-msvc.

## 59. Docker permission denied on Ubuntu

~~~bash
sudo usermod -aG docker "$USER"
~~~

Log out and back in. Docker-group access is privileged.

## 60. Validation fails

DragonForge scripts write timestamped logs under:

    test-logs/

Keep the entire log and investigate the first failed command. Do not disable security/audit gates merely to make a run pass.

---

# Part XIII — Security rules

1. Never bake production credentials into golden images.
2. Never put signing private keys in Git.
3. Never modify an immutable parent VHDX after children exist.
4. Keep worker nodes outbound-only.
5. Keep dashboard and MCP loopback-only unless a separately reviewed deployment is added.
6. Use DragonForge typed actions instead of ad-hoc remote command channels.
7. Restore disposable VMs before destructive tests.
8. Use isolated networking for risky network scenarios.
9. Keep host firmware, Windows, Hyper-V, guests, Rust, Docker, and tools patched.
10. Malware/kernel-exploit research requires stronger dedicated isolation than a normal developer workstation.

---

# Part XIV — Final setup checklist

- [ ] Supported Windows edition installed.
- [ ] BIOS/UEFI virtualization enabled.
- [ ] Hyper-V enabled and VMMS running.
- [ ] Normal operator is in Hyper-V Administrators.
- [ ] Git, C++ Build Tools, Rust, GitHub CLI, and Python installed.
- [ ] DragonForge builds.
- [ ] doctor reports phase=22 and status=local_worker_ready.
- [ ] windows-base.vhdx created.
- [ ] linux-base.vhdx created.
- [ ] Golden images contain no production secrets.
- [ ] Hyper-V readiness reports status=hyperv_host_ready.
- [ ] VM doctor reports status=vm_lab_ready.
- [ ] Windows disposable VM can create/start/baseline/restore/destroy.
- [ ] Ubuntu disposable VM can create/start/baseline/restore/destroy.
- [ ] Ubuntu clone can run linux-doctor.
- [ ] Docker works where required.
- [ ] Validation scripts produce logs.
- [ ] Signing keys are backed up securely if signing is enabled.

When these items are complete, you have a working DragonForge Test Lab suitable for the currently implemented Windows, Linux, Hyper-V, container, distributed, observability, installer, and release-engineering workflows.
