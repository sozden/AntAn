# assets/logo.svg'den simge dosyalarını üretir (Windows, Microsoft Edge gerekir):
#   assets/logo-512.png, assets/icon-256.png (pencere simgesi), assets/icon.ico (exe simgesi)
# Kullanım:  powershell -ExecutionPolicy Bypass -File tools\make-icons.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$assets = Join-Path $root "assets"
$svg = Join-Path $assets "logo.svg"
$edge = @("${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe", "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $edge) { throw "Microsoft Edge bulunamadı" }

# 1) SVG'yi saydam arka planla 512 px PNG olarak çiz.
$tmp = Join-Path ([IO.Path]::GetTempPath()) "antan-icons"
New-Item -ItemType Directory -Force $tmp | Out-Null
$html = Join-Path $tmp "logo.html"
$uri = ([Uri]$svg).AbsoluteUri
Set-Content $html "<html><body style='margin:0;background:transparent'><img src='$uri' width='512' height='512' style='display:block'></body></html>" -Encoding utf8
$big = Join-Path $assets "logo-512.png"
if (Test-Path $big) { Remove-Item $big }
& $edge --headless=new --disable-gpu --hide-scrollbars --force-device-scale-factor=1 --default-background-color=00000000 --window-size=512,512 "--screenshot=$big" ([Uri]$html).AbsoluteUri | Out-Null
for ($i = 0; $i -lt 50 -and -not (Test-Path $big); $i++) { Start-Sleep -Milliseconds 200 }
if (-not (Test-Path $big)) { throw "Edge görüntü üretmedi" }

# 2) Küçük boyutlar (yüksek kaliteli küçültme).
Add-Type -AssemblyName System.Drawing
$src = [System.Drawing.Image]::FromFile($big)
function Save-Size([int]$n, [string]$path) {
  $bmp = New-Object System.Drawing.Bitmap $n, $n, ([System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
  $g.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
  $g.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
  $g.Clear([System.Drawing.Color]::Transparent)
  $g.DrawImage($src, 0, 0, $n, $n)
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
}
$sizes = 16, 24, 32, 48, 64, 128, 256
$pngs = @{}
foreach ($n in $sizes) {
  $p = Join-Path $tmp "icon-$n.png"
  Save-Size $n $p
  $pngs[$n] = [IO.File]::ReadAllBytes($p)
}
Copy-Item (Join-Path $tmp "icon-256.png") (Join-Path $assets "icon-256.png") -Force
$src.Dispose()

# 3) ICO: başlık + dizin + PNG kayıtları (Windows Vista ve sonrası PNG'li ICO okur).
$ms = New-Object IO.MemoryStream
$w = New-Object IO.BinaryWriter $ms
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]$sizes.Count)
$offset = 6 + 16 * $sizes.Count
foreach ($n in $sizes) {
  $b = if ($n -ge 256) { 0 } else { $n }
  $w.Write([byte]$b); $w.Write([byte]$b); $w.Write([byte]0); $w.Write([byte]0)
  $w.Write([UInt16]1); $w.Write([UInt16]32)
  $w.Write([UInt32]$pngs[$n].Length); $w.Write([UInt32]$offset)
  $offset += $pngs[$n].Length
}
foreach ($n in $sizes) { $w.Write($pngs[$n]) }
[IO.File]::WriteAllBytes((Join-Path $assets "icon.ico"), $ms.ToArray())
$w.Dispose()
Remove-Item $tmp -Recurse -Force
Get-ChildItem $assets | Select-Object Name, Length
