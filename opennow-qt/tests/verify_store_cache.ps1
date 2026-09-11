param([Parameter(Mandatory=$true)][string]$CorePath)
$ErrorActionPreference = 'Stop'
$resolvedCore = (Resolve-Path -LiteralPath $CorePath).Path
foreach ($pass in 1, 2) {
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.FileName = $resolvedCore
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardInput = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    $core = [Diagnostics.Process]::Start($start)
    $stderr = $core.StandardError.ReadToEndAsync()
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $sequence = 0
    function Request-StoreCache([string]$Method, [hashtable]$Params) {
        $script:sequence++
        $id = "store-cache-$pass-$script:sequence"
        $core.StandardInput.WriteLine((@{type='request';id=$id;method=$Method;params=$Params} | ConvertTo-Json -Compress))
        $read = $core.StandardOutput.ReadLineAsync()
        if (-not $read.Wait(65000)) { throw "$Method timed out" }
        if ([Text.Encoding]::UTF8.GetByteCount($read.Result) -gt 1048576) { throw 'Oversized response' }
        $response = $read.Result | ConvertFrom-Json
        if ($response.id -ne $id -or -not $response.ok) { throw "$Method failed: $($response.error.message)" }
        if ($pass -eq 2 -and $response.result.cacheHit -ne $true) { throw 'Core restart missed the persistent Store cache' }
        return $response.result
    }
    try {
        $cursor = ''
        $games = [Collections.Generic.HashSet[string]]::new()
        $pages = 0
        $hits = 0
        do {
            $page = Request-StoreCache 'catalog.store.list' @{limit=100;cursor=$cursor;searchQuery=''}
            $pages++
            if ($page.cacheHit) { $hits++ }
            foreach ($game in $page.games) { [void]$games.Add([string]$game.id) }
            if ($page.hasNextPage -and (!$page.nextCursor -or $page.nextCursor -eq $cursor)) { throw 'Cursor did not advance' }
            $cursor = $page.nextCursor
            if ($pages -ge 100 -and $page.hasNextPage) { throw 'Verification page limit reached' }
        } while ($page.hasNextPage)
        foreach ($section in 'marquee','panels','filters') {
            $chrome = Request-StoreCache 'catalog.store.presentation' @{section=$section}
            if ($chrome.cacheHit) { $hits++ }
        }
        if ($pass -eq 1) { $expectedGames = $games.Count }
        elseif ($games.Count -ne $expectedGames) { throw 'Restart changed the cached game count' }
        [pscustomobject]@{Pass=$pass;Games=$games.Count;Pages=$pages;CacheHits=$hits;Seconds=[Math]::Round($clock.Elapsed.TotalSeconds,2)}
    } finally {
        $core.StandardInput.Close()
        if (-not $core.WaitForExit(3000)) { $core.Kill(); $core.WaitForExit() }
        $core.Dispose()
    }
}
