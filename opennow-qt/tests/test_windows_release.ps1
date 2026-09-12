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
function signtool {
    $script:Verified += $args[-1]
    & (Join-Path $PSHOME "pwsh") -NoProfile -Command "exit $script:SignToolExitCode"
}

function dumpbin {
    $script:Inspected += $args[-1]
    $global:LASTEXITCODE = 0
    "    KERNEL32.dll"
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
    Assert-OpenNowPackagePayload -Root "$root/package" -DeploymentRoot $deployment
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
        Copy-Item "$deployment/$name" $package
    }
    Copy-Item "$deployment/opennow-streamer.exe" "$root/package"
    Assert-Fails { Get-OpenNowReleaseBinaries -Root "$root/package" } "found 2"
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
    Write-Host "Windows package contract tests passed"
} finally {
    Remove-Item $root -Recurse -Force
    Remove-Item Function:signtool
    Remove-Item Function:dumpbin
}
