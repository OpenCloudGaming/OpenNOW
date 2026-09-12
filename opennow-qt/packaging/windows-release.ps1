$ErrorActionPreference = "Stop"

function Invoke-OpenNowSignTool {
    & signtool @args
    if ($LASTEXITCODE -ne 0) {
        throw "signtool $($args[0]) failed with exit code $LASTEXITCODE"
    }
}

function Get-OpenNowReleaseBinaries {
    param([Parameter(Mandatory)][string]$Root)

    $names = @(Get-Content (Join-Path $PSScriptRoot "windows-release-binaries.txt"))
    $files = @(Get-ChildItem $Root -Recurse -File)
    foreach ($name in $names) {
        $matches = @($files | Where-Object Name -EQ $name)
        if ($matches.Count -ne 1) {
            throw "Expected exactly one $name under $Root, found $($matches.Count)"
        }
        if ($name -eq "opennow-update-helper.exe") {
            Assert-OpenNowStandaloneUpdateHelper -Path $matches[0].FullName
        }
        $matches[0]
    }
}

function Assert-OpenNowStandaloneUpdateHelper {
    param([Parameter(Mandatory)][string]$Path)

    $output = & dumpbin /dependents $Path
    if ($LASTEXITCODE -ne 0) { throw "Could not inspect update helper dependencies" }
    $dependencies = @($output | ForEach-Object {
        if ($_ -match '^\s+([A-Za-z0-9_.-]+\.dll)\s*$') { $Matches[1].ToLowerInvariant() }
    })
    if ($dependencies.Count -eq 0) { throw "No update helper PE dependencies found" }
    $systemLibraries = @("advapi32.dll", "bcrypt.dll", "bcryptprimitives.dll", "combase.dll",
        "crypt32.dll", "gdi32.dll", "iphlpapi.dll", "kernel32.dll", "kernelbase.dll", "msi.dll",
        "netapi32.dll", "normaliz.dll", "ntdll.dll", "ole32.dll", "oleaut32.dll", "psapi.dll",
        "rpcrt4.dll", "secur32.dll", "shell32.dll", "shlwapi.dll", "user32.dll", "userenv.dll",
        "wintrust.dll", "ws2_32.dll")
    foreach ($dependency in $dependencies) {
        if ($dependency -notin $systemLibraries -and $dependency -notlike "api-ms-win-core-*.dll") {
            throw "Update helper must run outside the installation without non-system DLLs: $dependency"
        }
    }
}

function Assert-OpenNowSignedPackage {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][string]$SignedRoot
    )

    $signed = @{}
    foreach ($file in Get-OpenNowReleaseBinaries -Root $SignedRoot) {
        $signed[$file.Name] = (Get-FileHash $file.FullName -Algorithm SHA256).Hash
    }
    foreach ($file in Get-OpenNowReleaseBinaries -Root $Root) {
        Invoke-OpenNowSignTool verify /pa /all $file.FullName
        if ((Get-FileHash $file.FullName -Algorithm SHA256).Hash -ne $signed[$file.Name]) {
            throw "Packaged $($file.Name) differs from the signed deployment copy"
        }
    }
    Get-ChildItem $Root -Recurse -File -Filter *.exe | ForEach-Object {
        Invoke-OpenNowSignTool verify /pa /all $_.FullName
    }
}

function Assert-OpenNowPackagePayload {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][string]$DeploymentRoot
    )

    $deployment = @{}
    foreach ($file in Get-OpenNowReleaseBinaries -Root $DeploymentRoot) {
        $deployment[$file.Name] = (Get-FileHash $file.FullName -Algorithm SHA256).Hash
    }
    foreach ($file in Get-OpenNowReleaseBinaries -Root $Root) {
        if ((Get-FileHash $file.FullName -Algorithm SHA256).Hash -ne $deployment[$file.Name]) {
            throw "Packaged $($file.Name) differs from the deployment copy"
        }
    }
}
