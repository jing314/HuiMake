<#
.SYNOPSIS
    Run all HuiMake tests: unit tests + end-to-end tests + example-project tests.

.DESCRIPTION
    * unit tests inside src (#[test]) and the example unit tests (src/example_tests.rs)
    * end-to-end tests (tests/e2e.rs): really compile and run artifacts with gcc
    * example tests (tests/examples.rs): build 01/02, check the MCU plans of 03/04
    Tests that need gcc skip themselves when it is missing.

.PARAMETER Filter
    Only run tests whose name contains this string, e.g. example_tests or print_plan.

.PARAMETER UnitOnly
    Only run the unit tests inside the binary (skip the integration tests under tests/).

.PARAMETER SkipClippy
    Skip the clippy lint step.

.PARAMETER Release
    Compile the tests in release mode.

.EXAMPLE
    pwsh -File scripts/run-tests.ps1
    pwsh -File scripts/run-tests.ps1 -Filter example_tests
    pwsh -File scripts/run-tests.ps1 -UnitOnly -SkipClippy
#>
param(
    [string]$Filter = "",
    [switch]$UnitOnly,
    [switch]$SkipClippy,
    [switch]$Release
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Push-Location $root
try {
    Write-Host "== environment ==" -ForegroundColor Cyan
    & cargo --version
    & rustc --version
    if (Get-Command gcc -ErrorAction SilentlyContinue) {
        Write-Host ("gcc: " + (& gcc --version | Select-Object -First 1))
    } else {
        Write-Host "gcc: not found (tests that need it will be skipped)" -ForegroundColor Yellow
    }

    if (-not $SkipClippy) {
        Write-Host ""
        Write-Host "== clippy ==" -ForegroundColor Cyan
        & cargo clippy --all-targets -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw "clippy failed" }
    }

    Write-Host ""
    Write-Host "== cargo test ==" -ForegroundColor Cyan
    $cargoArgs = @("test")
    if ($UnitOnly) { $cargoArgs += @("--bin", "hk") }
    if ($Release) { $cargoArgs += "--release" }
    if ($Filter) { $cargoArgs += @("--", $Filter) }
    Write-Host ("cargo " + ($cargoArgs -join " "))
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "tests failed" }

    Write-Host ""
    Write-Host "All tests passed" -ForegroundColor Green
}
finally {
    Pop-Location
}
