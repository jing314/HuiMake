<#
.SYNOPSIS
    逐个验证 examples/ 下的示范工程，并打印 PASS/FAIL 汇总表。

.DESCRIPTION
    01-single-module / 02-multi-module：真实编译并运行（需要本机有 gcc）
    03-mcu-stm32   / 04-mcu-gd32    ：执行 --print-plan 并检查关键交叉编译参数（不需要交叉工具链）

    每个示例都会先复制到临时目录再构建，因此不会污染 examples/。

.PARAMETER Hk
    hk 可执行文件路径；默认使用 target/debug/hk，不存在时会先 cargo build。

.PARAMETER Keep
    保留临时构建目录，便于排查失败原因。

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
        if (-not (Test-Path -LiteralPath $Hk)) { throw "指定的 hk 不存在: $Hk" }
        return (Resolve-Path -LiteralPath $Hk).Path
    }
    $name = if ($env:OS -eq "Windows_NT") { "hk.exe" } else { "hk" }
    $candidate = Join-Path $root (Join-Path "target/debug" $name)
    if (Test-Path -LiteralPath $candidate) { return $candidate }
    Write-Host "未找到 hk，先执行 cargo build ..." -ForegroundColor Yellow
    Push-Location $root
    & cargo build
    Pop-Location
    if (-not (Test-Path -LiteralPath $candidate)) { throw "构建后仍未找到 $candidate" }
    return $candidate
}

$checks = @(
    [pscustomobject]@{
        Name = "01-single-module"
        WorkDir = "01-single-module/hello"
        Mode = "run"
        Args = @("build")
        Expect = @("构建完成")
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
Write-Host "gcc : $(if ($hasGcc) { '已找到' } else { '未找到（本机示例会被跳过）' })"
Write-Host ""

$results = @()

foreach ($check in $checks) {
    if ($check.Mode -eq "run" -and -not $hasGcc) {
        $results += [pscustomobject]@{ 示例 = $check.Name; 模式 = $check.Mode; 结果 = "SKIP"; 说明 = "本机没有 gcc" }
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
            $note = "构建失败，退出码 $code"
        } else {
            $missing = @($check.Expect | Where-Object { $output -notmatch [regex]::Escape($_) })
            if ($missing.Count -gt 0) {
                $note = "缺少标记: " + ($missing -join ", ")
            } elseif ($check.RunExpect -ne "") {
                $exe = $null
                foreach ($rel in $check.RunExe) {
                    $candidate = Join-Path $work ($rel -replace "/", [string][System.IO.Path]::DirectorySeparatorChar)
                    if (Test-Path -LiteralPath $candidate) { $exe = $candidate; break }
                }
                if ($null -eq $exe) {
                    $note = "没有找到可执行产物"
                } else {
                    $programOutput = & $exe 2>&1 | Out-String
                    if ($programOutput -match [regex]::Escape($check.RunExpect)) {
                        $ok = $true
                        $note = "构建 + 运行通过"
                    } else {
                        $note = "运行输出不符: " + $programOutput.Trim()
                    }
                }
            } else {
                $ok = $true
                $note = ($check.Expect.Count.ToString() + " 项参数检查通过")
            }
        }
    }
    catch {
        $note = "异常: " + $_.Exception.Message
    }
    finally {
        if (Test-Path -LiteralPath $tmp) {
            if (-not $Keep) { Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue }
        }
    }

    $results += [pscustomobject]@{ 示例 = $check.Name; 模式 = $check.Mode; 结果 = $(if ($ok) { "PASS" } else { "FAIL" }); 说明 = $note }
}

$results | Format-Table -AutoSize | Out-String -Width 200 | Write-Host

$failed = @($results | Where-Object { $_.结果 -eq "FAIL" })
if ($failed.Count -gt 0) {
    Write-Host ("有 " + $failed.Count + " 个示范工程验证失败") -ForegroundColor Red
    exit 1
}
Write-Host "全部示范工程验证通过" -ForegroundColor Green
