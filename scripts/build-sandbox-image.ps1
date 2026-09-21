param(
    [ValidateSet("docker", "podman")]
    [string]$Runtime = "docker"
)

$ErrorActionPreference = "Stop"

$image = "dragonforge/test-lab-rust:0.4.0"
$dockerfileDirectory = Join-Path $PSScriptRoot "..\containers\rust-worker"

Write-Host "Building DragonForge Test Lab sandbox image..."
Write-Host "Runtime: $Runtime"
Write-Host "Image: $image"

& $Runtime build --pull -t $image $dockerfileDirectory
if ($LASTEXITCODE -ne 0) {
    throw "$Runtime build failed with exit code $LASTEXITCODE"
}

& $Runtime image inspect $image | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "Sandbox image was not available after build"
}

Write-Host "Sandbox image ready: $image"
