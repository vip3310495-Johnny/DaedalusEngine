# Screenshot helper for development: captures every visible top-level window of a process.
# usage: powershell -File tools\shot.ps1 -ProcName DaedalusEngine -Out C:\path\prefix [-Exe path] [-Args "..."] [-Wait 3]
param(
  [string]$ProcName = "DaedalusEngine",
  [string]$Out = "$env:TEMP\daedalus_shot",
  [string]$Exe = "",
  [string]$Args = "",
  [int]$Wait = 3,
  [switch]$Kill
)
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetWindowThreadProcessId(IntPtr h, out int pid);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetWindowText(IntPtr h, System.Text.StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr hdc, uint flags);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  public struct RECT { public int L, T, R, B; }
  public static List<IntPtr> Windows(int pid) {
    var o = new List<IntPtr>();
    EnumWindows((h, l) => { int p; GetWindowThreadProcessId(h, out p); if (p == pid && IsWindowVisible(h)) o.Add(h); return true; }, IntPtr.Zero);
    return o;
  }
}
"@
[W]::SetProcessDPIAware() | Out-Null
if ($Exe -ne "") {
  if ($Args -ne "") { $p = Start-Process -FilePath $Exe -ArgumentList $Args -PassThru } else { $p = Start-Process -FilePath $Exe -PassThru }
  Start-Sleep -Seconds $Wait
  $procs = @($p)
} else {
  $procs = Get-Process -Name $ProcName -ErrorAction SilentlyContinue
}
$i = 0
foreach ($pr in $procs) {
  foreach ($h in [W]::Windows($pr.Id)) {
    $r = New-Object W+RECT
    [W]::GetWindowRect($h, [ref]$r) | Out-Null
    $w = $r.R - $r.L; $hh = $r.B - $r.T
    if ($w -lt 50 -or $hh -lt 50) { continue }
    $sb = New-Object System.Text.StringBuilder 256
    [W]::GetWindowText($h, $sb, 256) | Out-Null
    $bmp = New-Object System.Drawing.Bitmap $w, $hh
    $g = [System.Drawing.Graphics]::FromImage($bmp)

    $ok = $false

    if (-not $ok) { $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size) }
    $g.Dispose()
    $f = "$Out`_$i.png"
    $bmp.Save($f, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    Write-Output "$f  [$($sb.ToString())] ${w}x${hh}"
    $i++
  }
  if ($Kill) { Stop-Process -Id $pr.Id -Force }
}
