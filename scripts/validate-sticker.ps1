param([string]$Binary='target/release/rusty-note.exe')
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$exe=if([IO.Path]::IsPathRooted($Binary)){$Binary}else{Join-Path $root $Binary}
$out=Join-Path $root ('target/sticker-validation/'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $out | Out-Null
$md=Join-Path $out 'Sticker.md'
Copy-Item (Join-Path $root 'tests/fixtures/sticky-viewer.md') $md
$before=[IO.File]::ReadAllBytes($md)
$dropName=([string][char]0xD55C)+([string][char]0xAE00)+' '+([string][char]0xB4DC)+([string][char]0xB86D)
$dropMd=Join-Path $out ($dropName+'.md')
Copy-Item (Join-Path $root 'tests/fixtures/viewer-polish.md') $dropMd
$dropBefore=[IO.File]::ReadAllBytes($dropMd)
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class StickerProbe {
 delegate bool Callback(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb,IntPtr p);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h,int id);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int z,uint f);
 [StructLayout(LayoutKind.Sequential)] public struct RECT{public int L,T,R,B;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 [DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr h,uint flags);
 [StructLayout(LayoutKind.Sequential)] struct MONITORINFO{public uint Size;public RECT Monitor,Work;public uint Flags;}
 [DllImport("user32.dll")] static extern bool GetMonitorInfoW(IntPtr h,ref MONITORINFO info);
 public static bool IsVisiblePlacement(IntPtr h){RECT r;GetWindowRect(h,out r);var info=new MONITORINFO{Size=(uint)Marshal.SizeOf(typeof(MONITORINFO))};return GetMonitorInfoW(MonitorFromWindow(h,2),ref info)&&r.L>=info.Work.L&&r.T>=info.Work.T&&r.R<=info.Work.R&&r.B<=info.Work.B;}
 public static IntPtr Find(int pid){IntPtr found=IntPtr.Zero;EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);if(owner!=(uint)pid)return true;var s=new StringBuilder(128);GetClassNameW(h,s,128);if(s.ToString()!="RustyNote.Native")return true;found=h;return false;},IntPtr.Zero);return found;}
}
'@
$null=[StickerProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Capture([IntPtr]$hwnd,[string]$path){$r=New-Object StickerProbe+RECT;if(![StickerProbe]::GetWindowRect($hwnd,[ref]$r)){throw 'Owned sticker missing'};$bitmap=New-Object Drawing.Bitmap ($r.R-$r.L),($r.B-$r.T);$g=[Drawing.Graphics]::FromImage($bitmap);$dc=$g.GetHdc();try{if(![StickerProbe]::PrintWindow($hwnd,$dc,2)){throw 'Owned capture failed'}}finally{$g.ReleaseHdc($dc);$g.Dispose()};try{$bitmap.Save($path,[Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}}
foreach($theme in @('dark','light')){
 $state=Join-Path $out "$theme-state.json";$metrics=Join-Path $out "$theme-metrics.json"
 [IO.File]::WriteAllText($state,'{"x":32000,"y":32000,"width":1600,"height":1600,"pinned":true}')
 $argv=@(('"'+$md+'"'),'--smoke-drop-file',('"'+$dropMd+'"'),'--state-file',('"'+$state+'"'),'--metrics-file',('"'+$metrics+'"'))
 if($theme -eq 'light'){$argv+='--light'}
 $p=Start-Process -FilePath $exe -ArgumentList $argv -PassThru
 try{
  if(!$p.WaitForInputIdle(15000)){throw 'Sticker not ready'};$hwnd=[StickerProbe]::Find($p.Id);if($hwnd -eq [IntPtr]::Zero){throw 'Sticker HWND not found'};Start-Sleep -Milliseconds 400
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$first=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if(!$first.read_only -or $first.selected -ne 'Sticker' -or $first.lines -lt 5){throw 'Read-only file input failed'}
  if(![StickerProbe]::IsVisiblePlacement($hwnd)){throw 'Off-screen saved sticker was not recovered'}
  $null=[StickerProbe]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,600,760,0x16)
  Capture $hwnd (Join-Path $out "$theme-normal.png")
  $null=[StickerProbe]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,360,500,0x16)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$narrow=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($narrow.font_measurements -ne $first.font_measurements){throw 'Sticker resize remeasured fonts'}
  Capture $hwnd (Join-Path $out "$theme-narrow.png")
  foreach($id in @(200,201,202,203,208,209,210)){$null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]$id,[IntPtr]::Zero)}
  if([StickerProbe]::GetDlgItem($hwnd,100) -ne [IntPtr]::Zero){throw 'Viewer entered editor'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x09,[IntPtr]::Zero)
  # Allocate HGLOBAL inside the receiver and exercise its actual WM_DROPFILES handler.
  $null=[StickerProbe]::SendMessageW($hwnd,0x8007,[IntPtr]1,[IntPtr]::Zero);Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$dropped=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($dropped.selected -ne $dropName){throw 'Unicode native file drop failed'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x8007,[IntPtr]2,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$rejected=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($rejected.selected -ne $dropped.selected){throw 'Multiple-file drop replaced selection'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x22,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$scrolled=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($scrolled.scroll -le 0){throw 'Mixed reload fixture did not scroll'}
  [IO.File]::WriteAllText((Join-Path $out 'unrelated.tmp'),'unrelated');Start-Sleep -Milliseconds 250
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$unchanged=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($unchanged.font_measurements -ne $scrolled.font_measurements -or $unchanged.scroll -ne $scrolled.scroll){throw 'Unrelated folder change reparsed/reset reading position'}
  [IO.File]::WriteAllBytes($dropMd,[Text.Encoding]::UTF8.GetBytes([Text.Encoding]::UTF8.GetString($dropBefore)+"`n`nExternal fixture refresh. $theme"));Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$reloaded=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($reloaded.scroll -ne $scrolled.scroll -or $reloaded.font_measurements -le $scrolled.font_measurements){throw 'External reload did not retain reading position/update layout'}
  Capture $hwnd (Join-Path $out "$theme-reloaded.png")
  # Reset only the copied fixture before checking app-originated changes.
  [IO.File]::WriteAllBytes($dropMd,$dropBefore);Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x74,[IntPtr]::Zero);Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$idle=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  $p.Refresh();$cpu=$p.TotalProcessorTime.TotalMilliseconds;Start-Sleep -Milliseconds 1500;$p.Refresh();$delta=$p.TotalProcessorTime.TotalMilliseconds-$cpu
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$after=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($after.paint_count -ne $idle.paint_count){throw 'Idle sticker kept repainting'}
  if([Convert]::ToBase64String([IO.File]::ReadAllBytes($md)) -ne [Convert]::ToBase64String($before)){throw 'Viewer changed source Markdown'}
  if([Convert]::ToBase64String([IO.File]::ReadAllBytes($dropMd)) -ne [Convert]::ToBase64String($dropBefore)){throw 'Viewer changed dropped Markdown'}
  if((Get-ChildItem $out -Filter '*.md').Count -ne 2 -or (Get-ChildItem $out -Filter '*.bak').Count -ne 0){throw 'Viewer created source files'}
  Write-Output "$theme read-only sticker passed; idle CPU $delta ms; working set $([math]::Round($p.WorkingSet64/1MB,2)) MiB"
 }finally{if(!$p.HasExited){$null=[StickerProbe]::PostMessageW([StickerProbe]::Find($p.Id),0x10,[IntPtr]::Zero,[IntPtr]::Zero);if(!$p.WaitForExit(10000)){Stop-Process -Id $p.Id -Force;throw 'Owned sticker did not close'}}}
}
Write-Output "Sticker validation passed. Artifacts: $out"
