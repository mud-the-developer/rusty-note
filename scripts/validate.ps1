param([string]$Binary='target/release/rusty-note.exe')
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$exe=if([IO.Path]::IsPathRooted($Binary)){$Binary}else{Join-Path $root $Binary}
$out=Join-Path $root 'target/validation'
$run=[Guid]::NewGuid().ToString('N')
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
    [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h,int id);
    [DllImport("user32.dll",CharSet=CharSet.Unicode,EntryPoint="SendMessageW")] public static extern IntPtr SetText(IntPtr h,uint m,IntPtr w,string text);
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
    [DllImport("user32.dll")] public static extern bool RedrawWindow(IntPtr h,IntPtr r,IntPtr g,uint f);
    public static IntPtr Prompt(int pid) {IntPtr found=IntPtr.Zero; EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);if(owner!=(uint)pid)return true;var s=new StringBuilder(128);GetClassNameW(h,s,128);if(s.ToString()!="RustyNote.Prompt")return true;found=h;return false;},IntPtr.Zero);return found;}
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
    $notes=Join-Path $out "$theme-notes-$run"
    New-Item -ItemType Directory -Force $notes | Out-Null
    Copy-Item (Join-Path $root 'tests/fixtures/studio.md') (Join-Path $notes 'Studio.md')
    Copy-Item (Join-Path $root 'tests/fixtures/complete.md') (Join-Path $notes 'Complete.md')
    Copy-Item (Join-Path $root 'tests/fixtures/fixture.png') (Join-Path $notes 'fixture.png')
    [IO.File]::SetLastWriteTimeUtc((Join-Path $notes 'Complete.md'),[DateTime]::UtcNow.AddSeconds(5))
    $state=Join-Path $out "$theme-state.json";$metrics=Join-Path $out "$theme-metrics.json"
    [IO.File]::WriteAllText($state,'{"x":120,"y":100,"width":600,"height":760,"pinned":false,"selected":null,"light":'+($theme -eq 'light').ToString().ToLower()+'}')
    $argv=@('--editor','--notes-dir',('"'+$notes+'"'),'--state-file',('"'+$state+'"'),'--metrics-file',('"'+$metrics+'"'))
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
        if($narrow.table_cells -lt 6 -or $narrow.images -ne 2 -or $narrow.styled_runs -lt 5){throw 'Rich mixed document structure missing'}
        foreach($page in 1..4){$null=[NoteProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x22,[IntPtr]::Zero);Capture $hwnd (Join-Path $out "$theme-page-$page.png")}
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
        $null=[NoteProbe]::SendMessageW($hwnd,0x111,[IntPtr]200,[IntPtr]::Zero)
        $editor=[NoteProbe]::GetDlgItem($hwnd,100)
        if($editor -eq [IntPtr]::Zero){throw 'Native editor did not open'}
        $edited="# Edited fixture`r`n`r`nSaved by the isolated native editor test."
        $null=[NoteProbe]::SetText($editor,0x0c,[IntPtr]::Zero,$edited)
        Capture $hwnd (Join-Path $out "$theme-editor.png")
        $null=[NoteProbe]::SendMessageW($hwnd,0x111,[IntPtr]201,[IntPtr]::Zero)
        $path=Join-Path $notes ($after.selected+'.md')
        if(![IO.File]::ReadAllText($path).StartsWith('# Edited fixture')){throw 'Native Save failed'}
        if(!(Test-Path -LiteralPath ($path+'.bak'))){throw 'Save backup missing'}
        $null=[NoteProbe]::SendMessageW($hwnd,0x111,[IntPtr]203,[IntPtr]::Zero)
        if([NoteProbe]::GetDlgItem($hwnd,100) -ne [IntPtr]::Zero){throw 'Reading mode did not return'}
        $null=[NoteProbe]::SendMessageW($hwnd,0x111,[IntPtr]202,[IntPtr]::Zero)
        $created=[NoteProbe]::GetDlgItem($hwnd,100)
        if($created -eq [IntPtr]::Zero){throw 'New note did not start editing'}
        $null=[NoteProbe]::SendMessageW($hwnd,0x111,[IntPtr]203,[IntPtr]::Zero)
        # Native folder notifications update the library without a poll timer.
        [IO.File]::WriteAllText((Join-Path $notes 'Watched.md'),'# Watched note')
        Start-Sleep -Milliseconds 450
        $null=[NoteProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero)
        $watched=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
        if($watched.notes -lt 4){throw 'Native folder watcher did not refresh notes'}
        # Hide/restore only the owned process through the tray callback.
        $null=[NoteProbe]::SendMessageW($hwnd,0x111,[IntPtr]206,[IntPtr]::Zero)
        $null=[NoteProbe]::SendMessageW($hwnd,0x8005,[IntPtr]::Zero,[IntPtr]0x203)
        # Rename uses a native prompt; use PostMessage to avoid blocking this driver.
        $null=[NoteProbe]::PostMessageW($hwnd,0x111,[IntPtr]208,[IntPtr]::Zero)
        Start-Sleep -Milliseconds 300
        $prompt=[NoteProbe]::Prompt($p.Id)
        if($prompt -eq [IntPtr]::Zero){throw 'Rename prompt missing'}
        $name=[NoteProbe]::GetDlgItem($prompt,10)
        $null=[NoteProbe]::SetText($name,0x0c,[IntPtr]::Zero,'Renamed test')
        $null=[NoteProbe]::PostMessageW($prompt,0x111,[IntPtr]1,[IntPtr]::Zero)
        Start-Sleep -Milliseconds 300
        if(!(Test-Path -LiteralPath (Join-Path $notes 'Renamed test.md'))){throw 'Native rename did not preserve the note'}
    }finally{
        if(!$p.HasExited){$null=[NoteProbe]::PostMessageW([NoteProbe]::Find($p.Id),0x10,[IntPtr]::Zero,[IntPtr]::Zero);if(!$p.WaitForExit(10000)){Stop-Process -Id $p.Id -Force;throw 'Owned app did not close'}}
    }
}
Write-Output "Native validation passed. Artifacts: $out"
