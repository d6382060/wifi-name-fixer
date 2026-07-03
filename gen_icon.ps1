# 生成应用图标 icon.ico（蓝底 WiFi 标志，256x256 PNG 封装进 ICO 容器）
Add-Type -AssemblyName System.Drawing
$size = 256
$bmp = New-Object System.Drawing.Bitmap($size, $size)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$bg = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 25, 118, 210))
$g.FillEllipse($bg, 8, 8, $size - 16, $size - 16)
$pen = New-Object System.Drawing.Pen([System.Drawing.Color]::White, 20)
$pen.StartCap = [System.Drawing.Drawing2D.LineCap]::Round
$pen.EndCap = [System.Drawing.Drawing2D.LineCap]::Round
$cx = 128; $cy = 172
foreach ($r in 44, 82, 120) {
    $g.DrawArc($pen, $cx - $r, $cy - $r, $r * 2, $r * 2, 230, 80)
}
$dot = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
$g.FillEllipse($dot, $cx - 14, $cy - 14, 28, 28)
$g.Dispose()
$ms = New-Object System.IO.MemoryStream
$bmp.Save($ms, [System.Drawing.Imaging.ImageFormat]::Png)
$png = $ms.ToArray()
$out = Join-Path $PSScriptRoot "src-tauri\icons\icon.ico"
$fs = [System.IO.File]::Create($out)
$w = New-Object System.IO.BinaryWriter($fs)
$w.Write([UInt16]0); $w.Write([UInt16]1); $w.Write([UInt16]1)
$w.Write([Byte]0);  $w.Write([Byte]0)
$w.Write([Byte]0);  $w.Write([Byte]0)
$w.Write([UInt16]1); $w.Write([UInt16]32)
$w.Write([UInt32]$png.Length); $w.Write([UInt32]22)
$w.Write($png)
$w.Close()
Write-Host "icon written: $((Get-Item $out).Length) bytes"
