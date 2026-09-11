$ErrorActionPreference = "Stop"

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class OpenNowTestDesktop
{
    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr OpenInputDesktop(uint flags, bool inherit, uint access);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool CloseDesktop(IntPtr desktop);
}
'@

$desktop = [OpenNowTestDesktop]::OpenInputDesktop(0, $false, 1)
$errorCode = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
try {
    $interactive = [Environment]::UserInteractive
    $sessionId = (Get-Process -Id $PID).SessionId
    Write-Host "Windows test desktop: interactive=$interactive session=$sessionId inputDesktop=$($desktop -ne [IntPtr]::Zero)"
    if (-not $interactive -or $sessionId -eq 0 -or $desktop -eq [IntPtr]::Zero) {
        throw "Qt native-window tests require an interactive desktop outside service session 0 (OpenInputDesktop error=$errorCode)."
    }
} finally {
    if ($desktop -ne [IntPtr]::Zero) {
        [void][OpenNowTestDesktop]::CloseDesktop($desktop)
    }
}
