$ErrorActionPreference = 'Stop'
$artifacts = Join-Path $env:LEGACISTS_RUN_DIR 'artifacts'
$version = $env:LEGACISTS_VERSION
if (-not $version -or -not $env:LEGACISTS_CARGO_TARGET_DIR) { throw 'Product version and Cargo target directory are required' }
$exeName = "legacists-desktop-v$version.exe"
$release = Join-Path $env:LEGACISTS_CARGO_TARGET_DIR 'release'
$sourceExe = Join-Path $release 'legacists-desktop.exe'
$exe = Join-Path $artifacts $exeName
Copy-Item -LiteralPath $sourceExe -Destination $exe
$versionInfo = [Diagnostics.FileVersionInfo]::GetVersionInfo($exe)
if ($versionInfo.FileVersion -ne $version -or $versionInfo.ProductVersion -ne $version) { throw 'EXE version does not match the desktop package version' }
$vswhere = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
$dumpbin = Get-ChildItem (Join-Path $vs 'VC\Tools\MSVC\*\bin\Hostx64\x64\dumpbin.exe') | Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
if (-not $dumpbin) { throw 'PE inspection unavailable' }
$dependencies = & $dumpbin /dependents $exe
if ($LASTEXITCODE -ne 0) { throw 'PE inspection failed' }
$dependencies | Set-Content -LiteralPath (Join-Path $artifacts 'pe-dependencies.txt') -Encoding utf8NoBOM
if ($dependencies -match '(?i)WebView2Loader\.dll|VCRUNTIME\d+\.dll|MSVCP\d+\.dll') { throw 'Adjacent runtime DLL required' }
$headers = & $dumpbin /headers $exe
if (-not ($headers -match '8664 machine')) { throw 'Expected x64 PE' }
$zip = Join-Path $artifacts "legacists-v$version-windows-x64.zip"
Compress-Archive -LiteralPath $exe -DestinationPath $zip
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($zip)
try {
    if ($archive.Entries.Count -ne 1 -or $archive.Entries[0].FullName -ne $exeName) { throw 'ZIP contract mismatch' }
    $entryStream = $archive.Entries[0].Open()
    try { $zipHash = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($entryStream)) } finally { $entryStream.Dispose() }
    if ($zipHash -ne (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash) { throw 'ZIP EXE differs from the saved build' }
} finally { $archive.Dispose() }
$localExe = Join-Path $release $exeName
Copy-Item -LiteralPath $exe -Destination $localExe
if ((Get-FileHash -LiteralPath $localExe).Hash -ne (Get-FileHash -LiteralPath $exe).Hash) { throw 'Local EXE differs from the saved build' }
Write-Output "Release PE version/dependency check and versioned single-EXE ZIP passed ($version)."
