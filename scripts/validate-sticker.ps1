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
Copy-Item (Join-Path $root 'tests/fixtures/reader-comfort.md') $dropMd
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
 [StructLayout(LayoutKind.Sequential)] public struct POINT{public int X,Y;}
 [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
 [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
 [DllImport("user32.dll")] static extern IntPtr MonitorFromWindow(IntPtr h,uint flags);
 [StructLayout(LayoutKind.Sequential)] struct MONITORINFO{public uint Size;public RECT Monitor,Work;public uint Flags;}
 [DllImport("user32.dll")] static extern bool GetMonitorInfoW(IntPtr h,ref MONITORINFO info);
 public static bool IsVisiblePlacement(IntPtr h){RECT r;GetWindowRect(h,out r);var info=new MONITORINFO{Size=(uint)Marshal.SizeOf(typeof(MONITORINFO))};return GetMonitorInfoW(MonitorFromWindow(h,2),ref info)&&r.L>=info.Work.L&&r.T>=info.Work.T&&r.R<=info.Work.R&&r.B<=info.Work.B;}
 [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr h,Callback cb,IntPtr p);
 public static IntPtr DialogButton(IntPtr dialog){IntPtr found=IntPtr.Zero;int matches=0;EnumChildWindows(dialog,(h,p)=>{var s=new StringBuilder(64);GetClassNameW(h,s,64);if(s.ToString().Equals("Button",StringComparison.OrdinalIgnoreCase)){found=h;matches++;}return true;},IntPtr.Zero);return matches==1?found:IntPtr.Zero;}
 public static IntPtr Dialog(int pid){IntPtr found=IntPtr.Zero;int matches=0;EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);if(owner!=(uint)pid)return true;var s=new StringBuilder(128);GetClassNameW(h,s,128);if(s.ToString()=="#32770"){found=h;matches++;}return true;},IntPtr.Zero);return matches==1?found:IntPtr.Zero;}
 [DllImport("user32.dll",EntryPoint="SendMessageW",CharSet=CharSet.Unicode)] public static extern IntPtr SetText(IntPtr h,uint m,IntPtr w,string text);
 public static IntPtr Prompt(int pid){IntPtr found=IntPtr.Zero;int matches=0;EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);if(owner!=(uint)pid)return true;var s=new StringBuilder(128);GetClassNameW(h,s,128);if(s.ToString()=="RustyNote.Prompt"){found=h;matches++;}return true;},IntPtr.Zero);return matches==1?found:IntPtr.Zero;}
 public static IntPtr Find(int pid){IntPtr found=IntPtr.Zero;EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);if(owner!=(uint)pid)return true;var s=new StringBuilder(128);GetClassNameW(h,s,128);if(s.ToString()!="RustyNote.Native")return true;found=h;return false;},IntPtr.Zero);return found;}
}
'@
$null=[StickerProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
function PointParam([int]$x,[int]$y){return [IntPtr](($y -shl 16) -bor ($x -band 0xffff))}
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
  # Hover only inside the verified owned window; restore the pointer immediately.
  $oldPoint=New-Object StickerProbe+POINT;$null=[StickerProbe]::GetCursorPos([ref]$oldPoint)
  $owned=New-Object StickerProbe+RECT;if(![StickerProbe]::GetWindowRect($hwnd,[ref]$owned)){throw 'Owned hover target missing'}
  try{$null=[StickerProbe]::SetCursorPos($owned.L+28,$owned.T+20);Start-Sleep -Milliseconds 100
   $null=[StickerProbe]::SendMessageW($hwnd,0x200,[IntPtr]::Zero,(PointParam 28 20))
   $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$hovered=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
   if($hovered.hint -notlike 'Open Markdown*'){throw 'Open control help missing'}
   Capture $hwnd (Join-Path $out "$theme-open-help.png")
  }finally{$null=[StickerProbe]::SetCursorPos($oldPoint.X,$oldPoint.Y)}
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x50,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x54,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$keys=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($keys.pinned -eq $narrow.pinned -or $keys.light -eq $narrow.light){throw 'Pin/theme keyboard shortcuts failed'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x50,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x54,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x200,[IntPtr]::Zero,(PointParam 150 80))
  foreach($id in @(200,201,202,203,208,209,210)){$null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]$id,[IntPtr]::Zero)}
  if([StickerProbe]::GetDlgItem($hwnd,100) -ne [IntPtr]::Zero){throw 'Viewer entered editor'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x09,[IntPtr]::Zero)
  # Allocate HGLOBAL inside the receiver and exercise its actual WM_DROPFILES handler.
  $null=[StickerProbe]::SendMessageW($hwnd,0x8007,[IntPtr]1,[IntPtr]::Zero);Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$dropped=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($dropped.selected -ne $dropName){throw 'Unicode native file drop failed'}
  foreach($step in 1..4){$null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]211,[IntPtr]::Zero)}
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$zoomed=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if([math]::Abs($zoomed.zoom-1.4) -gt 0.001 -or $zoomed.font_measurements -le $dropped.font_measurements){throw 'Larger text did not prepare actual scaled fonts'}
  Capture $hwnd (Join-Path $out "$theme-zoom-narrow.png")
  $null=[StickerProbe]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,600,760,0x16)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$zoomWide=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($zoomWide.font_measurements -ne $zoomed.font_measurements){throw 'Resize at enlarged text measured fonts again'}
  Capture $hwnd (Join-Path $out "$theme-zoom-wide.png")
  foreach($page in 1..3){$null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x22,[IntPtr]::Zero);Capture $hwnd (Join-Path $out "$theme-zoom-page-$page.png")}
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x24,[IntPtr]::Zero)
  $null=[StickerProbe]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,360,500,0x16)
  foreach($step in 1..20){$null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]211,[IntPtr]::Zero)}
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$zoomMax=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if([math]::Abs($zoomMax.zoom-1.6) -gt 0.001){throw 'Reading zoom upper bound failed'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]213,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$reset=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($reset.zoom -ne 1){throw 'Reading zoom reset failed'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x8007,[IntPtr]2,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$rejected=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($rejected.selected -ne $dropped.selected){throw 'Multiple-file drop replaced selection'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x22,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$scrolled=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($scrolled.scroll -le 0){throw 'Mixed reload fixture did not scroll'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x25,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x27,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$arrows=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($arrows.scroll -ne $scrolled.scroll -or $arrows.font_measurements -ne $scrolled.font_measurements){throw 'Left/right arrow reset read-only document'}
  $thumbX=[int](($scrolled.width_dip-16)*$scrolled.scale);$thumbY=[int](($scrolled.scroll_thumb_top+5)*$scrolled.scale)
  $null=[StickerProbe]::SendMessageW($hwnd,0x201,[IntPtr]1,(PointParam $thumbX $thumbY))
  $null=[StickerProbe]::SendMessageW($hwnd,0x200,[IntPtr]1,(PointParam $thumbX ([int](($scrolled.height_dip-55)*$scrolled.scale))))
  $null=[StickerProbe]::SendMessageW($hwnd,0x202,[IntPtr]::Zero,(PointParam $thumbX ([int](($scrolled.height_dip-55)*$scrolled.scale))))
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$dragged=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($dragged.scroll -le $scrolled.scroll -or $dragged.font_measurements -ne $scrolled.font_measurements){throw 'Scrollbar drag did not advance cached reading view'}
  Capture $hwnd (Join-Path $out "$theme-scroll-drag.png")
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x26,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$up=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($up.scroll -ge $dragged.scroll){throw 'Up arrow did not scroll'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x28,[IntPtr]::Zero)
  # Use an interior reading position for the later external-refresh checks.
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x24,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x22,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$scrolled=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  [IO.File]::WriteAllText((Join-Path $out 'unrelated.tmp'),'unrelated');Start-Sleep -Milliseconds 250
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$unchanged=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($unchanged.font_measurements -ne $scrolled.font_measurements -or $unchanged.scroll -ne $scrolled.scroll){throw 'Unrelated folder change reparsed/reset reading position'}
  [IO.File]::WriteAllBytes($dropMd,[Text.Encoding]::UTF8.GetBytes([Text.Encoding]::UTF8.GetString($dropBefore)+"`n`nExternal fixture refresh. $theme"));Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$reloaded=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($reloaded.scroll -ne $scrolled.scroll -or $reloaded.font_measurements -le $scrolled.font_measurements){throw 'External reload did not retain reading position/update layout'}
  Capture $hwnd (Join-Path $out "$theme-reloaded.png")
  # Hide only the copied source to exercise recoverable read errors.
  $gone=$dropMd+'.gone';Move-Item -LiteralPath $dropMd -Destination $gone;Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$readError=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if(!$readError.error -or $readError.selected -ne $dropName){throw 'Missing source discarded the reading view or hid its error'}
  Capture $hwnd (Join-Path $out "$theme-read-error.png")
  $null=[StickerProbe]::PostMessageW($hwnd,0x111,[IntPtr]214,[IntPtr]::Zero);Start-Sleep -Milliseconds 250
  $details=[StickerProbe]::Dialog($p.Id);if($details -eq [IntPtr]::Zero){throw 'Full native error message missing'}
  Capture $details (Join-Path $out "$theme-error-details.png")
  $ok=[StickerProbe]::DialogButton($details);if($ok -eq [IntPtr]::Zero){throw 'Owned error dialog OK button missing'}
  $null=[StickerProbe]::PostMessageW($ok,0xF5,[IntPtr]::Zero,[IntPtr]::Zero);Start-Sleep -Milliseconds 250
  if([StickerProbe]::Dialog($p.Id) -ne [IntPtr]::Zero){throw 'Owned error dialog did not dismiss'}
  Move-Item -LiteralPath $gone -Destination $dropMd
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x74,[IntPtr]::Zero);Start-Sleep -Milliseconds 250
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$recovered=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($recovered.error -or $recovered.scroll -ne $reloaded.scroll){throw 'F5 did not recover the read view and its position'}
  # Reset only the copied fixture before checking app-originated changes.
  [IO.File]::WriteAllBytes($dropMd,$dropBefore);Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x74,[IntPtr]::Zero);Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]211,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]211,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$idle=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  $p.Refresh();$cpu=$p.TotalProcessorTime.TotalMilliseconds;Start-Sleep -Milliseconds 1500;$p.Refresh();$delta=$p.TotalProcessorTime.TotalMilliseconds-$cpu
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$after=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($after.paint_count -ne $idle.paint_count){throw 'Idle sticker kept repainting'}
  if([Convert]::ToBase64String([IO.File]::ReadAllBytes($md)) -ne [Convert]::ToBase64String($before)){throw 'Viewer changed source Markdown'}
  if([Convert]::ToBase64String([IO.File]::ReadAllBytes($dropMd)) -ne [Convert]::ToBase64String($dropBefore)){throw 'Viewer changed dropped Markdown'}
  if((Get-ChildItem $out -Filter '*.md').Count -ne 2 -or (Get-ChildItem $out -Filter '*.bak').Count -ne 0){throw 'Viewer created source files'}
  $restoreScroll=$after.scroll
  $null=[StickerProbe]::PostMessageW($hwnd,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
  if(!$p.WaitForExit(10000)){throw 'Sticker close before restore failed'}
  $p=Start-Process -FilePath $exe -ArgumentList @('--state-file',('"'+$state+'"'),'--metrics-file',('"'+$metrics+'"')) -PassThru
  if(!$p.WaitForInputIdle(15000)){throw 'Restored sticker not ready'};$hwnd=[StickerProbe]::Find($p.Id);Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$restored=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($restored.selected -ne $dropName -or [math]::Abs($restored.scroll-$restoreScroll) -gt 0.01 -or [math]::Abs($restored.zoom-$after.zoom) -gt 0.001){throw 'Reading position/text size did not survive close/reopen'}
  Capture $hwnd (Join-Path $out "$theme-restored.png")
  $null=[StickerProbe]::PostMessageW($hwnd,0x10,[IntPtr]::Zero,[IntPtr]::Zero);if(!$p.WaitForExit(10000)){throw 'Restore validation close failed'}
  $p=Start-Process -FilePath $exe -ArgumentList @(('"'+$md+'"'),'--state-file',('"'+$state+'"'),'--metrics-file',('"'+$metrics+'"')) -PassThru
  if(!$p.WaitForInputIdle(15000)){throw 'Different-file sticker not ready'};$hwnd=[StickerProbe]::Find($p.Id);Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$other=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($other.selected -ne 'Sticker' -or $other.scroll -ne 0){throw 'Different file inherited the saved reading offset'}
  Write-Output "$theme read-only usability checks passed; idle CPU $delta ms"
 }catch{Write-Output ("Native validation failure: "+$_.Exception.Message);throw}
 finally{if(!$p.HasExited){$modal=[StickerProbe]::Dialog($p.Id);if($modal -ne [IntPtr]::Zero){$null=[StickerProbe]::PostMessageW($modal,0x10,[IntPtr]::Zero,[IntPtr]::Zero);Start-Sleep -Milliseconds 150};$null=[StickerProbe]::PostMessageW([StickerProbe]::Find($p.Id),0x10,[IntPtr]::Zero,[IntPtr]::Zero);if(!$p.WaitForExit(10000)){Stop-Process -Id $p.Id -Force;throw 'Owned test-process did not close cleanly'}}}
}
Write-Output "Sticker validation passed. Artifacts: $out"
function FindQuery($process,[IntPtr]$hwnd,[string]$query){$null=[StickerProbe]::PostMessageW($hwnd,0x111,[IntPtr]215,[IntPtr]::Zero);Start-Sleep -Milliseconds 180;$prompt=[StickerProbe]::Prompt($process.Id);if($prompt -eq [IntPtr]::Zero){throw 'Owned find prompt missing'};$edit=[StickerProbe]::GetDlgItem($prompt,10);$null=[StickerProbe]::SetText($edit,0x0c,[IntPtr]::Zero,$query);$null=[StickerProbe]::PostMessageW($prompt,0x111,[IntPtr]1,[IntPtr]::Zero);Start-Sleep -Milliseconds 180}
$findOut=Join-Path $out 'find';New-Item -ItemType Directory -Force $findOut | Out-Null
$findMd=Join-Path $findOut 'Find.md';Copy-Item (Join-Path $root 'tests/fixtures/reader-find.md') $findMd
$findBefore=[IO.File]::ReadAllBytes($findMd)
foreach($theme in @('dark','light')){
 $state=Join-Path $findOut "$theme-state.json";$metrics=Join-Path $findOut "$theme-metrics.json"
 $argv=@(('"'+$findMd+'"'),'--state-file',('"'+$state+'"'),'--metrics-file',('"'+$metrics+'"'))
 if($theme -eq 'light'){$argv+='--light'}
 $p=Start-Process -FilePath $exe -ArgumentList $argv -PassThru
 try{
  if(!$p.WaitForInputIdle(15000)){throw 'Find app not ready'};$hwnd=[StickerProbe]::Find($p.Id);Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$baseline=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  FindQuery $p $hwnd 'needle phrase'
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$found=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($found.find_count -ne 3 -or $found.find_index -ne 0 -or $found.font_measurements -ne $baseline.font_measurements){throw 'Find did not return semantic results without font measurement'}
  Capture $hwnd (Join-Path $findOut "$theme-first.png")
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x72,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$next=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($next.find_index -ne 1 -or $next.scroll -le $found.scroll){throw 'F3 did not advance/jump to table result'}
  Capture $hwnd (Join-Path $findOut "$theme-table.png")
  $null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]217,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$prev=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($prev.find_index -ne 0){throw 'Previous result did not return'}
  $null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]217,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$wrap=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($wrap.find_index -ne 2){throw 'Previous result did not wrap'}
  Capture $hwnd (Join-Path $findOut "$theme-code.png")
  $null=[StickerProbe]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,600,760,0x16)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$wide=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($wide.find_count -ne 3 -or $wide.find_index -ne 2 -or $wide.font_measurements -ne $baseline.font_measurements){throw 'Resize lost search results or measured fonts'}
  Capture $hwnd (Join-Path $findOut "$theme-wide.png")
  $null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]211,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$zoom=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($zoom.find_count -ne 3){throw 'Text-size change lost query results'}
  [IO.File]::WriteAllBytes($findMd,[Text.Encoding]::UTF8.GetBytes([Text.Encoding]::UTF8.GetString($findBefore)+"`n`nA fourth needle phrase after refresh."));Start-Sleep -Milliseconds 350
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$refresh=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($refresh.find_count -ne 4){throw 'External refresh did not update query results'}
  [IO.File]::WriteAllBytes($findMd,$findBefore);Start-Sleep -Milliseconds 350
  FindQuery $p $hwnd 'not-found-123'
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$none=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($none.find_count -ne 0 -or !$none.find_active -or $none.error){throw 'No-results query was not handled normally'}
  Capture $hwnd (Join-Path $findOut "$theme-no-results.png")
  $korean=([string][char]0xD55C)+([string][char]0xAD6D)+([string][char]0xC5B4)+' '+([string][char]0xCC3E)+([string][char]0xAE30)
  FindQuery $p $hwnd $korean
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$unicode=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($unicode.find_count -ne 2){throw 'Unicode literal query missing results'}
  Capture $hwnd (Join-Path $findOut "$theme-unicode.png")
  $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x1b,[IntPtr]::Zero)
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$clear=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($clear.find_active -or $clear.find_count -ne 0){throw 'Escape did not clear find'}
  if([Convert]::ToBase64String([IO.File]::ReadAllBytes($findMd)) -ne [Convert]::ToBase64String($findBefore)){throw 'Find changed Markdown source'}
  Write-Output "$theme native find checks passed"
 }catch{Write-Output ("Find validation failure: "+$_.Exception.Message);throw}
 finally{if(!$p.HasExited){$prompt=[StickerProbe]::Prompt($p.Id);if($prompt -ne [IntPtr]::Zero){$null=[StickerProbe]::PostMessageW($prompt,0x111,[IntPtr]2,[IntPtr]::Zero);Start-Sleep -Milliseconds 100};$null=[StickerProbe]::PostMessageW([StickerProbe]::Find($p.Id),0x10,[IntPtr]::Zero,[IntPtr]::Zero);if(!$p.WaitForExit(10000)){Stop-Process -Id $p.Id -Force;throw 'Owned find process did not close'}}}
}
Write-Output "Find validation passed. Artifacts: $findOut"
function CheckTableGeometry($metrics){
 if($metrics.table_cells -ne 27 -or @($metrics.table_geometry).Count -lt 25){throw 'Alignment fixture lost table cells/lines'}
 $checked=0
 foreach($line in $metrics.table_geometry){
  if($line.ink_width -le 0 -or $line.ink_width -gt $line.box_width+0.1){continue}
  $expected=if($line.alignment -eq 'Center'){($line.box_width-$line.ink_width)/2}elseif($line.alignment -eq 'Right'){$line.box_width-$line.ink_width}else{0}
  if([math]::Abs($line.ink_left-$expected) -gt 0.75){throw ('Native glyph alignment wrong: '+($line|ConvertTo-Json -Compress))}
  $checked++
 }
 if($checked -lt 10){throw 'Too few real DirectWrite lines checked'}
}
function CheckTableLinks([IntPtr]$hwnd,$metrics){
 foreach($alignment in @('Center','Right')){
  $line=@($metrics.table_geometry|Where-Object {$_.linked -and $_.alignment -eq $alignment -and $_.y -gt 106 -and $_.y+$_.height -lt $metrics.height_dip-46 -and $_.ink_width -le $_.box_width+0.1})|Select-Object -First 1
  if(!$line){throw "No visible $alignment link for native hit testing"}
  $x=[int]($line.x+$line.ink_left+$line.ink_width/2);$y=[int]($line.y+$line.height/2)
  $hit=[StickerProbe]::SendMessageW($hwnd,0x8008,[IntPtr]::Zero,(PointParam $x $y))
  if($hit.ToInt32() -ne 1){throw "$alignment table link did not hit its rendered glyphs"}
  if($line.ink_left -gt 5){$miss=[StickerProbe]::SendMessageW($hwnd,0x8008,[IntPtr]::Zero,(PointParam ([int]($line.x+1)) $y));if($miss.ToInt32() -ne 0){throw 'Table padding incorrectly activated a link'}}
 }
}
$tableOut=Join-Path $out 'alignment';New-Item -ItemType Directory -Force $tableOut|Out-Null
$tableMd=Join-Path $tableOut 'Alignment.md';Copy-Item (Join-Path $root 'tests/fixtures/table-alignment.md') $tableMd
$tableBefore=[IO.File]::ReadAllBytes($tableMd)
foreach($theme in @('dark','light')){
 $state=Join-Path $tableOut "$theme-state.json";$metrics=Join-Path $tableOut "$theme-metrics.json"
 $argv=@(('"'+$tableMd+'"'),'--state-file',('"'+$state+'"'),'--metrics-file',('"'+$metrics+'"'))
 if($theme -eq 'light'){$argv+='--light'}
 $p=Start-Process -FilePath $exe -ArgumentList $argv -PassThru
 try{
  if(!$p.WaitForInputIdle(15000)){throw 'Alignment app not ready'};$hwnd=[StickerProbe]::Find($p.Id);if($hwnd -eq [IntPtr]::Zero){throw 'Owned alignment HWND missing'};Start-Sleep -Milliseconds 350
  foreach($width in @(600,360)){
   $height=if($width -eq 600){760}else{500}
   $null=[StickerProbe]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,$width,$height,0x16)
   $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x24,[IntPtr]::Zero)
   $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$first=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
   CheckTableGeometry $first
   Capture $hwnd (Join-Path $tableOut "$theme-$width-top.png")
   FindQuery $p $hwnd 'center link'
   $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$links=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
   CheckTableGeometry $links;CheckTableLinks $hwnd $links
   if($links.find_count -ne 1 -or $links.font_measurements -ne $first.font_measurements){throw 'Find/links remeasured aligned-table fonts or lost a match'}
   Capture $hwnd (Join-Path $tableOut "$theme-$width-links.png")
   $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x1b,[IntPtr]::Zero)
   FindQuery $p $hwnd '123.45'
   $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$number=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
   if($number.find_count -ne 2){throw 'Right-aligned number/final surrounding text not searchable'}
   Capture $hwnd (Join-Path $tableOut "$theme-$width-number.png")
   $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x1b,[IntPtr]::Zero)
   $null=[StickerProbe]::SendMessageW($hwnd,0x100,[IntPtr]0x23,[IntPtr]::Zero)
   Capture $hwnd (Join-Path $tableOut "$theme-$width-bottom.png")
   $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$after=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
   if($after.font_measurements -ne $first.font_measurements){throw 'Aligned table interaction remeasured fonts'}
   if($width -eq 600){$wideFontCalls=$first.font_measurements}elseif($first.font_measurements -ne $wideFontCalls){throw 'Aligned table resize remeasured fonts'}
  }
  $null=[StickerProbe]::SendMessageW($hwnd,0x111,[IntPtr]211,[IntPtr]::Zero)
  FindQuery $p $hwnd 'center link'
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$zoom=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  CheckTableGeometry $zoom;CheckTableLinks $hwnd $zoom
  Capture $hwnd (Join-Path $tableOut "$theme-zoom-links.png")
  Start-Sleep -Milliseconds 250
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$quiet=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  $p.Refresh();$cpu=$p.TotalProcessorTime.TotalMilliseconds;Start-Sleep -Milliseconds 1500;$p.Refresh();$delta=$p.TotalProcessorTime.TotalMilliseconds-$cpu
  $null=[StickerProbe]::SendMessageW($hwnd,0x8001,[IntPtr]::Zero,[IntPtr]::Zero);$idle=[IO.File]::ReadAllText($metrics)|ConvertFrom-Json
  if($idle.paint_count -ne $quiet.paint_count -or $delta -gt 100){throw 'Aligned table idle activity exceeded local check'}
  $p.Refresh();Write-Output "$theme native alignment/link checks passed; idle CPU $delta ms, working set $($p.WorkingSet64), private bytes $($p.PrivateMemorySize64)"
  if([Convert]::ToBase64String([IO.File]::ReadAllBytes($tableMd)) -ne [Convert]::ToBase64String($tableBefore)){throw 'Table reading changed Markdown source'}
 }catch{Write-Output ('Alignment validation failure: '+$_.Exception.Message);throw}
 finally{if(!$p.HasExited){$prompt=[StickerProbe]::Prompt($p.Id);if($prompt -ne [IntPtr]::Zero){$null=[StickerProbe]::PostMessageW($prompt,0x111,[IntPtr]2,[IntPtr]::Zero);Start-Sleep -Milliseconds 100};$null=[StickerProbe]::PostMessageW([StickerProbe]::Find($p.Id),0x10,[IntPtr]::Zero,[IntPtr]::Zero);if(!$p.WaitForExit(10000)){Stop-Process -Id $p.Id -Force;throw 'Owned alignment process did not close'}}}
}
Write-Output "Alignment validation passed. Artifacts: $tableOut"
