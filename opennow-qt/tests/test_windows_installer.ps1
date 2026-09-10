$ErrorActionPreference = "Stop"
$root = Join-Path ([IO.Path]::GetTempPath()) "opennow-msi-test-$([Guid]::NewGuid())"
$packages = @()
$testNamespace = [Guid]::NewGuid().ToString()
$metadata = (Resolve-Path "$PSScriptRoot/../cmake/BuildMetadata.cmake").Path.Replace('\', '/')
$policy = (Resolve-Path "$PSScriptRoot/../cmake/WindowsInstaller.cmake").Path.Replace('\', '/')

function Invoke-Installer {
    param([string]$Package, [string]$Destination, [int]$Expected = 0)

    $log = Join-Path $root "install-$([Guid]::NewGuid()).log"
    $process = Start-Process msiexec.exe -Wait -PassThru -ArgumentList "/i `"$Package`" /qn /norestart INSTALL_ROOT=`"$Destination`" /l*v `"$log`""
    if ($process.ExitCode -ne $Expected) {
        Get-Content $log | Write-Host
        throw "MSI install returned $($process.ExitCode), expected $Expected"
    }
    if ($Expected -ne 0 -and -not (Select-String -Path $log -SimpleMatch "A later version")) {
        throw "Older MSI failed without the expected downgrade rejection"
    }
}

function Assert-Payload {
    param([string]$Directory, [string]$Version)
    if ((Get-Content (Join-Path $Directory "fixture.txt") -Raw).Trim() -ne $Version) {
        throw "Installed payload does not match $Version"
    }
    $label = if ($Version -like "*-nightly.*") { "OpenNOW Nightly" } else { "OpenNOW" }
    $menu = "OpenNOW MSI Contract $testNamespace $label"
    $links = @(@("CommonPrograms", "Programs") | ForEach-Object {
        $path = Join-Path ([Environment]::GetFolderPath($_)) "$menu/$label.lnk"
        if (Test-Path $path) { $path }
    } | Select-Object -Unique)
    if ($links.Count -ne 1) { throw "Expected one installed Start Menu launcher for $label" }
    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($links[0])
    $workingDirectory = [IO.Path]::GetFullPath($shortcut.WorkingDirectory).TrimEnd([IO.Path]::DirectorySeparatorChar)
    if ($shortcut.TargetPath -ne [IO.Path]::GetFullPath("$Directory/bin/OpenNOW.exe") -or
        $workingDirectory -ne [IO.Path]::GetFullPath("$Directory/bin")) {
        throw "Start Menu launcher does not target the installed bin/OpenNOW.exe"
    }
}

try {
    New-Item -ItemType Directory $root | Out-Null
    foreach ($version in @("1.0.0-nightly.255.1", "1.0.0-nightly.256.1", "1.0.0-nightly.256.2", "1.0.0")) {
        $source = New-Item -ItemType Directory "$root/$version"
        Set-Content "$source/fixture.txt" $version
        @"
cmake_minimum_required(VERSION 3.24)
project(InstallerContract VERSION 1.0.0 LANGUAGES NONE)
set(CMAKE_SYSTEM_PROCESSOR AMD64)
set(OPENNOW_BUILD_VERSION "$version")
include("$metadata")
set(CPACK_PACKAGE_NAME OpenNOW)
set(CPACK_PACKAGE_VENDOR OpenCloudGaming)
set(CPACK_PACKAGE_FILE_NAME "fixture-$version")
include("$policy")
set(CPACK_WIX_PROGRAM_MENU_FOLDER "OpenNOW MSI Contract $testNamespace `${CPACK_PACKAGE_NAME}")
string(UUID CPACK_WIX_UPGRADE_GUID NAMESPACE "$testNamespace"
    NAME "`${CPACK_WIX_UPGRADE_GUID}" TYPE SHA1 UPPER)
set(CPACK_PACKAGE_NAME "OpenNOW MSI Contract `${CPACK_PACKAGE_NAME}")
install(FILES "`${CMAKE_CURRENT_SOURCE_DIR}/fixture.txt" DESTINATION .)
install(FILES "`${CMAKE_CURRENT_SOURCE_DIR}/fixture.txt" DESTINATION bin RENAME OpenNOW.exe)
include(CPack)
"@ | Set-Content "$source/CMakeLists.txt"
        cmake -S $source -B "$source/build"
        if ($LASTEXITCODE -ne 0) { throw "MSI fixture configuration failed" }
        cpack --config "$source/build/CPackConfig.cmake" -G WIX -B "$source/packages"
        if ($LASTEXITCODE -ne 0) { throw "MSI fixture packaging failed" }
        $license = @(Get-ChildItem "$source/packages" -Recurse -Filter License.rtf)
        if ($license.Count -ne 1 -or
            -not (Select-String -Path $license[0].FullName -SimpleMatch "MIT License") -or
            -not (Select-String -Path $license[0].FullName -SimpleMatch "Zortos")) {
            throw "WiX did not receive the project's MIT license"
        }
        $msi = @(Get-ChildItem "$source/packages" -Filter *.msi)
        if ($msi.Count -ne 1) { throw "Expected one fixture MSI" }
        $packages += $msi[0].FullName
    }
    $nightly = "$root/installed/OpenNOW Nightly"
    $stable = "$root/installed/OpenNOW"
    Invoke-Installer $packages[0] $nightly
    Assert-Payload $nightly "1.0.0-nightly.255.1"
    Invoke-Installer $packages[3] $stable
    Assert-Payload $stable "1.0.0"
    Assert-Payload $nightly "1.0.0-nightly.255.1"
    Invoke-Installer $packages[1] $nightly
    Assert-Payload $nightly "1.0.0-nightly.256.1"
    Invoke-Installer $packages[2] $nightly
    Assert-Payload $nightly "1.0.0-nightly.256.2"
    Invoke-Installer $packages[1] $nightly 1603
    Invoke-Installer $packages[0] $nightly 1603
    Assert-Payload $nightly "1.0.0-nightly.256.2"
    Assert-Payload $stable "1.0.0"
    Write-Host "Windows MSI run, retry, downgrade, stable isolation, and Start Menu launcher tests passed"
} finally {
    [array]::Reverse($packages)
    foreach ($package in $packages) {
        $process = Start-Process msiexec.exe -Wait -PassThru -ArgumentList "/x `"$package`" /qn /norestart"
        if ($process.ExitCode -notin 0, 1605) {
            Write-Warning "MSI fixture cleanup returned $($process.ExitCode) for $package"
        }
    }
    Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
}
