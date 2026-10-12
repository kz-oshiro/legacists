$ErrorActionPreference = 'Stop'
$workspace = Split-Path $PSScriptRoot
$temporary = Join-Path $workspace ('outputs\isolation\' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $temporary | Out-Null
Copy-Item -LiteralPath (Join-Path $workspace 'crates\legacists-core') -Destination (Join-Path $temporary 'core') -Recurse
$consumer = Join-Path $temporary 'consumer'
New-Item -ItemType Directory -Path (Join-Path $consumer 'src') -Force | Out-Null
@'
[package]
name = "independent-consumer"
version = "0.1.0"
edition = "2021"
[dependencies]
legacists-core = { path = "../core" }
[workspace]
'@ | Set-Content -LiteralPath (Join-Path $consumer 'Cargo.toml') -Encoding utf8NoBOM
@'
use legacists_core::*;
use std::sync::Arc;
fn main() {
    let fixture = FileInput::new("../core/tests/fixtures/mixed").unwrap();
    let snapshot = analyze(AnalysisOptions::default(), &fixture, &Cancellation::default(), |_| {}).unwrap();
    assert_eq!(snapshot.relations.len(), 8);
    assert!(snapshot.relations.iter().all(|r| r.status == Resolution::Confirmed));
    let page = query(&snapshot, &Query::Resources {search:"index.jsp".into(), parent:None}, None, 100).unwrap();
    assert_eq!(page.total, 1);
    let selected = snapshot.resources.iter().find(|r|r.path=="web/index.jsp").unwrap().id.clone();
    let plan = plan_export(Arc::new(snapshot), &[selected], &ExportOptions::default(), &Cancellation::default()).unwrap();
    let reconstructed = reconstruct_sources(&[render_export(&plan,0).unwrap().to_vec()]).unwrap();
    assert_eq!(reconstructed["web/index.jsp"], std::fs::read_to_string("../core/tests/fixtures/mixed/web/index.jsp").unwrap());
    println!("T-21: independent analysis/query/export passed");
}
'@ | Set-Content -LiteralPath (Join-Path $consumer 'src\main.rs') -Encoding utf8NoBOM
Push-Location $consumer
$previousTargetDirectory=$env:CARGO_TARGET_DIR
try {
    $env:CARGO_TARGET_DIR = Join-Path $workspace 'target\isolation'
    cargo generate-lockfile --offline
    if ($LASTEXITCODE -ne 0) { throw 'Isolation lockfile failed' }
    cargo run --locked --offline
    if ($LASTEXITCODE -ne 0) { throw 'Independent consumer failed' }
    $metadata = cargo metadata --locked --offline --format-version 1 | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'Isolation dependency inspection failed' }
    if ($metadata.packages.name | Where-Object { $_ -match '^(tauri|legacists-desktop|wry|vue)' }) { throw 'GUI dependency in Core' }
} finally { if($null -eq $previousTargetDirectory){Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue}else{$env:CARGO_TARGET_DIR=$previousTargetDirectory}; Pop-Location }
