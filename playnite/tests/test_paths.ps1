$ErrorActionPreference = "Stop"
Add-Type -Path @(
    "$PSScriptRoot/../OpenNow.Playnite/Services/OpenNowPath.cs",
    "$PSScriptRoot/OpenNowPathTests.cs"
)
[OpenNowPathTests]::Run()
Write-Host "Playnite Qt executable discovery tests passed"
