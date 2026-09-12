$ErrorActionPreference = "Stop"
$PSNativeCommandUseErrorActionPreference = $false
$output = (New-Item -ItemType Directory -Force "build/windows-hdr-diagnostics").FullName
$executable = (Resolve-Path "build/opennow-qt-release/Release/opennow-hdrcolor-tests.exe").Path
$runtime = Split-Path $executable
[pscustomobject]@{
    UserInteractive = [Environment]::UserInteractive
    SessionId = (Get-Process -Id $PID).SessionId
} | ConvertTo-Json | Tee-Object -FilePath "$output/desktop-session.json"

Get-ChildItem $runtime -Filter "Qt6*.dll" | ForEach-Object {
    [pscustomobject]@{ Path = $_.FullName; Version = $_.VersionInfo.FileVersion }
} | Format-Table -AutoSize | Out-String -Width 240 | Set-Content "$output/qt-runtime-versions.txt"
& llvm-readobj.exe --file-headers --coff-imports $executable 2>&1 |
    Set-Content "$output/executable-imports.txt"
Get-ChildItem "$runtime/platforms", "$env:QT_PLUGIN_PATH/platforms" -Filter "*.dll" -ErrorAction SilentlyContinue |
    ForEach-Object {
        [pscustomobject]@{ Path = $_.FullName; Version = $_.VersionInfo.FileVersion }
    } | Format-Table -AutoSize | Out-String -Width 240 | Set-Content "$output/platform-plugins.txt"

$env:QT_QPA_PLATFORM = "windows"
$env:QT_FORCE_STDERR_LOGGING = "1"
$env:QT_DEBUG_PLUGINS = "1"
$env:QT_LOGGING_RULES = "qt.rhi.*=true;qt.scenegraph.*=true;qt.qpa.*=true"
$env:QSG_INFO = "1"
Remove-Item Env:QT_LOGGING_TO_CONSOLE -ErrorAction SilentlyContinue

foreach ($slot in @("-functions", "sdrRoundTripsAndWhiteLevel", "chromeLayerPreservesColorAlphaAndSurfaceChanges")) {
    $name = $slot.TrimStart('-')
    $start = [System.Diagnostics.ProcessStartInfo]::new($executable)
    $start.UseShellExecute = $false
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    foreach ($argument in @($slot, "-o", "$output/$name-test.txt,txt", "-v2")) {
        $start.ArgumentList.Add($argument)
    }
    $process = [System.Diagnostics.Process]::Start($start)
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(60000)) {
        $process.Kill($true)
        $process.WaitForExit()
    }
    $stdout.GetAwaiter().GetResult() | Set-Content "$output/$name-stdout.txt"
    $stderr.GetAwaiter().GetResult() | Set-Content "$output/$name-stderr.txt"
    "$slot exit code: $($process.ExitCode)" | Tee-Object -FilePath "$output/exit-codes.txt" -Append
    $process.Dispose()
}

$archive = Join-Path $env:RUNNER_TEMP "procdump.zip"
Invoke-WebRequest "https://download.sysinternals.com/files/Procdump.zip" -OutFile $archive -TimeoutSec 30
$debugger = Join-Path $env:RUNNER_TEMP "hdr-procdump"
Expand-Archive $archive $debugger -Force
$procdump = Join-Path $debugger "procdump64.exe"
$signature = Get-AuthenticodeSignature $procdump
if ($signature.Status -ne "Valid" -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') {
    throw "ProcDump does not have a valid Microsoft signature."
}
& $procdump -accepteula -e -x $output $executable -o "$output/crash-test.txt,txt" -v2 2>&1 |
    Tee-Object -FilePath "$output/procdump.txt"
$cdb = Join-Path ${env:ProgramFiles(x86)} "Windows Kits/10/Debuggers/x64/cdb.exe"
if (Test-Path $cdb) {
    foreach ($dump in Get-ChildItem $output -Filter "*.dmp") {
        & $cdb -z $dump.FullName -c ".symfix; .sympath+ $runtime; .reload; !analyze -v; kb; q" 2>&1 |
            Tee-Object -FilePath "$output/$($dump.BaseName)-stack.txt"
    }
}
Get-Content "$output/*-stderr.txt", "$output/*-test.txt" -ErrorAction SilentlyContinue
exit 0
