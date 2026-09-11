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
function Request-Artwork([string]$Method, [hashtable]$Params) {
    $script:sequence++
    $id = "artwork-$script:sequence"
    $core.StandardInput.WriteLine((@{type='request';id=$id;method=$Method;params=$Params} | ConvertTo-Json -Compress))
    $read = $core.StandardOutput.ReadLineAsync()
    if (-not $read.Wait(45000)) { throw "$Method timed out" }
    if ([Text.Encoding]::UTF8.GetByteCount($read.Result) -gt 1048576) { throw 'Oversized response' }
    $response = $read.Result | ConvertFrom-Json
    if ($response.id -ne $id -or -not $response.ok) { throw "$Method failed: $($response.error.message)" }
    return $response.result
}
try {
    $panels = Request-Artwork 'catalog.store.presentation' @{section='panels'}
    $games = @($panels.items.sections.games) | Select-Object -First 3
    if ($games.Count -eq 0) { throw 'No shelf games returned' }
    foreach ($game in $games) {
        $search = Request-Artwork 'catalog.store.list' @{limit=100;cursor='';searchQuery=$game.title}
        $match = $search.games | Where-Object id -eq $game.id | Select-Object -First 1
        if (-not $match -or -not $game.imageUrl -or $game.imageUrl -ne $match.imageUrl) {
            throw "Shelf poster does not match the catalog for $($game.title)"
        }
        [pscustomobject]@{Title=$game.title;ShelfArtwork=$game.imageUrl;CatalogArtwork=$match.imageUrl;MatchesCatalog=($game.imageUrl -eq $match.imageUrl)}
    }
} finally {
    $core.StandardInput.Close()
    if (-not $core.WaitForExit(3000)) { $core.Kill() }
    $core.Dispose()
}
