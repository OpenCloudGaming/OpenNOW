$ErrorActionPreference = "Stop"
. "$PSScriptRoot/../packaging/windows-release.ps1"

$script:dumpbinExitCode = 0
$script:dependencies = @("    KERNEL32.dll", "    msi.dll", "    bcryptprimitives.dll")
function dumpbin {
    $global:LASTEXITCODE = $script:dumpbinExitCode
    $script:dependencies
}

Assert-OpenNowStandaloneUpdateHelper -Path "test-helper.exe"
foreach ($library in @("VCRUNTIME140.dll", "VCRUNTIME140D.dll", "msvcp140.dll", "ucrtbase.dll",
                       "api-ms-win-crt-runtime-l1-1-0.dll", "Qt6Core.dll", "unexpected.dll")) {
    $script:dependencies = @("    KERNEL32.dll", "    $library")
    $rejected = $false
    try {
        Assert-OpenNowStandaloneUpdateHelper -Path "test-helper.exe"
    } catch {
        if ($_.Exception.Message -notlike "Update helper must run outside*") { throw }
        $rejected = $true
    }
    if (-not $rejected) { throw "Unexpectedly allowed helper dependency $library" }
}

$script:dependencies = @("No import table")
try {
    Assert-OpenNowStandaloneUpdateHelper -Path "test-helper.exe"
    throw "Unexpectedly accepted an unparsed import table"
} catch {
    if ($_.Exception.Message -ne "No update helper PE dependencies found") { throw }
}

$script:dumpbinExitCode = 1
try {
    Assert-OpenNowStandaloneUpdateHelper -Path "test-helper.exe"
    throw "Unexpectedly accepted a failing PE inspector"
} catch {
    if ($_.Exception.Message -ne "Could not inspect update helper dependencies") { throw }
}

$global:LASTEXITCODE = 0
Write-Host "Windows helper system-only PE dependency policy tests passed"
