param([string]$Binary='target/release/rusty-note.exe',[switch]$HelpersOnly)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$exe=if([IO.Path]::IsPathRooted($Binary)){$Binary}else{Join-Path $root $Binary}
$out=Join-Path $root ('target/v1-m2-main/validation-'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $out|Out-Null
$fixture=Join-Path $root 'tests/fixtures/inline-sticker.md'
$fixtureHash=(Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash
$iconHash=(Get-FileHash -LiteralPath (Join-Path $root 'icon.png') -Algorithm SHA256).Hash
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class InlineStickerProbe {
 delegate bool Callback(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb,IntPtr p);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] public static extern IntPtr SendMessageTimeoutW(IntPtr h,uint m,IntPtr w,IntPtr l,uint f,uint timeout,out UIntPtr result);
 [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h,int id);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int z,uint f);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [StructLayout(LayoutKind.Sequential)] public struct RECT{public int L,T,R,B;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
 [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr h,IntPtr r);
 [DllImport("gdi32.dll")] static extern IntPtr CreateRectRgn(int l,int t,int r,int b);
 [DllImport("gdi32.dll")] static extern int GetRgnBox(IntPtr r,out RECT box);
 [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr o);
 public static RECT Region(IntPtr h){var r=CreateRectRgn(0,0,0,0);try{RECT box;if(r==IntPtr.Zero||GetWindowRgn(h,r)==0||GetRgnBox(r,out box)==0)throw new InvalidOperationException("Owned child clip unavailable");return box;}finally{if(r!=IntPtr.Zero)DeleteObject(r);}}
 public static IntPtr Find(int pid,string name){IntPtr found=IntPtr.Zero;int count=0;EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);if(owner!=(uint)pid)return true;var s=new StringBuilder(128);GetClassNameW(h,s,128);if(s.ToString()==name){found=h;count++;}return true;},IntPtr.Zero);return count==1?found:IntPtr.Zero;}
}
'@
$null=[InlineStickerProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Send([IntPtr]$h,[uint32]$m,[long]$w=0,[long]$l=0){
 $result=[UIntPtr]::Zero
 if([InlineStickerProbe]::SendMessageTimeoutW($h,$m,[IntPtr]$w,[IntPtr]$l,2,5000,[ref]$result) -eq [IntPtr]::Zero){throw "Owned message timeout: $m"}
}
function Message([uint32]$m,[long]$w=0,[long]$l=0){Send $script:hwnd $m $w $l}
function ReadSource([string]$path){return [Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($path))}
function Metrics([string]$label){
 Message 0x8001
 $text=[IO.File]::ReadAllText($script:metrics)
 [IO.File]::WriteAllText((Join-Path $case ($label+'.json')),$text)
 return $text|ConvertFrom-Json
}
function Capture([string]$label){
 $r=New-Object InlineStickerProbe+RECT
 if(![InlineStickerProbe]::GetWindowRect($script:hwnd,[ref]$r)){throw 'Owned window missing'}
 $bitmap=New-Object Drawing.Bitmap ($r.R-$r.L),($r.B-$r.T)
 try{
  $g=[Drawing.Graphics]::FromImage($bitmap);$dc=$g.GetHdc()
  try{if(![InlineStickerProbe]::PrintWindow($script:hwnd,$dc,2)){throw 'Owned capture failed'}}finally{$g.ReleaseHdc($dc);$g.Dispose()}
  $bitmap.Save((Join-Path $case ($label+'-parent.png')),[Drawing.Imaging.ImageFormat]::Png)
  $child=[InlineStickerProbe]::GetDlgItem($script:hwnd,101)
  if($child -ne [IntPtr]::Zero -and [InlineStickerProbe]::IsWindowVisible($child)){
   $cr=New-Object InlineStickerProbe+RECT
   if(![InlineStickerProbe]::GetWindowRect($child,[ref]$cr)){throw 'Owned paragraph missing'}
   $picture=New-Object Drawing.Bitmap ($cr.R-$cr.L),($cr.B-$cr.T)
   try{
    $nativeFile=[IO.Path]::ChangeExtension($script:metrics,'inline.bmp')
    if([IO.File]::Exists($nativeFile)){[IO.File]::Delete($nativeFile)}
    Message 0x8021 5
    if(![IO.File]::Exists($nativeFile)){throw 'Receiver-local native client capture failed'}
    $native=[Drawing.Bitmap]::new($nativeFile);$g=[Drawing.Graphics]::FromImage($picture)
    try{$g.DrawImageUnscaled($native,0,0)}finally{$g.Dispose();$native.Dispose()}
    $picture.Save((Join-Path $case ($label+'-native.png')),[Drawing.Imaging.ImageFormat]::Png)
    $clip=[InlineStickerProbe]::Region($child)
    if($clip.R -le $clip.L -or $clip.B -le $clip.T){throw 'Visible native paragraph has an empty clip'}
    $current=[IO.File]::ReadAllText($script:metrics)|ConvertFrom-Json
    $ink=if($current.light){[Drawing.Color]::FromArgb(48,39,34)}else{[Drawing.Color]::FromArgb(245,234,220)}
    $pixels=0
    for($cy=$clip.T;$cy -lt [math]::Min($clip.B,$picture.Height) -and $pixels -lt 12;$cy++){for($cx=$clip.L;$cx -lt [math]::Min($clip.R,$picture.Width-18) -and $pixels -lt 12;$cx++){$c=$picture.GetPixel($cx,$cy);if([math]::Abs($c.R-$ink.R) -lt 60 -and [math]::Abs($c.G-$ink.G) -lt 60 -and [math]::Abs($c.B-$ink.B) -lt 60){$pixels++}}}
    if($pixels -lt 12){throw "Native paragraph text is not visible: $label"}
    # Parent PrintWindow is not a reliable child compositor. Use only actual owned
    # HWND bitmaps, their real screen-coordinate delta and the actual native clip.
    $g=[Drawing.Graphics]::FromImage($bitmap)
    try{$g.SetClip([Drawing.Rectangle]::new($cr.L-$r.L+$clip.L,$cr.T-$r.T+$clip.T,$clip.R-$clip.L,$clip.B-$clip.T));$g.DrawImageUnscaled($picture,$cr.L-$r.L,$cr.T-$r.T)}finally{$g.Dispose()}
   }finally{$picture.Dispose()}
  }
  $bitmap.Save((Join-Path $case ($label+'.png')),[Drawing.Imaging.ImageFormat]::Png)
 }finally{$bitmap.Dispose()}
}
function Launch {
 $argv=@($script:viewerModeArgs)+@(('"'+$original+'"'),'--state-file',('"'+$state+'"'),'--metrics-file',('"'+$script:metrics+'"'),'--smoke-drop-file',('"'+$other+'"'),'--reduced-motion')
 if($theme -eq 'light'){$argv+='--light'}
 $script:p=Start-Process -FilePath $exe -ArgumentList $argv -PassThru
 if(!$script:p.WaitForInputIdle(15000)){throw 'Inline sticker not ready'}
 $script:hwnd=[InlineStickerProbe]::Find($script:p.Id,'RustyNote.Native')
 if($script:hwnd -eq [IntPtr]::Zero){throw 'Owned sticker HWND missing'}
 Start-Sleep -Milliseconds 450
}
function Answer([int]$id){
 $dialog=[IntPtr]::Zero
 for($i=0;$i -lt 40 -and $dialog -eq [IntPtr]::Zero;$i++){$dialog=[InlineStickerProbe]::Find($script:p.Id,'#32770');if($dialog -eq [IntPtr]::Zero){Start-Sleep -Milliseconds 50}}
 if($dialog -eq [IntPtr]::Zero){throw 'Expected owned decision dialog missing'}
 $null=[InlineStickerProbe]::PostMessageW($dialog,0x111,[IntPtr]$id,[IntPtr]::Zero)
 Start-Sleep -Milliseconds 150
}
function CloseOwned([int]$choice=7){
 if(!$script:p -or $script:p.HasExited){return}
 if([InlineStickerProbe]::Find($script:p.Id,'#32770') -ne [IntPtr]::Zero){Answer 7}
 $null=[InlineStickerProbe]::PostMessageW($script:hwnd,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
 Start-Sleep -Milliseconds 150
 if(!$script:p.HasExited -and [InlineStickerProbe]::Find($script:p.Id,'#32770') -ne [IntPtr]::Zero){Answer $choice}
 if(!$script:p.WaitForExit(10000)){Stop-Process -Id $script:p.Id -Force;throw 'Owned sticker did not close'}
 if($script:p.ExitCode -ne 0){throw "Owned sticker exited $($script:p.ExitCode)"}
}
function CrashOwned {
 Stop-Process -Id $script:p.Id -Force
 if(!$script:p.WaitForExit(10000)){throw 'Owned process did not stop'}
}
function Working($m){
 $head=[IO.File]::ReadAllText([IO.Path]::Combine($m.inline.directory,'current.json'))|ConvertFrom-Json
 return ReadSource ([IO.Path]::Combine($m.inline.directory,('checkpoint-'+($head.revision%2)),'working.md'))
}
function SelectRange([int]$start,[int]$end){Message 0x8021 0 ([long]$start -bor ([long]$end -shl 16))}
function TypeText([string]$text){
 $child=[InlineStickerProbe]::GetDlgItem($script:hwnd,101)
 if($child -eq [IntPtr]::Zero){throw 'Actual paragraph control missing'}
 Send $child 0x458
 foreach($unit in $text.ToCharArray()){Send $child 0x102 ([int]$unit)}
 Send $child 0x458
 Start-Sleep -Milliseconds 200
}
function Styles($m){
 if(@($m.inline.styles|Where-Object bold -eq -1).Count -ne 1 -or @($m.inline.styles|Where-Object italic -eq -1).Count -ne 1){throw 'Actual native emphasis not retained'}
 $size=[math]::Round(16*$m.zoom,0,[MidpointRounding]::AwayFromZero)*0.75
 foreach($style in $m.inline.styles){if([math]::Abs($style.size-$size) -gt 0.05){throw 'Native input font does not match document size'}}
}
function ClickSecond {
 Message 0x100 0x24
 $m=Metrics 'before-click'
 $paragraphs=@($m.inline.paragraphs|Where-Object supported)
 if($paragraphs.Count -lt 3){throw 'Expected source-mapped paragraphs missing'}
 $paragraph=$paragraphs[2]
 $x=[int][math]::Round(($paragraph.x+20)*$m.scale)
 $y=[int][math]::Round(($paragraph.y+$paragraph.height/2)*$m.scale)
 if($paragraph.y -lt 106 -or $paragraph.y -gt $m.height_dip-46){throw 'Test paragraph is not visible'}
 $packed=[long]$x -bor ([long]$y -shl 16)
 Message 0x201 0 $packed;Message 0x202 0 $packed
 Start-Sleep -Milliseconds 200
 $active=Metrics 'clicked'
 if(!$active.inline.active -or $active.inline.active_source.start -ne $paragraph.start){throw 'Real click selected the wrong paragraph'}
 Styles $active
 return $active
}
if($HelpersOnly){return}
# The old narrow M2 assertions are historical; use the current short v1 smoke.
& (Join-Path $PSScriptRoot 'validate-v1.ps1') -Binary $Binary
return
$script:viewerModeArgs=@('--inline-edit')
foreach($theme in @('dark','light')){
 $case=Join-Path $out $theme;$notes=Join-Path $case 'notes'
 New-Item -ItemType Directory -Force $notes|Out-Null
 $original=Join-Path $notes 'Inline.md';$other=Join-Path $notes 'Other.md'
 $state=Join-Path $case 'state.json';$script:metrics=Join-Path $case 'live.json'
 $before=ReadSource $fixture
 if($theme -eq 'light'){$before=([string][char]0xfeff)+$before.Replace("`n","`r`n")}
 [IO.File]::WriteAllText($original,$before,[Text.UTF8Encoding]::new($false))
 [IO.File]::WriteAllText($other,'# Other note')
 Copy-Item (Join-Path $root 'tests/fixtures/fixture.png') (Join-Path $notes 'fixture.png')
 [IO.File]::WriteAllText($state,'{"x":120,"y":100,"width":600,"height":760,"pinned":false}')
 $typed=([string][char]0xd55c)+([string][char]0xae00)+' '+[char]::ConvertFromUtf32(0x1f600)
 $offset=$before.LastIndexOf('bold words')
 $expected=$before.Substring(0,$offset)+$typed+$before.Substring($offset+4)
 try{
  Launch
  $first=Metrics 'initial';if(!$first.inline.enabled -or $first.read_only){throw 'Inline mode not enabled'}
  Capture 'normal-before'
  $clicked=ClickSecond
  $bold=@($clicked.inline.styles|Where-Object bold -eq -1)[0]
  SelectRange $bold.start ($bold.start+4)
  TypeText $typed
  $edited=Metrics 'edited';Styles $edited
  if(!$edited.inline.dirty -or $edited.inline.pending){throw 'Supported edit was not applied'}
  if((ReadSource $original) -cne $before){throw 'Typing changed original'}
  Start-Sleep -Milliseconds 650
  $checkpoint=Metrics 'checkpoint'
  if($checkpoint.inline.checkpoint_timer -or (Working $checkpoint) -cne $expected){throw 'Working checkpoint/source splice mismatch'}
  Capture 'normal-edited'
  Message 0x8021 1;Start-Sleep -Milliseconds 150
  Message 0x8021 3;$undo=Metrics 'undo'
  if((Working $undo) -cne $before){throw 'Native undo changed a wrong source span'}
  Message 0x8021 2;Start-Sleep -Milliseconds 150
  Message 0x8021 3;$redo=Metrics 'redo';Styles $redo
  if((Working $redo) -cne $expected){throw 'Native redo lost source mapping'}
  Message 0x100 0x74;Start-Sleep -Milliseconds 100
  $f5=Metrics 'dirty-f5';if((Working $f5) -cne $expected -or !$f5.inline.active){throw 'F5 replaced dirty input'}
  Message 0x100 0x54;Start-Sleep -Milliseconds 100
  $toggled=Metrics 'theme-toggle';Styles $toggled;Capture 'theme-toggled'
  Message 0x100 0x54
  $null=[InlineStickerProbe]::SetWindowPos($script:hwnd,[IntPtr]::Zero,0,0,360,500,0x16)
  Start-Sleep -Milliseconds 150
  $narrow=Metrics 'narrow';Styles $narrow;Capture 'narrow-edited'
  foreach($step in 1..4){Message 0x111 211}
  $zoom=Metrics 'zoom';Styles $zoom;Capture 'narrow-zoom'
  $activeBox=@($zoom.inline.paragraphs|Where-Object start -eq $zoom.inline.active_source.start)[0]
  if(!$zoom.inline.focused -or $activeBox.y -lt 106 -or $activeBox.y -ge $zoom.height_dip-46){throw 'Zoom/resize hid the focused paragraph'}
  $fontCalls=$zoom.font_measurements
  $null=[InlineStickerProbe]::SetWindowPos($script:hwnd,[IntPtr]::Zero,0,0,600,760,0x16)
  Start-Sleep -Milliseconds 150
  $wide=Metrics 'wide-again'
  if($wide.font_measurements -ne $fontCalls){throw 'Resize remeasured fonts'}
  foreach($step in 1..4){Message 0x111 212}
  Message 0x111 201;Start-Sleep -Milliseconds 200
  $saved=Metrics 'saved'
  if($saved.inline.dirty -or (ReadSource $original) -cne $expected -or (ReadSource ($original+'.bak')) -cne $before){throw 'Explicit Save/backup failed'}
  SelectRange 0 0;TypeText 'X'
  Start-Sleep -Milliseconds 650
  $dirty=Metrics 'before-crash';$crashText=Working $dirty
  if(!$dirty.inline.dirty -or $dirty.inline.checkpoint_timer){throw 'Crash draft not checkpointed'}
  CrashOwned;Launch
  $recovered=Metrics 'recovered';Styles $recovered;Capture 'recovered-draft'
  if(!$recovered.inline.dirty -or !$recovered.inline.active -or (Working $recovered) -cne $crashText -or (ReadSource $original) -cne $expected){throw 'Completed draft did not recover'}
  SelectRange 0 0;TypeText '&amp;'
  Start-Sleep -Milliseconds 650
  $pending=Metrics 'pending';Capture 'pending-retained'
  if(!$pending.inline.pending -or $pending.inline.checkpoint_timer){throw 'Unsupported native input not retained'}
  Message 0x111 201
  $refused=Metrics 'pending-save-refused'
  if((ReadSource $original) -cne $expected -or !$refused.error){throw 'Unprojected Save was not visibly blocked'}
  CloseOwned 6;Launch
  $pendingRecovered=Metrics 'pending-recovered';Capture 'pending-recovered'
  if(!$pendingRecovered.inline.pending -or !$pendingRecovered.inline.active){throw 'Pending native input was not recovered'}
  $null=[InlineStickerProbe]::PostMessageW($script:hwnd,0x111,[IntPtr]219,[IntPtr]::Zero);Answer 6
  $discarded=Metrics 'discarded'
  if($discarded.inline.active -or $discarded.inline.dirty -or (ReadSource $original) -cne $expected){throw 'Explicit discard changed original or retained active state'}
  $clicked=ClickSecond
  SelectRange 0 0;TypeText 'Y';Start-Sleep -Milliseconds 650
  $local=Metrics 'before-external';$localText=Working $local
  $external=$expected+"`nExternal writer paragraph.`n"
  [IO.File]::WriteAllText($original,$external,[Text.UTF8Encoding]::new($false))
  [IO.File]::WriteAllText((Join-Path $notes 'unrelated.tmp'),'not a note')
  Start-Sleep -Milliseconds 300
  Message 0x100 0x74;Message 0x111 201
  $conflict=Metrics 'external-conflict';Capture 'external-conflict'
  if((ReadSource $original) -cne $external -or (Working $conflict) -cne $localText -or !$conflict.error){throw 'External conflict overwrote original or working source'}
  $null=[InlineStickerProbe]::PostMessageW($script:hwnd,0x8007,[IntPtr]1,[IntPtr]::Zero)
  $dialog=[IntPtr]::Zero
  for($wait=0;$wait -lt 40 -and $dialog -eq [IntPtr]::Zero;$wait++){$dialog=[InlineStickerProbe]::Find($script:p.Id,'#32770');if($dialog -eq [IntPtr]::Zero){Start-Sleep -Milliseconds 50}}
  if($dialog -eq [IntPtr]::Zero){throw 'Dirty switch dialog missing'}
  # A reentrant owner-close while the modal dialog holds App must be ignored.
  $null=[InlineStickerProbe]::PostMessageW($script:hwnd,0x10,[IntPtr]::Zero,[IntPtr]::Zero)
  Start-Sleep -Milliseconds 100
  if($script:p.HasExited -or [InlineStickerProbe]::Find($script:p.Id,'RustyNote.Native') -ne $script:hwnd){throw 'Reentrant close destroyed modal owner'}
  Answer 2
  $cancelled=Metrics 'cancel-switch'
  if($cancelled.selected -ne 'Inline' -or !$cancelled.inline.focused -or (Working $cancelled) -cne $localText){throw 'Cancelled file switch lost input or native focus'}
  Message 0x8021 3
  Start-Sleep -Milliseconds 400
  $quiet=Metrics 'idle-before';$script:p.Refresh();$cpu=$script:p.TotalProcessorTime.TotalMilliseconds
  Start-Sleep -Milliseconds 3000;$script:p.Refresh()
  $delta=$script:p.TotalProcessorTime.TotalMilliseconds-$cpu
  $idle=Metrics 'idle-after'
  if($idle.paint_count -ne $quiet.paint_count -or $idle.inline.checkpoint_timer -or $idle.inline.child_paint_queued -or $idle.animation_active){throw 'Application kept painting/timing at idle'}
  $report=[PSCustomObject]@{theme=$theme;interval_ms=3000;cpu_ms=$delta;parent_paints=$idle.paint_count-$quiet.paint_count;native_paints=$idle.inline.native_paints-$quiet.inline.native_paints;working_set_bytes=$script:p.WorkingSet64;private_bytes=$script:p.PrivateMemorySize64;focused=$idle.inline.focused;foreground=$idle.inline.foreground;physical_ime_verified=$false}
  $report|ConvertTo-Json|Set-Content (Join-Path $case 'profile.json');$report|Format-List
  CloseOwned 7
  if((ReadSource $original) -cne $external){throw 'Close/discard changed original'}
  Write-Output "$theme main-window click/live edit/undo/recovery/Save/conflict checks passed"
 }catch{Write-Output ($_|Out-String);throw}finally{if($script:p -and !$script:p.HasExited){CloseOwned 7}}
}
# Malformed state must remain untouched, not erase a possible recovery association.
$theme='dark';$case=Join-Path $out 'malformed-state';$notes=Join-Path $case 'notes'
New-Item -ItemType Directory -Force $notes|Out-Null
$original=Join-Path $notes 'Inline.md';$other=Join-Path $notes 'Other.md'
$state=Join-Path $case 'state.json';$script:metrics=Join-Path $case 'live.json'
Copy-Item $fixture $original;Copy-Item (Join-Path $root 'tests/fixtures/fixture.png') (Join-Path $notes 'fixture.png')
[IO.File]::WriteAllText($other,'# Other')
$badState='{"inline_draft":{"original":17}}';[IO.File]::WriteAllText($state,$badState)
try{
 Launch
 $m=Metrics 'blocked-before'
 if(!$m.inline.blocked -or !$m.error){throw 'Malformed state did not visibly block inline editing'}
 $paragraph=@($m.inline.paragraphs|Where-Object supported)[0]
 $x=[int](($paragraph.x+15)*$m.scale);$y=[int](($paragraph.y+$paragraph.height/2)*$m.scale)
 Message 0x202 0 ([long]$x -bor ([long]$y -shl 16))
 $m=Metrics 'blocked-after';if($m.inline.active){throw 'Malformed state allowed an unindexed draft'}
 CloseOwned
 if([IO.File]::ReadAllText($state) -cne $badState){throw 'Malformed state was overwritten'}
 Write-Output 'Malformed-state recovery pointer preservation passed'
}finally{if($script:p -and !$script:p.HasExited){CloseOwned}}
if((Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash -ne $fixtureHash -or (Get-FileHash -LiteralPath (Join-Path $root 'icon.png') -Algorithm SHA256).Hash -ne $iconHash){throw 'Source fixture or icon changed'}
Write-Output "Inline sticker validation artifacts: $out"
