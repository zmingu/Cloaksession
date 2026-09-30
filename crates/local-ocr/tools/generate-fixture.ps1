# Development-only fixture generator. Does not call OCR, read identities, install
# resources, or contact the network. Uses an installed Chinese font through GDI+.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$fixtureDir = Join-Path $PSScriptRoot '../tests/fixtures'
[System.IO.Directory]::CreateDirectory($fixtureDir) | Out-Null
$installed = [System.Drawing.Text.InstalledFontCollection]::new()
$fontName = @('Microsoft YaHei', 'SimHei', 'SimSun') | Where-Object { $installed.Families.Name -contains $_ } | Select-Object -First 1
if (-not $fontName) { throw 'No installed Chinese test font; no automatic installation.' }

# Deliberately fictitious non-issued area 990101; valid date and check digit only.
$prefix = '99010120000101001'
$weights = @(7, 9, 10, 5, 8, 4, 2, 1, 6, 3, 7, 9, 10, 5, 8, 4, 2)
$sum = 0
for ($i = 0; $i -lt 17; $i++) { $sum += [int]::Parse($prefix.Substring($i, 1)) * $weights[$i] }
$number = $prefix + '10X98765432'[$sum % 11]
$bitmap = [System.Drawing.Bitmap]::new(1600, 600)
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$font = [System.Drawing.Font]::new($fontName, 52, [System.Drawing.FontStyle]::Regular, [System.Drawing.GraphicsUnit]::Pixel)
try {
    $graphics.Clear([System.Drawing.Color]::White)
    $graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
    $graphics.DrawString('合成测试资料 非真实证件', $font, [System.Drawing.Brushes]::Black, 60, 55)
    $graphics.DrawString('姓名 测试', $font, [System.Drawing.Brushes]::Black, 60, 180)
    $graphics.DrawString('公民身份号码 ' + $number, $font, [System.Drawing.Brushes]::Black, 60, 310)
    $bitmap.Save((Join-Path $fixtureDir 'synthetic-zh.png'), [System.Drawing.Imaging.ImageFormat]::Png)
} finally {
    $font.Dispose()
    $graphics.Dispose()
    $bitmap.Dispose()
    $installed.Dispose()
}
Write-Output ('Generated synthetic fixture with installed font: ' + $fontName)
