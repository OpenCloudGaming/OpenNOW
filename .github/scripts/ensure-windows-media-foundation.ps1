$ErrorActionPreference = "Stop"

$feature = Get-WindowsFeature -Name Server-Media-Foundation
if (-not $feature) {
    throw "The Server-Media-Foundation feature is unavailable on this runner."
}

if (-not $feature.Installed) {
    $result = Install-WindowsFeature -Name Server-Media-Foundation
    $result | Format-List Success, RestartNeeded, ExitCode, FeatureResult
    if (-not $result.Success) {
        throw "Installing Server-Media-Foundation failed with exit code $($result.ExitCode)."
    }
    if ($result.RestartNeeded.ToString() -ne "No") {
        throw "Installing Server-Media-Foundation requires a runner restart."
    }
}

$feature = Get-WindowsFeature -Name Server-Media-Foundation
$dll = Join-Path $env:SystemRoot "System32\mfplat.dll"
if (-not $feature.Installed -or -not (Test-Path $dll)) {
    throw "Windows Media Foundation is unavailable (feature=$($feature.InstallState), mfplat=$((Test-Path $dll)))."
}
