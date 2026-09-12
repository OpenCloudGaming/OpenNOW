param([Parameter(Mandatory=$true)][string]$CorePath)
$ErrorActionPreference = 'Stop'
$start = [Diagnostics.ProcessStartInfo]::new()
$start.FileName = (Resolve-Path -LiteralPath $CorePath).Path
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$core = [Diagnostics.Process]::Start($start)
$stderr = $core.StandardError.ReadToEndAsync()
$sequence = 0
function Request-Local([string]$Method, [hashtable]$Params) {
    $script:sequence++
    $id = "local-store-$script:sequence"
    $core.StandardInput.WriteLine((@{type='request';id=$id;method=$Method;params=$Params} | ConvertTo-Json -Compress))
    $read = $core.StandardOutput.ReadLineAsync()
    if (-not $read.Wait(65000)) { throw "$Method timed out" }
    if ([Text.Encoding]::UTF8.GetByteCount($read.Result) -gt 1048576) { throw 'Oversized response' }
    $response = $read.Result | ConvertFrom-Json
    if ($response.id -ne $id -or -not $response.ok) { throw "$Method failed: $($response.error.message)" }
    return $response.result
}
try {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $first = Request-Local 'catalog.store.local' @{limit=40;cursor='';searchQuery=''}
    if ($first.games.Count -gt 40 -or !$first.hasNextPage -or !$first.cacheComplete) { throw 'Expected a bounded page from the complete saved catalog' }
    $firstMs = $clock.ElapsedMilliseconds
    $next = Request-Local 'catalog.store.local' @{limit=40;cursor=$first.nextCursor;searchQuery=''}
    if (@($next.games | Where-Object { $_.id -in $first.games.id }).Count) { throw 'Adjacent local pages overlap' }
    $panels = Request-Local 'catalog.store.presentation' @{section='panels';metadataOnly=$true}
    foreach ($panel in $panels.items) {
        foreach ($section in $panel.sections) {
            if ($section.games.Count -ne 0) { throw 'Shelf metadata included hidden game payloads' }
        }
    }
    foreach ($category in $first.facets.categories) {
        $shelf = Request-Local 'catalog.store.local' @{limit=6;cursor='';searchQuery='';categoryId=$category.id}
        if ($shelf.games.Count -gt 6 -or $shelf.games.Count -eq 0) { throw "Category could not be opened: $($category.label)" }
    }
    foreach ($query in 'Fortnite','fortntie','cs2') {
        $result = Request-Local 'catalog.store.local' @{limit=6;cursor='';searchQuery=$query}
        if ($result.games.Count -eq 0 -or $result.games.Count -gt 6) { throw "Search failed: $query" }
        [pscustomobject]@{Query=$query;TopResult=$result.games[0].title;Count=$result.games.Count}
    }
    Write-Output ("Catalog={0}; FirstPage={1}; FirstPageMs={2}; Categories={3}; Genres={4}; Stores={5}" -f $first.catalogTotalCount,$first.games.Count,$firstMs,$first.facets.categories.Count,$first.facets.genres.Count,$first.facets.stores.Count)
} finally {
    $core.StandardInput.Close()
    if (-not $core.WaitForExit(3000)) { $core.Kill(); $core.WaitForExit() }
    $core.Dispose()
}
