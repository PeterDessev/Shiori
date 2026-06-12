# Captures the Shiori window at native resolution to a PNG.
# Usage: powershell -File capture.ps1 <output.png>
param([Parameter(Mandatory = $true)][string]$Out)

Add-Type -AssemblyName System.Drawing
Add-Type -MemberDefinition @'
[DllImport("dwmapi.dll")]
public static extern int DwmGetWindowAttribute(IntPtr hwnd, int attr, out RECT rect, int size);
[DllImport("user32.dll")]
public static extern bool SetForegroundWindow(IntPtr hWnd);
public struct RECT { public int Left, Top, Right, Bottom; }
'@ -Name Win32Cap -Namespace Shiori

$proc = Get-Process shiori -ErrorAction Stop
$h = $proc.MainWindowHandle
[Shiori.Win32Cap]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 400

$rect = New-Object Shiori.Win32Cap+RECT
# DWMWA_EXTENDED_FRAME_BOUNDS = 9: the visual bounds, without the
# invisible resize borders GetWindowRect includes on Windows 10.
[Shiori.Win32Cap]::DwmGetWindowAttribute($h, 9, [ref]$rect, 16) | Out-Null
$w = $rect.Right - $rect.Left
$ht = $rect.Bottom - $rect.Top

$bmp = New-Object System.Drawing.Bitmap($w, $ht)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bmp.Size)
$g.Dispose()
$bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "captured ${w}x${ht} -> $Out"
