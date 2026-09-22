param(
    [string]$RepositoryUrl = "https://github.com/djames1987/DragonForge-Test-Lab.git",
    [string]$Revision = "phase-5-deep-rust-testing",
    [string]$LogDirectory = ".\test-logs",
    [switch]$InstallTools,
    [switch]$IncludeMiri,
    [switch]$IncludeSanitizer,
    [switch]$IncludeFuzz,
    [ValidateRange(1, 3600)]
    [int]$FuzzSeconds = 30
)

$ErrorActionPreference = "Stop"

function Assert-LastExitCode {
    param(
        [Parameter(Mandatory)]
        [string]$Step
    )

    if ($LASTEXITCODE -ne 0) {
        throw "$Step failed: $LASTEXITCODE"
    }
}

if (-not (Test-Path $LogDirectory)) {
    New-Item -ItemType Directory -Path $LogDirectory -Force | Out-Null
}

$timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$logPath = Join-Path $LogDirectory "phase5-validation-$timestamp.log"
Start-Transcript -Path $logPath -Force | Out-Null

try {
    Write-Host "=== DragonForge Test Lab - Phase 5 Validation ==="
    Write-Host "Started: $(Get-Date -Format o)"
    Write-Host "Computer: $env:COMPUTERNAME"
    Write-Host "Working directory: $(Get-Location)"
    Write-Host "Revision: $Revision"
    Write-Host ""

    if ($InstallTools) {
        Write-Host "[setup] Installing/updating mandatory deep Rust tools"
        cargo install --locked cargo-nextest
        Assert-LastExitCode "install cargo-nextest"
        cargo install --locked cargo-llvm-cov
        Assert-LastExitCode "install cargo-llvm-cov"

        if ($IncludeFuzz) {
            cargo install --locked cargo-fuzz
            Assert-LastExitCode "install cargo-fuzz"
        }

        if ($IncludeMiri -or $IncludeSanitizer) {
            rustup toolchain install nightly
            Assert-LastExitCode "install nightly toolchain"
            rustup component add --toolchain nightly rust-src
            Assert-LastExitCode "install nightly rust-src"
        }

        if ($IncludeMiri) {
            rustup component add --toolchain nightly miri
            Assert-LastExitCode "install nightly Miri"
        }
        Write-Host ""
    }

    Write-Host "[0/9] Environment"
    git --version
    cargo --version
    rustc --version
    rustup show active-toolchain
    gh --version
    Write-Host ""

    Write-Host "[1/9] cargo fmt"
    cargo fmt --all -- --check
    Assert-LastExitCode "cargo fmt"

    Write-Host "[2/9] cargo clippy"
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    Assert-LastExitCode "cargo clippy"

    Write-Host "[3/9] cargo test"
    cargo test --workspace --all-features
    Assert-LastExitCode "cargo test"

    Write-Host "[4/9] Deep Rust tool doctor"
    cargo run -p dragonforge-test-lab -- rust-doctor
    Assert-LastExitCode "rust-doctor"

    Write-Host "[5/9] cargo-nextest"
    cargo nextest run --workspace --all-features --no-fail-fast
    Assert-LastExitCode "cargo nextest"

    Write-Host "[6/9] Property tests"
    cargo test -p df-test-protocol property_
    Assert-LastExitCode "property tests"

    Write-Host "[7/9] Coverage summary"
    cargo llvm-cov --workspace --all-features --summary-only
    Assert-LastExitCode "cargo llvm-cov"

    Write-Host "[8/9] Benchmark compile"
    cargo bench -p df-test-protocol --bench capability_derivation --no-run
    Assert-LastExitCode "benchmark compile"

    Write-Host "[9/9] GitHub-aware native worker regression"
    cargo run -p dragonforge-test-lab -- run-github --repo $RepositoryUrl --revision $Revision --sandbox native --no-status
    Assert-LastExitCode "run-github regression"

    if ($IncludeMiri) {
        Write-Host ""
        Write-Host "[optional] Miri - protocol crate"
        cargo +nightly miri setup
        Assert-LastExitCode "Miri setup"
        cargo +nightly miri test -p df-test-protocol
        Assert-LastExitCode "Miri tests"
    }

    $isWindowsHost = [System.Environment]::OSVersion.Platform -eq [System.PlatformID]::Win32NT

    if ($IncludeSanitizer) {
        Write-Host ""
        if ($isWindowsHost) {
            Write-Host "[optional] AddressSanitizer SKIPPED on Windows."
            Write-Host "Run this lane on the Linux worker/VM; Rust's documented ASan target set includes x86_64-unknown-linux-gnu, not x86_64-pc-windows-msvc."
        }
        else {
            Write-Host "[optional] AddressSanitizer - protocol crate"
            $oldRustFlags = $env:RUSTFLAGS
            $oldRustDocFlags = $env:RUSTDOCFLAGS
            try {
                $env:RUSTFLAGS = "-Zsanitizer=address"
                $env:RUSTDOCFLAGS = "-Zsanitizer=address"
                cargo +nightly test -p df-test-protocol -Zbuild-std --target x86_64-unknown-linux-gnu
                Assert-LastExitCode "AddressSanitizer tests"
            }
            finally {
                $env:RUSTFLAGS = $oldRustFlags
                $env:RUSTDOCFLAGS = $oldRustDocFlags
            }
        }
    }

    if ($IncludeFuzz) {
        Write-Host ""
        if ($isWindowsHost) {
            Write-Host "[optional] cargo-fuzz execution SKIPPED on Windows."
            Write-Host "Use the Linux worker/VM for the libFuzzer lane."
        }
        else {
            Write-Host "[optional] Fuzz JobRequest JSON for $FuzzSeconds seconds"
            cargo fuzz run job_request_json -- -max_total_time=$FuzzSeconds
            Assert-LastExitCode "cargo fuzz"
        }
    }

    Write-Host ""
    Write-Host "Phase 5 mandatory deep Rust validation passed."
    if ($IncludeMiri -or $IncludeSanitizer -or $IncludeFuzz) {
        Write-Host "Requested optional lanes completed or were explicitly reported unsupported on this host."
    }
}
catch {
    Write-Host ""
    Write-Host "Phase 5 validation FAILED: $($_.Exception.Message)"
    exit 1
}
finally {
    Write-Host ""
    Write-Host "Log file: $logPath"
    Stop-Transcript | Out-Null
}
