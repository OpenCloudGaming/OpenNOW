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
        $matches[0]
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
