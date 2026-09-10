$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"

$llvmVersion = "22.1.8"
$officialArtifacts = @{
  X64 = @{
    FileName = "LLVM-22.1.8-win64.exe"
    Url = "https://github.com/llvm/llvm-project/releases/download/llvmorg-22.1.8/LLVM-22.1.8-win64.exe"
    Sha256 = "16e5709785fef73c854646241c4a92c5cd574318d1b33c63330dd7721903e55c"
  }
  Arm64 = @{
    FileName = "LLVM-22.1.8-woa64.exe"
    Url = "https://github.com/llvm/llvm-project/releases/download/llvmorg-22.1.8/LLVM-22.1.8-woa64.exe"
    Sha256 = "76f44ef1ba6eeb5a65904e9500f042f588fade49952778ce48f0374daa934396"
  }
}

function Get-LibclangBinFromCandidates {
  $candidates = @()

  if ($env:LIBCLANG_PATH) {
    $candidates += $env:LIBCLANG_PATH
  }

  if ($env:LLVM_PATH) {
    $candidates += $env:LLVM_PATH
    $candidates += (Join-Path $env:LLVM_PATH "bin")
  }

  if ($env:ProgramFiles) {
    $candidates += (Join-Path $env:ProgramFiles "LLVM\bin")
  }

  $programFilesX86 = [Environment]::GetEnvironmentVariable("ProgramFiles(x86)")
  if ($programFilesX86) {
    $candidates += (Join-Path $programFilesX86 "LLVM\bin")
  }

  if ($env:ChocolateyInstall) {
    $candidates += (Join-Path $env:ChocolateyInstall "lib\llvm\tools\LLVM\bin")
  }

  foreach ($candidate in $candidates | Select-Object -Unique) {
    if ($candidate -and (Test-Path (Join-Path $candidate "libclang.dll"))) {
      return $candidate
    }
  }

  return $null
}

function Install-OfficialLlvmArtifact {
  $architecture = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
  switch ($architecture) {
    "X64" { $artifact = $officialArtifacts.X64 }
    "Arm64" { $artifact = $officialArtifacts.Arm64 }
    default { throw "Unsupported Windows LLVM runner architecture: $architecture" }
  }

  $installer = Join-Path $env:RUNNER_TEMP $artifact.FileName
  for ($attempt = 1; $attempt -le 3; $attempt++) {
    try {
      Invoke-WebRequest -Uri $artifact.Url -OutFile $installer
      break
    } catch {
      if ($attempt -eq 3) {
        throw
      }
      Start-Sleep -Seconds (10 * $attempt)
    }
  }

  $actualHash = (Get-FileHash -Algorithm SHA256 $installer).Hash.ToLowerInvariant()
  if ($actualHash -ne $artifact.Sha256) {
    throw "LLVM $llvmVersion installer hash mismatch. Expected $($artifact.Sha256), got $actualHash"
  }

  $process = Start-Process -FilePath $installer -ArgumentList "/S" -Wait -PassThru
  if ($process.ExitCode -ne 0) {
    throw "LLVM $llvmVersion installer exited with code $($process.ExitCode)"
  }
}

$llvmBin = Get-LibclangBinFromCandidates
if (-not $llvmBin) {
  Install-OfficialLlvmArtifact
  $llvmBin = Get-LibclangBinFromCandidates
}

if (-not $llvmBin) {
  throw "LLVM $llvmVersion installation is missing libclang.dll"
}

$llvmBin | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
"LIBCLANG_PATH=$llvmBin" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
Write-Host "Using LLVM libclang from $llvmBin"
