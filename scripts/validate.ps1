param([string]$Binary='target/release/rusty-note.exe')
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$exe=if([IO.Path]::IsPathRooted($Binary)){$Binary}else{Join-Path $root $Binary}
$out=Join-Path $root 'target/validation'
$notes=Join-Path $out 'notes'
New-Item -ItemType Directory -Force $notes | Out-Null
$fixture=Join-Path $root 'tests/fixtures/studio.md'
Copy-Item $fixture (Join-Path $notes 'Studio.md') -Force
Copy-Item (Join-Path $root 'tests/fixtures/moments.md') (Join-Path $notes 'Moments.md') -Force
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class NoteProbe {
    private delegate bool Callback(IntPtr h,IntPtr p);
    [DllImport("user32.dll")] private static extern bool EnumWindows(Callback cb,IntPtr p);
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
    [DllImport("user32.dll",CharSet=CharSet.Unicode)] private static extern int GetClassNameW(IntPtr h,StringBuilder s,int n);
    [StructLayout(LayoutKind.Sequential)] public struct RECT {public int L,T,R,B;}
    [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr ctx);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int z,uint f);
    [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
    [DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr h,IntPtr r,IntPtr g,uint f);
    public static IntPtr Find(int pid) {IntPtr found=IntPtr.Zero; EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);if(owner!=(uint)pid)return true;var s=new StringBuilder(128);GetClassNameW(h,s,128);if(s.ToString()!="RustyNote.Native")return true;found=h;return false;},IntPtr.Zero);return found;}
}
'@
$null=[NoteProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Capture([IntPtr]$hwnd,[string]$path) {
    $r=New-Object NoteProbe+RECT
    if(![NoteProbe]::GetWindowRect($hwnd,[ref]$r)){throw 'Owned window disappeared'}
    $bitmap=New-Object Drawing.Bitmap ($r.R-$r.L),($r.B-$r.T)
    try {
        $g=[Drawing.Graphics]::FromImage($bitmap);$dc=$g.GetHdc()
        try {
            $success=$false
            for($i=0;$i -lt 5 -and !$success;$i++){
                $null=[NoteProbe]::RedrawWindow($hwnd,[IntPtr]::Zero,[IntPtr]::Zero,0x101)
                $success=[NoteProbe]::PrintWindow($hwnd,$dc,2)
                if(!$success){Start-Sleep -Milliseconds 100}
            }
            if(!$success){throw 'Owned-window capture unavailable'}
        }finally{$g.ReleaseHdc($dc);$g.Dispose()}
        $bitmap.Save($path,[Drawing.Imaging.ImageFormat]::Png)
    }finally{$bitmap.Dispose()}
}
foreach($theme in @('dark','light')) {
    $state=Join-Path $out "$theme-state.json";$metrics=Join-Path $out "$theme-metrics.json"
    [IO.File]::WriteAllText($state,'{"x":120,"y":100,"width":600,"height":760,"pinned":false,"selected":null,"light":'+($theme -eq 'light').ToString().ToLower()+'}')
    $argv=@('--notes-dir',('"'+$notes+'"'),'--state-file',('"'+$state+'"'),'--metrics-file',('"'+$metrics+'"'))
    $p=Start-Process -FilePath $exe -ArgumentList $argv -PassThru
    try {
        if(!$p.WaitForInputIdle(15000)){throw 'Native app was not ready'}
        $hwnd=[NoteProbe]::Find($p.Id)
        if($hwnd -eq [IntPtr]::Zero){throw 'Native class not found'}
        Start-Sleep -Milliseconds 600
        $null=[NoteProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero)
        $first=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
        if($first.animation_active -or $first.lines -lt 1){throw 'Initial rendering did not settle'}
        Capture $hwnd (Join-Path $out "$theme-wide.png")
        # Reflow at a narrow size must use prepared Pretext metrics, not remeasure.
        $null=[NoteProbe]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,360,640,0x16)
        Start-Sleep -Milliseconds 150
        $null=[NoteProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero)
        $narrow=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
        if($narrow.font_measurements -ne $first.font_measurements){throw 'Resize remeasured fonts'}
        Capture $hwnd (Join-Path $out "$theme-narrow.png")
        $null=[NoteProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x23,[IntPtr]::Zero)
        Capture $hwnd (Join-Path $out "$theme-bottom.png")
        $null=[NoteProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x24,[IntPtr]::Zero)
        $null=[NoteProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x09,[IntPtr]::Zero)
        Start-Sleep -Milliseconds 350
        Capture $hwnd (Join-Path $out "$theme-library.png")
        $null=[NoteProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x27,[IntPtr]::Zero)
        Start-Sleep -Milliseconds 350
        $null=[NoteProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero)
        $after=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
        if($after.selected -eq $first.selected -or $after.animation_active){throw 'Animated note switch failed'}
        # Move the pointer nowhere: sample a genuinely idle owned process.
        $p.Refresh();$cpu=$p.TotalProcessorTime.TotalMilliseconds
        $null=[NoteProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero)
        $idleBefore=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
        Start-Sleep -Milliseconds 2500
        $p.Refresh();$cpuDelta=$p.TotalProcessorTime.TotalMilliseconds-$cpu
        $null=[NoteProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero)
        $idleAfter=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
        if($idleAfter.paint_count -ne $idleBefore.paint_count){throw 'Idle app kept painting'}
        $report=[PSCustomObject]@{theme=$theme;idle_cpu_ms=$cpuDelta;interval_ms=2500;working_set_mib=[math]::Round($p.WorkingSet64/1MB,2);private_mib=[math]::Round($p.PrivateMemorySize64/1MB,2);paint_count=$idleAfter.paint_count;font_measurements=$idleAfter.font_measurements;animation_active=$idleAfter.animation_active}
        $report|ConvertTo-Json|Set-Content (Join-Path $out "$theme-profile.json")
        $report|Format-List
    }finally{
        if(!$p.HasExited){$null=[NoteProbe]::PostMessageW([NoteProbe]::Find($p.Id),0x10,[IntPtr]::Zero,[IntPtr]::Zero);if(!$p.WaitForExit(10000)){Stop-Process -Id $p.Id -Force;throw 'Owned app did not close'}}
    }
}
Write-Output "Native validation passed. Artifacts: $out"
