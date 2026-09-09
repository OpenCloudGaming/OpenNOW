$ErrorActionPreference = "Stop"

if (Test-Path O:\) {
    throw "The stable Windows CI workspace drive O: is already in use."
}
subst O: $env:GITHUB_WORKSPACE
if ($LASTEXITCODE -ne 0 -or -not (Test-Path O:\opennow-qt\CMakeLists.txt)) {
    throw "Could not map the checkout to the stable Windows CI workspace O:."
}
