$ErrorActionPreference = 'Stop'
$destination = Join-Path $PSScriptRoot '..\apps\desktop\src-tauri\icons\icon.ico'
New-Item -ItemType Directory -Force -Path (Split-Path $destination) | Out-Null
$stream = [IO.MemoryStream]::new()
$writer = [IO.BinaryWriter]::new($stream)
$writer.Write([uint16]0); $writer.Write([uint16]1); $writer.Write([uint16]1)
$writer.Write([byte]32); $writer.Write([byte]32); $writer.Write([byte]0); $writer.Write([byte]0)
$writer.Write([uint16]1); $writer.Write([uint16]32); $writer.Write([uint32](40 + 4096 + 128)); $writer.Write([uint32]22)
$writer.Write([uint32]40); $writer.Write([int32]32); $writer.Write([int32]64)
$writer.Write([uint16]1); $writer.Write([uint16]32)
0..5 | ForEach-Object { $writer.Write([uint32]0) }
for ($y=0; $y -lt 32; $y++) { for ($x=0; $x -lt 32; $x++) {
    $accent = ($x -ge 7 -and $x -le 11 -and $y -ge 7 -and $y -le 25) -or ($x -ge 7 -and $x -le 24 -and $y -ge 7 -and $y -le 11)
    if ($accent) { $writer.Write([byte]212); $writer.Write([byte]190); $writer.Write([byte]64); $writer.Write([byte]255) }
    else { $writer.Write([byte]37); $writer.Write([byte]28); $writer.Write([byte]24); $writer.Write([byte]255) }
} }
$writer.Write([byte[]]::new(128)); $writer.Flush()
[IO.File]::WriteAllBytes($destination, $stream.ToArray())
$writer.Dispose(); $stream.Dispose()
