# Builds setup.exe from the portable ZIP layout. setup.exe is the Windows
# first install only. It does not register a Windows Installer product.
param(
    [Parameter(Mandatory = $true)][string]$Payload,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$Arch,
    [Parameter(Mandatory = $true)][string]$OutputFile
)

$ErrorActionPreference = "Stop"
if ($Arch -notin @("x64", "arm64")) {
    throw "Setup architecture must be x64 or arm64"
}
. "$PSScriptRoot/../windows-release.ps1"
$Payload = Resolve-OpenNowSetupPayload -Root $Payload

$expectedHash = "9C73C3BAE7ED48D44112A0F48E66742C00090BDB5BEF71D9D3C056C66E97B732"
$installerUrl = "https://github.com/jrsoftware/issrc/releases/download/is-6_7_3/innosetup-6.7.3.exe"
$toolRoot = Join-Path $env:RUNNER_TEMP "opennow-innosetup-6.7.3"
if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
    $toolRoot = Join-Path ([IO.Path]::GetTempPath()) "opennow-innosetup-6.7.3"
}
$iscc = Join-Path $toolRoot "ISCC.exe"
if (-not (Test-Path -LiteralPath $iscc)) {
    $installer = Join-Path ([IO.Path]::GetTempPath()) "innosetup-6.7.3.exe"
    Invoke-WebRequest -Uri $installerUrl -OutFile $installer
    $hash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash
    if ($hash -ne $expectedHash) {
        throw "Inno Setup installer hash mismatch"
    }
    $process = Start-Process -FilePath $installer -Wait -PassThru -ArgumentList @(
        "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/DIR=$toolRoot"
    )
    if ($process.ExitCode -ne 0) {
        throw "Inno Setup installer failed with exit code $($process.ExitCode)"
    }
}
if (-not (Test-Path -LiteralPath $iscc)) {
    throw "ISCC.exe was not installed"
}

$OutputFile = [IO.Path]::GetFullPath($OutputFile)
$outputDirectory = Split-Path -Parent $OutputFile
$baseName = [IO.Path]::GetFileNameWithoutExtension($OutputFile)
New-Item -ItemType Directory -Force $outputDirectory | Out-Null
$script = (Resolve-Path "$PSScriptRoot/setup.iss").Path
& $iscc $script "/DPayload=$Payload" "/DAppVersion=$Version" "/DArch=$Arch" "/O$outputDirectory" "/F$baseName"
if ($LASTEXITCODE -ne 0) {
    throw "ISCC failed with exit code $LASTEXITCODE"
}
$built = Join-Path $outputDirectory "$baseName.exe"
if (-not (Test-Path -LiteralPath $built -PathType Leaf)) {
    throw "setup.exe was not created"
}
if ((Get-Item -LiteralPath $built).Length -le 0) {
    throw "setup.exe is empty"
}
if ($built -ne $OutputFile) {
    Move-Item -LiteralPath $built -Destination $OutputFile -Force
}
