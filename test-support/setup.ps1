$ErrorActionPreference='Stop'
$workspace=Split-Path $PSScriptRoot
if (-not [Environment]::Is64BitOperatingSystem -or -not $IsWindows) {throw 'Windows x64 development host required'}
$tools=Join-Path $workspace 'outputs\tools'
$node=Join-Path $tools 'node-v24.21.0-win-x64'
if (-not (Test-Path -LiteralPath (Join-Path $node 'node.exe'))) {
    New-Item -ItemType Directory -Force -Path $tools | Out-Null
    $zip=Join-Path $tools 'node.zip'
    Invoke-WebRequest 'https://nodejs.org/dist/v24.21.0/node-v24.21.0-win-x64.zip' -OutFile $zip
    if ((Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash -ne '158F7685B44DE51F6C0DF1D153526CBCD3E1BC739A8DFC607721CEF75DE9E541') {throw 'Node archive checksum mismatch'}
    Expand-Archive -LiteralPath $zip -DestinationPath $tools
}
$env:PATH=$node+';'+$env:PATH
rustup toolchain install 1.99.0 --profile minimal --component rustfmt,clippy
if ($LASTEXITCODE -ne 0) {throw 'Rust toolchain setup failed'}
Push-Location $workspace
try {cargo xtask setup;if($LASTEXITCODE -ne 0){throw 'Dependency setup failed'}}finally{Pop-Location}
