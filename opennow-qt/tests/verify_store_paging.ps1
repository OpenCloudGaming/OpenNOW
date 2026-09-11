param([Parameter(Mandatory=$true)][string]$CorePath)
$ErrorActionPreference = 'Stop'
$start = [System.Diagnostics.ProcessStartInfo]::new()
$start.FileName = (Resolve-Path -LiteralPath $CorePath).Path
$start.WorkingDirectory = Split-Path -Parent $start.FileName
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$core = [System.Diagnostics.Process]::Start($start)
$stderr = $core.StandardError.ReadToEndAsync()
$sequence = 0
$maximumBytes = 0
function Request-Catalog([string]$Method, [hashtable]$Params) {
    $script:sequence++
    $id = "catalog-verification-$script:sequence"
    $message = @{type='request';id=$id;method=$Method;params=$Params} | ConvertTo-Json -Depth 8 -Compress
    $core.StandardInput.WriteLine($message)
    $read = $core.StandardOutput.ReadLineAsync()
    if (-not $read.Wait(45000)) { throw "$Method timed out" }
    $line = $read.Result
    $bytes = [Text.Encoding]::UTF8.GetByteCount($line)
    if ($bytes -gt 1048576) { throw "$Method exceeded the 1 MiB protocol limit ($bytes bytes)" }
    $script:maximumBytes = [Math]::Max($script:maximumBytes, $bytes)
    $response = $line | ConvertFrom-Json
    if ($response.id -ne $id -or -not $response.ok) { throw "$Method failed: $($response.error.message)" }
    return $response.result
}
try {
    $hello = Request-Catalog 'core.hello' @{protocolVersion=1;shell='qt';shellVersion='0.5.4'}
    if ($hello.capabilities -notcontains 'catalog.storePages.v1') { throw 'Core lacks Store pagination capability' }
    $cursor = ''
    $seenCursors = [System.Collections.Generic.HashSet[string]]::new()
    $games = [System.Collections.Generic.HashSet[string]]::new()
    $pages = 0
    do {
        $page = Request-Catalog 'catalog.store.list' @{limit=100;cursor=$cursor;searchQuery=''}
        $pages++
        foreach ($game in $page.games) { [void]$games.Add([string]$game.id) }
        if ($pages -eq 1 -or $pages % 10 -eq 0) { Write-Host "Verified $pages pages, $($games.Count) unique games; maximum response $maximumBytes bytes" }
        if ($page.hasNextPage) {
            $cursor = [string]$page.nextCursor
            if (-not $cursor -or -not $seenCursors.Add($cursor)) { throw 'Cursor failed to advance' }
        }
        if ($pages -ge 100 -and $page.hasNextPage) { throw 'Catalog exceeded the verification page bound' }
    } while ($page.hasNextPage)
    $sections = @{}
    foreach ($section in @('marquee','panels','filters')) {
        $presentation = Request-Catalog 'catalog.store.presentation' @{section=$section}
        $sections[$section] = @($presentation.items).Count
    }
    $search = Request-Catalog 'catalog.store.list' @{limit=100;cursor='';searchQuery='Fortnite'}
    $alive = Request-Catalog 'core.hello' @{protocolVersion=1;shell='qt';shellVersion='0.5.4'}
    [pscustomobject]@{pages=$pages;uniqueGames=$games.Count;totalCount=$page.totalCount;
        maxResponseBytes=$maximumBytes;protocolLimitBytes=1048576;sections=$sections;
        searchResults=@($search.games).Count;coreStillAlive=(-not $core.HasExited)} | ConvertTo-Json -Depth 4
} finally {
    $core.StandardInput.Close()
    if (-not $core.WaitForExit(3000)) { $core.Kill() }
    $core.Dispose()
}
