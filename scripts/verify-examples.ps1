<#
.SYNOPSIS
    Verify every example project under examples/ and print a PASS/FAIL table.

.DESCRIPTION
    01-single-module / 02-multi-module : really compile and run (needs gcc on this host)
    03-mcu-stm32   / 04-mcu-gd32    : run --print-plan and check the key cross flags (no toolchain needed)

    Every example is copied to a temp directory first, so examples/ stays clean.

.PARAMETER Hk
    Path to the hk binary; defaults to target/debug/hk, building it with cargo if needed.

.PARAMETER Keep
    Keep the temporary build directories for troubleshooting.

.EXAMPLE
    pwsh -File scripts/verify-examples.ps1
    pwsh -File scripts/verify-examples.ps1 -Hk target/release/hk.exe
#>
param(
    [string]$Hk = "",
    [switch]$Keep
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$examples = Join-Path $root "examples"

function Resolve-Hk {
    if ($Hk -ne "") {
        if (-not (Test-Path -LiteralPath $Hk)) { throw "the given hk does not exist: $Hk" }
        return (Resolve-Path -LiteralPath $Hk).Path
    }
    $name = if ($env:OS -eq "Windows_NT") { "hk.exe" } else { "hk" }
    $candidate = Join-Path $root (Join-Path "target/debug" $name)
    if (Test-Path -LiteralPath $candidate) { return $candidate }
    Write-Host "hk not found, running cargo build ..." -ForegroundColor Yellow
    Push-Location $root
    & cargo build
    Pop-Location
    if (-not (Test-Path -LiteralPath $candidate)) { throw "still not found after building: $candidate" }
    return $candidate
}

$checks = @(
    [pscustomobject]@{
        Name = "01-single-module"
        WorkDir = "01-single-module/hello"
        Mode = "run"
        Args = @("build")
        Expect = @("Build finished")
        RunExe = @("build/bin/hello.exe", "build/bin/hello")
        RunExpect = "HELLO FROM HK"
    },
    [pscustomobject]@{
        Name = "02-multi-module"
        WorkDir = "02-multi-module"
        Mode = "run"
        Args = @("build")
        Expect = @("libcore.a", "libsensor.a")
        RunExe = @("app/build/bin/app.exe", "app/build/bin/app")
        RunExpect = "sensor=43 core=2 sqrt=4"
    },
    [pscustomobject]@{
        Name = "03-mcu-stm32"
        WorkDir = "03-mcu-stm32"
        Mode = "plan"
        Args = @("build", "-t", "stm32f407", "-m", "mcu-release", "--print-plan")
        Expect = @(
            "arm-none-eabi-gcc",
            "-mcpu=cortex-m4",
            "-mfloat-abi=hard",
            "-specs=nano.specs",
            "-Wl,--gc-sections",
            "-Wl,--print-memory-usage",
            "STM32F407VGTx_FLASH.ld",
            "-DSTM32F407xx",
            "USE_HAL_DRIVER",
            "startup_stm32f407",
            "libbsp.a",
            "libhal.a",
            "firmware.elf",
            "arm-none-eabi-objcopy -O binary",
            "arm-none-eabi-objcopy -O ihex",
            "arm-none-eabi-size"
        )
        RunExe = @()
        RunExpect = ""
    },
    [pscustomobject]@{
        Name = "04-mcu-gd32"
        WorkDir = "04-mcu-gd32"
        Mode = "plan"
        Args = @("build", "-t", "gd32f303", "-m", "gd32-release", "--print-plan")
        Expect = @(
            "arm-none-eabi-gcc",
            "-mcpu=cortex-m4",
            "-DGD32F30X_HD",
            "-DUSE_STDPERIPH_DRIVER",
            "-O2",
            "GD32F303CCT6_FLASH.ld",
            "-Wl,--print-memory-usage",
            "startup_gd32f303",
            "libgd32_driver.a",
            "firmware.elf",
            "arm-none-eabi-objcopy -O binary"
        )
        RunExe = @()
        RunExpect = ""
    }
)

$hk = Resolve-Hk
$hasGcc = [bool](Get-Command gcc -ErrorAction SilentlyContinue)
Write-Host "hk  : $hk"
Write-Host "gcc : $(if ($hasGcc) { 'found' } else { 'not found (host examples will be skipped)' })"
Write-Host ""

$results = @()

foreach ($check in $checks) {
    if ($check.Mode -eq "run" -and -not $hasGcc) {
        $results += [pscustomobject]@{ Example = $check.Name; Mode = $check.Mode; Result = "SKIP"; Note = "gcc is not available" }
        continue
    }

    $tmp = Join-Path ([System.IO.Path]::GetTempPath()) ("hk-verify-" + $check.Name + "-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    Copy-Item -Recurse (Join-Path $examples $check.Name) (Join-Path $tmp $check.Name)
    $work = Join-Path $tmp ($check.WorkDir -replace "/", [string][System.IO.Path]::DirectorySeparatorChar)

    $note = ""
    $ok = $false
    try {
        Push-Location $work
        $argv = $check.Args
        $output = & $hk @argv 2>&1 | Out-String
        $code = $LASTEXITCODE
        Pop-Location

        if ($code -ne 0) {
            $note = "build failed with exit code $code"
        } else {
            $missing = @($check.Expect | Where-Object { $output -notmatch [regex]::Escape($_) })
            if ($missing.Count -gt 0) {
                $note = "missing markers: " + ($missing -join ", ")
            } elseif ($check.RunExpect -ne "") {
                $exe = $null
                foreach ($rel in $check.RunExe) {
                    $candidate = Join-Path $work ($rel -replace "/", [string][System.IO.Path]::DirectorySeparatorChar)
                    if (Test-Path -LiteralPath $candidate) { $exe = $candidate; break }
                }
                if ($null -eq $exe) {
                    $note = "no executable artifact found"
                } else {
                    $programOutput = & $exe 2>&1 | Out-String
                    if ($programOutput -match [regex]::Escape($check.RunExpect)) {
                        $ok = $true
                        $note = "built and ran successfully"
                    } else {
                        $note = "unexpected program output: " + $programOutput.Trim()
                    }
                }
            } else {
                $ok = $true
                $note = ($check.Expect.Count.ToString() + " flag checks passed")
            }
        }
    }
    catch {
        $note = "exception: " + $_.Exception.Message
    }
    finally {
        if (Test-Path -LiteralPath $tmp) {
            if (-not $Keep) { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
        }
    }

    $results += [pscustomobject]@{ Example = $check.Name; Mode = $check.Mode; Result = $(if ($ok) { "PASS" } else { "FAIL" }); Note = $note }
}

$results | Format-Table -AutoSize | Out-String -Width 200 | Write-Host

$failed = @($results | Where-Object { $_.Result -eq "FAIL" })
if ($failed.Count -gt 0) {
    Write-Host ($failed.Count.ToString() + " example(s) failed") -ForegroundColor Red
    exit 1
}
Write-Host "All example projects verified" -ForegroundColor Green
