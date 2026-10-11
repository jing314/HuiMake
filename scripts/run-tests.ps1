<#
.SYNOPSIS
    运行 HuiMake 的全部测试：单元测试 + 端到端测试 + 示范工程测试。

.DESCRIPTION
    * 单元测试（src 内 #[test]）与示例单元测试（src/example_tests.rs）
    * 端到端测试（tests/e2e.rs）：真实调用 gcc 编译并运行产物
    * 示范工程测试（tests/examples.rs）：构建 01/02，检查 03/04 的 MCU 计划
    缺少 gcc 的用例会自动跳过。

.PARAMETER Filter
    只运行名字里包含该字符串的用例，例如 example_tests、print_plan。

.PARAMETER UnitOnly
    只跑二进制内的单元测试（不跑 tests/ 下的集成测试）。

.PARAMETER SkipClippy
    跳过 clippy 静态检查。

.PARAMETER Release
    以 release 模式编译测试。

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
    Write-Host "== 环境 ==" -ForegroundColor Cyan
    & cargo --version
    & rustc --version
    if (Get-Command gcc -ErrorAction SilentlyContinue) {
        Write-Host ("gcc: " + (& gcc --version | Select-Object -First 1))
    } else {
        Write-Host "gcc: 未找到（依赖 gcc 的端到端用例会被跳过）" -ForegroundColor Yellow
    }

    if (-not $SkipClippy) {
        Write-Host ""
        Write-Host "== clippy ==" -ForegroundColor Cyan
        & cargo clippy --all-targets -- -D warnings
        if ($LASTEXITCODE -ne 0) { throw "clippy 失败" }
    }

    Write-Host ""
    Write-Host "== cargo test ==" -ForegroundColor Cyan
    $cargoArgs = @("test")
    if ($UnitOnly) { $cargoArgs += @("--bin", "hk") }
    if ($Release) { $cargoArgs += "--release" }
    if ($Filter) { $cargoArgs += @("--", $Filter) }
    Write-Host ("cargo " + ($cargoArgs -join " "))
    & cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw "测试失败" }

    Write-Host ""
    Write-Host "全部测试通过" -ForegroundColor Green
}
finally {
    Pop-Location
}
