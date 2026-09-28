param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$native = Join-Path $root 'target/release/omnibci_matlab_native.dll'
if (-not (Test-Path -LiteralPath $native)) {
    throw "Build the release MEX first: $native"
}

$dist = Join-Path $root 'dist'
$name = "omnibci-matlab-v$Version-windows-x64"
$stage = Join-Path $dist $name
if (Test-Path -LiteralPath $stage) {
    $resolvedRoot = [IO.Path]::GetFullPath($root).TrimEnd('\')
    $resolvedStage = [IO.Path]::GetFullPath($stage)
    if (-not $resolvedStage.StartsWith("$resolvedRoot\dist\", [StringComparison]::OrdinalIgnoreCase)) {
        throw "Refusing to remove stage outside the repository: $resolvedStage"
    }
    Remove-Item -LiteralPath $stage -Recurse -Force
}
New-Item -ItemType Directory -Force -Path (Join-Path $stage 'matlab/+omnibci/private') | Out-Null
Copy-Item -LiteralPath (Join-Path $root 'matlab/+omnibci/Board.m') -Destination (Join-Path $stage 'matlab/+omnibci/Board.m')
Copy-Item -LiteralPath (Join-Path $root 'matlab/+omnibci/decodeFrames.m') -Destination (Join-Path $stage 'matlab/+omnibci/decodeFrames.m')
Copy-Item -LiteralPath (Join-Path $root 'matlab/+omnibci/private/native.m') -Destination (Join-Path $stage 'matlab/+omnibci/private/native.m')
Copy-Item -LiteralPath (Join-Path $root 'matlab/+omnibci/private/normalizeBatch.m') -Destination (Join-Path $stage 'matlab/+omnibci/private/normalizeBatch.m')
Copy-Item -LiteralPath $native -Destination (Join-Path $stage 'matlab/+omnibci/private/omnibci_mex.mexw64')
Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $stage
Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $stage
Copy-Item -LiteralPath (Join-Path $root 'sdk/LICENSE') -Destination (Join-Path $stage 'SDK-LICENSE')
Copy-Item -LiteralPath (Join-Path $root 'examples') -Destination $stage -Recurse

$archive = Join-Path $dist "$name.zip"
Compress-Archive -LiteralPath $stage -DestinationPath $archive -Force
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
Set-Content -LiteralPath (Join-Path $dist "$name.sha256") -Value "$hash  $name.zip" -Encoding ascii
Write-Host "Created $archive"
