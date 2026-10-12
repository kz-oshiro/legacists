$ErrorActionPreference = 'Stop'
$artifacts = Join-Path $env:LEGACISTS_RUN_DIR 'artifacts'
$exe = Join-Path $artifacts 'legacists-desktop.exe'
$workspace = Split-Path $PSScriptRoot
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
$zip = Join-Path $artifacts 'legacists-windows-x64.zip'
Compress-Archive -LiteralPath $exe -DestinationPath $zip
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::OpenRead($zip)
try { if ($archive.Entries.Count -ne 1 -or $archive.Entries[0].Name -ne 'legacists-desktop.exe') { throw 'ZIP contract mismatch' } } finally { $archive.Dispose() }
Write-Output 'Release PE dependency check and single-EXE ZIP passed.'
