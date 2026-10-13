param([string]$Binary='target/release/rusty-note.exe')
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$exe=if([IO.Path]::IsPathRooted($Binary)){$Binary}else{Join-Path $root $Binary}
$out=Join-Path $root ('target/sticker-validation/'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $out | Out-Null
$md=Join-Path $out 'Sticker.md'
Copy-Item (Join-Path $root 'tests/fixtures/sticky-viewer.md') $md
$before=[IO.File]::ReadAllBytes($md)
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
 public static IntPtr Find(int pid){IntPtr found=IntPtr.Zero;EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);if(owner!=(uint)pid)return true;var s=new StringBuilder(128);GetClassNameW(h,s,128);if(s.ToString()!="RustyNote.Native")return true;found=h;return false;},IntPtr.Zero);return found;}
}
'@
$null=[StickerProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Capture([IntPtr]$hwnd,[string]$path){$r=New-Object StickerProbe+RECT;if(![StickerProbe]::GetWindowRect($hwnd,[ref]$r)){throw 'Owned sticker missing'};$bitmap=New-Object Drawing.Bitmap ($r.R-$r.L),($r.B-$r.T);$g=[Drawing.Graphics]::FromImage($bitmap);$dc=$g.GetHdc();try{if(![StickerProbe]::PrintWindow($hwnd,$dc,2)){throw 'Owned capture failed'}}finally{$g.ReleaseHdc($dc);$g.Dispose()};try{$bitmap.Save($path,[Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}}
foreach($theme in @('dark','light')){
 $state=Join-Path $out "$theme-state.json";$metrics=Join-Path $out "$theme-metrics.json"
 $argv=@(('"'+$md+'"'),'--state-file',('"'+$state+'"'),'--metrics-file',('"'+$metrics+'"'))
 if($theme -eq 'light'){$argv+='--light'}
 $p=Start-Process -FilePath $exe -ArgumentList $argv -PassThru
 try{
  if(!$p.WaitForInputIdle(15000)){throw 'Sticker not ready'};$hwnd=[StickerProbe]::Find($p.Id);if($hwnd -eq [IntPtr]::Zero){throw 'Sticker HWND not found'};Start-Sleep -Milliseconds 400
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$first=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if(!$first.read_only -or $first.selected -ne 'Sticker' -or $first.lines -lt 5){throw 'Read-only file input failed'}
  Capture $hwnd (Join-Path $out "$theme-normal.png")
  $null=[StickerProbe]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,360,500,0x16)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$narrow=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($narrow.font_measurements -ne $first.font_measurements){throw 'Sticker resize remeasured fonts'}
  Capture $hwnd (Join-Path $out "$theme-narrow.png")
  foreach($id in @(200,201,202,203,208,209,210)){$null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]$id,[IntPtr]::Zero)}
  if([StickerProbe]::GetDlgItem($hwnd,100) -ne [IntPtr]::Zero){throw 'Viewer entered editor'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x09,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x74,[IntPtr]::Zero);Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$idle=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  $p.Refresh();$cpu=$p.TotalProcessorTime.TotalMilliseconds;Start-Sleep -Milliseconds 1500;$p.Refresh();$delta=$p.TotalProcessorTime.TotalMilliseconds-$cpu
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$after=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($after.paint_count -ne $idle.paint_count){throw 'Idle sticker kept repainting'}
  if([Convert]::ToBase64String([IO.File]::ReadAllBytes($md)) -ne [Convert]::ToBase64String($before)){throw 'Viewer changed source Markdown'}
  if((Get-ChildItem $out -Filter '*.md').Count -ne 1 -or (Get-ChildItem $out -Filter '*.bak').Count -ne 0){throw 'Viewer created source files'}
  Write-Output "$theme read-only sticker passed; idle CPU $delta ms; working set $([math]::Round($p.WorkingSet64/1MB,2)) MiB"
 }finally{if(!$p.HasExited){$null=[StickerProbe]::PostMessageW([StickerProbe]::Find($p.Id),0x10,[IntPtr]::Zero,[IntPtr]::Zero);if(!$p.WaitForExit(10000)){Stop-Process -Id $p.Id -Force;throw 'Owned sticker did not close'}}}
}
Write-Output "Sticker validation passed. Artifacts: $out"
