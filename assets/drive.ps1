# Drives the Shiori window for screenshot sessions: focus, click at
# window-virtual coordinates, paste text, send keys.
# Usage:
#   drive.ps1 click <x> <y>        (screen coords in this process's virtual space)
#   drive.ps1 paste "<text>"       (clipboard + Ctrl+V)
#   drive.ps1 keys "<sendkeys>"    (System.Windows.Forms.SendKeys syntax)
param(
    [Parameter(Mandatory = $true)][string]$Action,
    [string]$A,
    [string]$B
)

Add-Type -AssemblyName System.Windows.Forms
Add-Type -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
[DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
[DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
'@ -Name Drive -Namespace Shiori

$h = (Get-Process shiori -ErrorAction Stop).MainWindowHandle
[Shiori.Drive]::SetForegroundWindow($h) | Out-Null
Start-Sleep -Milliseconds 250

switch ($Action) {
    "click" {
        [Shiori.Drive]::SetCursorPos([int]$A, [int]$B) | Out-Null
        Start-Sleep -Milliseconds 120
        [Shiori.Drive]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)  # down
        Start-Sleep -Milliseconds 60
        [Shiori.Drive]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)  # up
    }
    "paste" {
        Set-Clipboard -Value $A
        Start-Sleep -Milliseconds 120
        [System.Windows.Forms.SendKeys]::SendWait("^v")
    }
    "keys" {
        [System.Windows.Forms.SendKeys]::SendWait($A)
    }
}
Start-Sleep -Milliseconds 350
Write-Output "ok"
