# Hyper-V Host Setup for DragonForge Test Lab

This document prepares a Windows host for DragonForge Test Lab Phase 4 VM Lab.

The goal is to keep one-time host administration separate from normal Test Lab use. Hyper-V installation, BIOS/UEFI virtualization, local-group membership, storage preparation, and virtual-switch setup are host-administration tasks. After setup, the normal Test Lab operator should not need to run the entire lab as Administrator.

## 1. Supported host requirements

Microsoft currently requires the following for Hyper-V on desktop Windows:

- Windows 10 Pro or Enterprise, or Windows 11 Pro or Enterprise.
- A 64-bit CPU with Second Level Address Translation (SLAT).
- Hardware virtualization / VM Monitor Mode extensions enabled in firmware.
- Data Execution Prevention support enabled.
- At least 4 GB RAM; practical Test Lab use should have substantially more.

Microsoft references:

- https://learn.microsoft.com/windows-server/virtualization/hyper-v/get-started/install-hyper-v
- https://learn.microsoft.com/windows-server/virtualization/hyper-v/host-hardware-requirements

Check the Windows edition:

    Get-ComputerInfo -Property WindowsProductName,WindowsVersion,OsBuildNumber

Check hardware virtualization requirements:

    systeminfo.exe

Near the bottom, review the Hyper-V requirements. If Hyper-V is already enabled, Windows may instead report that a hypervisor has been detected.

If firmware virtualization is disabled, enter the computer BIOS/UEFI and enable the virtualization option. Common vendor names include Intel VT-x, Intel Virtualization Technology, AMD-V, or SVM Mode.

## 2. Enable Hyper-V

Open Windows PowerShell as Administrator.

Enable the complete Hyper-V feature:

    Enable-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V -All

Restart when Windows requests it.

Equivalent DISM method:

    DISM /Online /Enable-Feature /All /FeatureName:Microsoft-Hyper-V

After reboot, verify:

    Get-WindowsOptionalFeature -Online -FeatureName Microsoft-Hyper-V-All
    Get-Service vmms
    Get-Module -ListAvailable Hyper-V

Expected state:

- Hyper-V feature: Enabled
- VMMS service: Running
- Hyper-V PowerShell module: present

## 3. Give the normal Test Lab account Hyper-V permissions

Do not make the Test Lab process permanently run as Administrator just to manage VMs.

Instead, add the normal Windows account used for Test Lab to the built-in local group Hyper-V Administrators.

Run an elevated PowerShell session:

    Get-LocalGroupMember -Group "Hyper-V Administrators"

Add the intended operator account, replacing the example:

    Add-LocalGroupMember -Group "Hyper-V Administrators" -Member "COMPUTERNAME\UserName"

Then sign out and sign back in so the new group token is applied.

Verify from a normal, non-elevated PowerShell session:

    Get-VMHost
    Get-VM

If these work without access denied, normal Test Lab VM commands have the required Hyper-V permission.

## 4. Create dedicated VM Lab storage

Do not store VM disks inside the Git repository.

Recommended root:

    C:\DragonForge-Test-Lab-VMs

Recommended layout:

    C:\DragonForge-Test-Lab-VMs\
      images\
        windows-base.vhdx
        linux-base.vhdx
      vms\
        DragonForge-...\

Create it:

    New-Item -ItemType Directory -Force C:\DragonForge-Test-Lab-VMs | Out-Null
    New-Item -ItemType Directory -Force C:\DragonForge-Test-Lab-VMs\images | Out-Null
    New-Item -ItemType Directory -Force C:\DragonForge-Test-Lab-VMs\vms | Out-Null

Make sure the Test Lab operator account has Modify or Full Control on this root.

Phase 4 creates a differencing VHDX for every managed VM. The golden VHDX in images must remain immutable after child disks exist.

## 5. Choose a Hyper-V virtual switch

List switches:

    Get-VMSwitch | Format-Table Name,SwitchType

Test Lab defaults to:

    Default Switch

On Windows 10/11 this is commonly available and convenient. If it is not present, choose another existing switch and pass it with --switch.

Microsoft documents three general switch types:

- External: VM can reach the physical network through a host NIC.
- Internal: VM can communicate with the host and other VMs on that switch.
- Private: VM can communicate only with other VMs on that switch.

Reference:

- https://learn.microsoft.com/windows-server/virtualization/hyper-v/get-started/create-a-virtual-switch-for-hyper-v-virtual-machines

Dedicated internal switch:

    New-VMSwitch -Name "DragonForge-TestLab-Internal" -SwitchType Internal

Private isolated switch:

    New-VMSwitch -Name "DragonForge-TestLab-Isolated" -SwitchType Private

External switch:

    Get-NetAdapter
    New-VMSwitch -Name "DragonForge-TestLab-External" -NetAdapterName "Ethernet"

Creating or changing an external switch can temporarily disrupt host networking.

## 6. Prepare a Windows golden image

Use installation media you are licensed to use.

Create a temporary Generation 2 Hyper-V VM in Hyper-V Manager or PowerShell, give it a normal dynamic VHDX, install Windows, and configure the software you want every clone to inherit.

Recommended preparation:

1. Install Windows.
2. Install all Windows updates.
3. Install Git, Rust/rustup, build tools, and project dependencies needed by your tests.
4. Configure any dedicated in-guest Test Lab account or future guest agent.
5. Remove secrets, browser sessions, SSH private keys, cloud credentials, and personal files.
6. Clear temporary files/logs that should not be cloned.
7. Shut the guest down completely.
8. Confirm the VM is Off.
9. Copy or move the final VHDX to C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx.
10. Remove the temporary preparation VM registration while keeping the VHDX.
11. Do not boot or modify this base VHDX after differencing children are created.

Optionally mark the base VHDX read-only as an additional operational safeguard.

## 7. Prepare a Linux golden image

Create a Generation 2 Hyper-V VM and install the desired Linux distribution.

Recommended preparation:

1. Install the distribution.
2. Apply all updates.
3. Install Git, Rust, build-essential/compiler packages, OpenSSH if desired, and other project dependencies.
4. Confirm Hyper-V Linux integration support is functioning.
5. Remove machine-specific secrets and SSH material that should not be cloned.
6. Shut the guest down.
7. Copy or move the final disk to C:\DragonForge-Test-Lab-VMs\images\linux-base.vhdx.
8. Remove the temporary preparation VM while preserving the VHDX.
9. Keep the base image immutable after clones exist.

Phase 4 disables Secure Boot for the generic Linux profile for compatibility.

## 8. Run the read-only host readiness checker

From the DragonForge Test Lab repository:

    .\scripts\check-hyperv-host.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs -SwitchName "Default Switch"

The checker does not enable Hyper-V or create/delete VMs. It verifies:

- hardware/firmware Hyper-V requirement output;
- Hyper-V feature state;
- Hyper-V PowerShell module;
- VMMS service;
- current-account Hyper-V permission;
- virtual switches;
- VM Lab storage directories;
- existing DragonForge-* VMs.

A ready machine ends with:

    status=hyperv_host_ready

## 9. Run Test Lab VM doctor

    cargo run -p dragonforge-test-lab -- vm-doctor --vm-root C:\DragonForge-Test-Lab-VMs --image-root C:\DragonForge-Test-Lab-VMs\images --switch "Default Switch"

Expected final line:

    status=vm_lab_ready

## 10. Create the first disposable VM

Windows example:

    cargo run -p dragonforge-test-lab -- vm-create --name DragonForge-Windows-Test-01 --guest-os windows --base-vhdx C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx --vm-root C:\DragonForge-Test-Lab-VMs --image-root C:\DragonForge-Test-Lab-VMs\images --switch "Default Switch" --memory-mib 4096 --processors 2

Linux example:

    cargo run -p dragonforge-test-lab -- vm-create --name DragonForge-Linux-Test-01 --guest-os linux --base-vhdx C:\DragonForge-Test-Lab-VMs\images\linux-base.vhdx --vm-root C:\DragonForge-Test-Lab-VMs --image-root C:\DragonForge-Test-Lab-VMs\images --switch "Default Switch" --memory-mib 4096 --processors 2

Test Lab creates a child differencing disk beneath the managed VM directory. It does not copy or modify the golden image.

## 11. Create the clean baseline checkpoint

Intended workflow:

1. Create VM from golden image.
2. Perform per-instance one-time configuration if required.
3. Shut down the guest.
4. Create the baseline:

       cargo run -p dragonforge-test-lab -- vm-baseline --name DragonForge-Windows-Test-01

The checkpoint name is DragonForge-Baseline.

Phase 4 configures managed VMs for Standard checkpoints. Microsoft documents Standard checkpoints as a valid checkpoint type and Restore-VMCheckpoint as the rollback operation.

Reference:

- https://learn.microsoft.com/windows-server/virtualization/hyper-v/checkpoints

## 12. Restore before a destructive test

    cargo run -p dragonforge-test-lab -- vm-restore --name DragonForge-Windows-Test-01

Restore behavior:

1. turn the managed VM off;
2. restore DragonForge-Baseline;
3. start the VM.

## 13. Start, stop, list, and destroy

List managed VMs:

    cargo run -p dragonforge-test-lab -- vm-list

Start:

    cargo run -p dragonforge-test-lab -- vm-start --name DragonForge-Windows-Test-01

Hard stop:

    cargo run -p dragonforge-test-lab -- vm-stop --name DragonForge-Windows-Test-01

Destroy:

    cargo run -p dragonforge-test-lab -- vm-destroy --name DragonForge-Windows-Test-01 --vm-root C:\DragonForge-Test-Lab-VMs --confirm

Destroy is limited to names beginning with DragonForge- and requires --confirm.

## 14. Phase 4 validation

Core validation and host readiness:

    .\scripts\test-phase4.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs -SwitchName "Default Switch"

For full VM lifecycle validation, also provide a prepared base image:

    .\scripts\test-phase4.ps1 -VmRoot C:\DragonForge-Test-Lab-VMs -SwitchName "Default Switch" -BaseVhdx C:\DragonForge-Test-Lab-VMs\images\windows-base.vhdx -GuestOs windows

The full validation creates only DragonForge-Phase4-Validation. It creates a differencing VM, creates a baseline, starts it, restores the baseline, stops it, and destroys it.

## 15. Safety and recovery rules

- Never edit, mount-write, or compact a golden parent VHDX while differencing children depend on it.
- Never place permanent credentials or secrets in a golden image.
- Keep golden images backed up separately.
- Use DragonForge- names only for Test Lab-managed VMs.
- Use vm-destroy --confirm rather than manually deleting managed directories.
- Use checkpoints for short-lived test rollback, not as the only long-term backup.
- Keep the host OS, Hyper-V, guest OSes, and firmware updated.
- For malware, kernel-exploit, or hostile-code testing, use stronger network isolation and dedicated disposable hardware/VM boundaries. Phase 4 is a foundation and does not claim protection from hypervisor escape.
