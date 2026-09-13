$ErrorActionPreference = "Stop"
. "$PSScriptRoot/../packaging/windows-release.ps1"

function Assert-Fails {
    param([scriptblock]$Action, [string]$Expected)
    try {
        & $Action | Out-Null
    } catch {
        if ($_.Exception.Message -notlike "*$Expected*") { throw }
        return
    }
    throw "Expected failure containing: $Expected"
}

$script:SignToolExitCode = 0
$script:Verified = @()
$script:Inspected = @()
$script:HelperDependency = "KERNEL32.dll"
function signtool {
    $script:Verified += $args[-1]
    & (Join-Path $PSHOME "pwsh") -NoProfile -Command "exit $script:SignToolExitCode"
}

function dumpbin {
    $script:Inspected += $args[-1]
    $global:LASTEXITCODE = 0
    "    $script:HelperDependency"
}

$root = Join-Path ([IO.Path]::GetTempPath()) "opennow-package-test-$([Guid]::NewGuid())"
try {
    $deployment = New-Item -ItemType Directory "$root/deployment"
    $package = New-Item -ItemType Directory "$root/package/bin"
    $names = @(Get-Content "$PSScriptRoot/../packaging/windows-release-binaries.txt")
    $expected = @("OpenNOW.exe", "opennow-core.exe", "opennow-acceptance-verify.exe", "opennow-update-helper.exe", "opennow-streamer.exe", "opennow_streamer_ffi.dll")
    if (Compare-Object $names $expected) { throw "Unexpected first-party binary contract" }
    foreach ($name in $names) {
        Set-Content "$deployment/$name" "signed $name"
        Copy-Item "$deployment/$name" $package
    }
    Assert-OpenNowSignedPackage -Root "$root/package" -SignedRoot $deployment
    $verifiedBeforeUnsigned = $script:Verified.Count
    Assert-OpenNowPackagePayload -Root "$root/package" -DeploymentRoot $deployment
    if ($script:Verified.Count -ne $verifiedBeforeUnsigned) { throw "Unsigned validation invoked signtool" }
    $script:HelperDependency = "Qt6Core.dll"
    Assert-Fails { Assert-OpenNowPackagePayload -Root "$root/package" -DeploymentRoot $deployment } "without non-system DLLs: qt6core.dll"
    $script:HelperDependency = "KERNEL32.dll"
    if ($script:Inspected.Count -eq 0 -or
        @($script:Inspected | Where-Object { [IO.Path]::GetFileName($_) -ne "opennow-update-helper.exe" }).Count -ne 0) {
        throw "Package validation did not inspect the update helper dependencies"
    }
    foreach ($name in $names) {
        if (-not ($script:Verified | Where-Object { [IO.Path]::GetFileName($_) -eq $name })) {
            throw "$name was not signature-verified"
        }
    }
    foreach ($name in $names) {
        Remove-Item "$package/$name"
        Assert-Fails { Get-OpenNowReleaseBinaries -Root "$root/package" } "Expected exactly one $name"
        Assert-Fails { Assert-OpenNowPackagePayload -Root "$root/package" -DeploymentRoot $deployment } "Expected exactly one $name"
        Copy-Item "$deployment/$name" $package
    }
    Copy-Item "$deployment/opennow-streamer.exe" "$root/package"
    Assert-Fails { Get-OpenNowReleaseBinaries -Root "$root/package" } "found 2"
    Assert-Fails { Assert-OpenNowPackagePayload -Root "$root/package" -DeploymentRoot $deployment } "found 2"
    Remove-Item "$root/package/opennow-streamer.exe"
    foreach ($name in $names) {
        Set-Content "$package/$name" "unsigned Cargo copy"
        Assert-Fails { Assert-OpenNowSignedPackage -Root "$root/package" -SignedRoot $deployment } "differs from the signed deployment copy"
        Assert-Fails { Assert-OpenNowPackagePayload -Root "$root/package" -DeploymentRoot $deployment } "differs from the deployment copy"
        Copy-Item "$deployment/$name" $package -Force
    }
    foreach ($code in @(1, 2, 7)) {
        $script:SignToolExitCode = $code
        Assert-Fails { Invoke-OpenNowSignTool sign /fd SHA256 "$deployment/OpenNOW.exe" } "exit code $code"
        Assert-Fails { Assert-OpenNowSignedPackage -Root "$root/package" -SignedRoot $deployment } "exit code $code"
    }
    $script:SignToolExitCode = 0

    $source = New-Item -ItemType Directory "$root/source"
    $module = (Resolve-Path "$PSScriptRoot/../packaging/WindowsReleaseBinaries.cmake").Path.Replace('\', '/')
    $deployPath = $deployment.FullName.Replace('\', '/')
    @"
cmake_minimum_required(VERSION 3.24)
project(PackageContract NONE)
set(OPENNOW_EXECUTABLE_NAME OpenNOW)
set(CMAKE_INSTALL_BINDIR bin)
add_executable(opennow-qt IMPORTED)
set_target_properties(opennow-qt PROPERTIES IMPORTED_LOCATION "$deployPath/OpenNOW.exe")
include("$module")
install(PROGRAMS "$deployPath/OpenNOW.exe" DESTINATION bin)
set(CPACK_PACKAGE_NAME PackageContract)
set(CPACK_PACKAGE_VERSION 1.0.0)
set(CPACK_GENERATOR ZIP)
include(CPack)
"@ | Set-Content "$source/CMakeLists.txt"
    cmake -S $source -B "$root/build"
    if ($LASTEXITCODE -ne 0) { throw "Fixture configuration failed" }
    cmake --install "$root/build" --prefix "$root/installed"
    if ($LASTEXITCODE -ne 0) { throw "Fixture installation failed" }
    foreach ($name in $names | Where-Object { $_ -ne "OpenNOW.exe" }) {
        if ((Get-FileHash "$root/installed/bin/$name").Hash -ne (Get-FileHash "$deployment/$name").Hash) {
            throw "CMake did not install the deployment copy of $name"
        }
    }
    cpack --config "$root/build/CPackConfig.cmake" -B "$root/archives"
    if ($LASTEXITCODE -ne 0) { throw "Fixture packaging failed" }
    $zip = Get-ChildItem "$root/archives" -Filter *.zip
    Expand-Archive $zip.FullName "$root/unpacked"
    Assert-OpenNowSignedPackage -Root "$root/unpacked" -SignedRoot $deployment
    Assert-OpenNowPackagePayload -Root "$root/unpacked" -DeploymentRoot $deployment

    $workflow = Get-Content "$PSScriptRoot/../../.github/workflows/qt-release-candidate.yml" -Raw
    function Get-CandidateStep {
        param([string]$Name)
        $step = ($workflow -split [regex]::Escape("      - name: $Name`n"), 2)[1]
        $step = ($step -split '      - name:', 2)[0]
        $body = ($step -split '        run: \|\r?\n', 2)[1]
        [scriptblock]::Create(($body -replace '(?m)^          ', ''))
    }
    $workflow = $workflow.Replace("`r`n", "`n")
    $verifyPackages = Get-CandidateStep "Verify MSI and portable ZIP payloads"
    $signBinaries = Get-CandidateStep "Authenticode-sign application binaries"
    New-Item -ItemType Directory "$root/opennow-qt/packaging", "$root/build/release-artifacts", "$root/build/opennow-qt-release/Release" | Out-Null
    Copy-Item "$PSScriptRoot/../packaging/windows-release.ps1", "$PSScriptRoot/../packaging/windows-release-binaries.txt" "$root/opennow-qt/packaging"
    Copy-Item "$deployment/*" "$root/build/opennow-qt-release/Release"
    Copy-Item $zip.FullName "$root/build/release-artifacts/fixture.zip"
    Set-Content "$root/build/release-artifacts/fixture.msi" "MSI extraction fixture"
    $script:ExtractionExitCode = 0
    function Start-Process {
        param($FilePath, [switch]$Wait, [switch]$PassThru, $ArgumentList)
        if ($FilePath -ne "msiexec.exe" -or $ArgumentList -notlike '/a * /qn TARGETDIR=*') {
            throw "Expected MSI administrative extraction"
        }
        New-Item -ItemType Directory "$env:RUNNER_TEMP/msi-expanded" | Out-Null
        Copy-Item "$deployment/*" "$env:RUNNER_TEMP/msi-expanded"
        [pscustomobject]@{ ExitCode = $script:ExtractionExitCode }
    }
    $savedEnvironment = @{}
    foreach ($name in @("RUNNER_TEMP", "WINDOWS_SIGNING_MODE", "WINDOWS_PFX_BASE64", "WINDOWS_PFX_PASSWORD")) {
        $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name)
    }
    Push-Location $root
    try {
        $env:RUNNER_TEMP = $root
        foreach ($mode in @("unsigned", "authenticode")) {
            $env:WINDOWS_SIGNING_MODE = $mode
            $script:Verified = @()
            & $verifyPackages
            if ($mode -eq "unsigned" -and $script:Verified.Count -ne 0) { throw "Unsigned candidate invoked signtool" }
            if ($mode -eq "authenticode") {
                foreach ($directory in @("msi-expanded", "portable")) {
                    foreach ($name in $names) {
                        if (-not ($script:Verified | Where-Object { $_ -like "*$directory*" -and [IO.Path]::GetFileName($_) -eq $name })) {
                            throw "$directory/$name was not signature-verified"
                        }
                    }
                }
            }
            Remove-Item "$root/msi-expanded", "$root/portable" -Recurse -Force
        }
        $env:WINDOWS_SIGNING_MODE = "unsigned"
        $script:ExtractionExitCode = 7
        Assert-Fails { & $verifyPackages } "MSI administrative extraction failed"
        $env:WINDOWS_SIGNING_MODE = "auto"
        Assert-Fails { & $verifyPackages } "Invalid Windows signing mode"
        foreach ($missing in @("WINDOWS_PFX_BASE64", "WINDOWS_PFX_PASSWORD")) {
            $env:WINDOWS_PFX_BASE64 = [Convert]::ToBase64String([byte[]]@(1, 2, 3))
            $env:WINDOWS_PFX_PASSWORD = "fixture password"
            [Environment]::SetEnvironmentVariable($missing, $null)
            Assert-Fails { & $signBinaries } "Authenticode mode requires the Windows PFX and password secrets"
            if (Test-Path "$root/opennow-signing.pfx") { throw "Missing credentials created a PFX" }
        }
    } finally {
        Pop-Location
        foreach ($name in $savedEnvironment.Keys) {
            [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name])
        }
        Remove-Item Function:Start-Process
    }
    Write-Host "Windows package contract tests passed"
} finally {
    Remove-Item $root -Recurse -Force
    Remove-Item Function:signtool
    Remove-Item Function:dumpbin
}
