param([string]$Binary='target/debug/examples/inline-edit.exe',[switch]$ViaCargo)
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$exe=Join-Path $root $Binary
$id=[Guid]::NewGuid().ToString('N')
$out=Join-Path $root ('target/v1-m2/validation-'+$id)
New-Item -ItemType Directory -Force $out | Out-Null
$fixture=Join-Path $root 'tests/fixtures/inline-edit.md'
$fixtureBefore=(Get-FileHash $fixture -Algorithm SHA256).Hash
$iconBefore=(Get-FileHash (Join-Path $root 'icon.png') -Algorithm SHA256).Hash
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class InlineProbe {
 delegate bool Callback(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb,IntPtr p);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h,int id);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int z,uint f);
 [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
 [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
 [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
 [DllImport("user32.dll")] static extern bool AttachThreadInput(uint a,uint b,bool attach);
 public static bool ForegroundOwned(IntPtr h){
  uint pid;uint foreground=GetWindowThreadProcessId(GetForegroundWindow(),out pid);uint current=GetCurrentThreadId();
  bool attached=foreground!=current&&AttachThreadInput(current,foreground,true);
  try{return SetForegroundWindow(h)&&GetForegroundWindow()==h;}
  finally{if(attached)AttachThreadInput(current,foreground,false);}
 }
 [StructLayout(LayoutKind.Sequential)] public struct RECT{public int L,T,R,B;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 public static string Class(IntPtr h){var s=new StringBuilder(128);GetClassNameW(h,s,128);return s.ToString();}
 public static IntPtr Find(int pid){IntPtr found=IntPtr.Zero;int count=0;EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);if(owner==(uint)pid&&Class(h)=="RustyNote.InlinePrototype"){found=h;count++;}return true;},IntPtr.Zero);return count==1?found:IntPtr.Zero;}
}
'@
$null=[InlineProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Message([uint32]$m,[int]$w=0){$null=[InlineProbe]::SendMessageW($script:hwnd,$m,[IntPtr]$w,[IntPtr]::Zero)}
function Metrics([string]$name){
 Message 0x8014
 if(Test-Path (Join-Path $script:run 'last-error.txt')){throw ([IO.File]::ReadAllText((Join-Path $script:run 'last-error.txt')))}
 $text=[IO.File]::ReadAllText((Join-Path $script:run 'diagnostics.json'))
 [IO.File]::WriteAllText((Join-Path $out ($theme+'-'+$name+'.json')),$text)
 return $text|ConvertFrom-Json
}
function Capture([string]$name,[switch]$Screen){
 $r=New-Object InlineProbe+RECT
 if(![InlineProbe]::GetWindowRect($script:hwnd,[ref]$r)){throw 'Owned inline window missing'}
 $bitmap=New-Object Drawing.Bitmap ($r.R-$r.L),($r.B-$r.T)
 try{
  $g=[Drawing.Graphics]::FromImage($bitmap)
  try{
   if($Screen){
    if([InlineProbe]::GetForegroundWindow() -ne $script:hwnd){throw 'Refusing screen capture without owned foreground window'}
    $g.CopyFromScreen($r.L,$r.T,0,0,$bitmap.Size)
   }else{
    $dc=$g.GetHdc();try{if(![InlineProbe]::PrintWindow($script:hwnd,$dc,2)){throw 'Owned inline capture failed'}}finally{$g.ReleaseHdc($dc)}
   }
  }finally{$g.Dispose()}
  $bitmap.Save((Join-Path $out ($theme+'-'+$name+'.png')),[Drawing.Imaging.ImageFormat]::Png)
 }finally{$bitmap.Dispose()}
}
function BaseFormat($m){
 $ink=if($m.light){0x222730}else{0xdceaf5}
 if($m.input_format[2] -ne 13.5 -or $m.input_format[3] -ne $ink){throw 'Native base font/color lost (including restored unsupported input)'}
}
function Styles($m){
 BaseFormat $m
 if($m.projection_error){throw ('Unexpected projection failure: '+$m.projection_error)}
 if(!$m.selection_preserved -or !$m.original_unchanged){throw 'Selection or original changed during decoration'}
 if($m.text -ne $m.projected_text -or $m.runs.Count -lt 5){throw 'Native text/projection mismatch'}
 foreach($run in $m.runs){
  $b=if($run.bold){-1}else{0};$i=if($run.italic){-1}else{0}
  if($null -eq $run.native -or $run.native[0] -ne $b -or $run.native[1] -ne $i){throw 'Actual native TOM formatting differs from source projection'}
 }
}
function ObserveIdle([string]$name){
 # No synthetic global keystrokes or caret disabling; detach input queues in finally.
 $null=[InlineProbe]::ForegroundOwned($script:hwnd)
 Start-Sleep -Milliseconds 750
 $quiet=Metrics ($name+'-before')
 $script:p.Refresh();$cpu=$script:p.TotalProcessorTime.TotalMilliseconds
 Start-Sleep -Milliseconds 3000
 $script:p.Refresh();$delta=$script:p.TotalProcessorTime.TotalMilliseconds-$cpu
 $working=$script:p.WorkingSet64;$private=$script:p.PrivateMemorySize64
 $idle=Metrics ($name+'-after')
 if($quiet.checkpoint_timer -or $idle.checkpoint_timer){throw 'Checkpoint timer active at idle'}
 $profile=[PSCustomObject]@{theme=$theme;interval_ms=3000;cpu_ms=$delta;working_set_bytes=$working;private_bytes=$private;parent_paints=$idle.parent_paints-$quiet.parent_paints;paragraph_paints=$idle.paragraph_paints-$quiet.paragraph_paints;foreground_before=$quiet.foreground;foreground_after=$idle.foreground;native_focus=$idle.native_focus;native_caret=$idle.native_caret;caret_blinking=$idle.caret_blinking;physical_ime_verified=$false;note='Unicode injection is not physical IME; nonzero CPU/paints are observations, not budget failures.'}
 $profile|ConvertTo-Json|Set-Content (Join-Path $out ($theme+'-'+$name+'-profile.json'))
 $profile|Format-List|Out-Host
 return $idle
}
function Launch([bool]$Reopen){
 $switch=if($Reopen){'--reopen'}else{'--run'}
 $argv=@($switch,$script:runName);if($theme -eq 'light'){$argv+='--light'}
 $script:launcher=$null
 if($ViaCargo -and !$Reopen){
  # Owned UI run, waited and closed by this harness, not detached background work.
  $script:launcher=Start-Process -FilePath 'cargo.exe' -WorkingDirectory $root -ArgumentList (@('run','--locked','--example','inline-edit','--')+$argv) -RedirectStandardOutput (Join-Path $out "$theme-cargo-run.stdout.log") -RedirectStandardError (Join-Path $out "$theme-cargo-run.stderr.log") -PassThru
  # Cache the process handle before exit so Windows PowerShell retains ExitCode.
  $null=$script:launcher.Handle
  $deadline=[DateTime]::UtcNow.AddSeconds(30)
  do{Start-Sleep -Milliseconds 100;if($script:launcher.HasExited){throw 'cargo run exited before native window startup'}}while(!(Test-Path (Join-Path $script:run 'diagnostics.json')) -and [DateTime]::UtcNow -lt $deadline)
  $m=[IO.File]::ReadAllText((Join-Path $script:run 'diagnostics.json'))|ConvertFrom-Json
  $script:p=Get-Process -Id $m.pid
 }else{
  $script:p=Start-Process -FilePath $exe -WorkingDirectory $root -ArgumentList $argv -PassThru
 }
 if(!$script:p.WaitForInputIdle(15000)){throw 'Inline example not ready'}
 $script:hwnd=[InlineProbe]::Find($script:p.Id)
 if($script:hwnd -eq [IntPtr]::Zero){throw 'Owned inline HWND missing'}
 if([InlineProbe]::Class([InlineProbe]::GetDlgItem($script:hwnd,100)) -ne 'RICHEDIT50W'){throw 'Actual Msftedit native child missing'}
 $null=[InlineProbe]::ForegroundOwned($script:hwnd)
 Start-Sleep -Milliseconds 250
}
function CloseOwned {
 if($script:p -and !$script:p.HasExited){
  $owned=[InlineProbe]::Find($script:p.Id)
  if($owned -ne [IntPtr]::Zero){$null=[InlineProbe]::PostMessageW($owned,0x10,[IntPtr]::Zero,[IntPtr]::Zero)}
  if(!$script:p.WaitForExit(10000)){throw 'Owned inline process refused close; inspect retained error/draft'}
 }
 if($script:launcher){
  if(!$script:launcher.WaitForExit(10000) -or $script:launcher.ExitCode -ne 0){throw "cargo run did not complete successfully (exit=$($script:launcher.ExitCode))"}
  $script:launcher=$null
 }
}
$bold=([string][char]0xAD75)+([string][char]0xC740)+' '+[char]::ConvertFromUtf32(0x1F600)+' '+([string][char]0xAE00)+([string][char]0xC790)
$italic=([string][char]0xBC14)+([string][char]0xB85C)+' italic '+[char]::ConvertFromUtf32(0x1F680)
$plain=([string][char]0xD55C)+[char]::ConvertFromUtf32(0x1F600)+' '
foreach($theme in @('dark','light')){
 $script:runName='run-'+$id+'-'+$theme
 $script:run=Join-Path $root ('target/v1-m2/'+$script:runName)
 try{
  Launch $false
  $original=Join-Path $script:run 'original.md';$before=(Get-FileHash $original -Algorithm SHA256).Hash
  if($before -ne $fixtureBefore){throw 'Embedded sample differs from fixture bytes'}
  $first=Metrics 'initial';Styles $first
  if($first.can_undo -or $first.dirty -or $first.text.Contains('**')){throw 'Not an initially rendered paragraph'}
  Capture 'normal-initial'
  Message 0x8015 7;$selection=Metrics 'selection-before'
  Message 0x8015 10;$decorated=Metrics 'selection-after';Styles $decorated
  if(($selection.selection -join ',') -ne ($decorated.selection -join ',')){throw 'Styling moved selection'}
  Capture 'normal-selection'
  Message 0x8015 1;$edited=Metrics 'bold';Styles $edited
  $boldSource=$first.source.Replace('**bold words**',('**'+$bold+'**'))
  if($edited.source -ne $boldSource -or !$edited.can_undo){throw 'Bold Korean/emoji native replacement did not update M1'}
  Message 0x8015 2;$undo=Metrics 'undo';Styles $undo
  if(!$undo.last_native_result -or !$undo.can_redo -or $undo.source -ne $first.source){throw 'Native undo did not restore exact source'}
  Message 0x8015 10;$restyledUndo=Metrics 'undo-restyled';Styles $restyledUndo
  if(!$restyledUndo.can_redo){throw 'Styling destroyed native redo'}
  Capture 'normal-undo'
  Message 0x8015 3;$redo=Metrics 'redo';Styles $redo
  if(!$redo.last_native_result -or $redo.source -ne $boldSource){throw 'Native redo did not restore edited source'}
  Message 0x8015 4;$both=Metrics 'italic';Styles $both
  $bothSource=$boldSource.Replace('*italic words*',('*'+$italic+'*'))
  if($both.source -ne $bothSource){throw 'Italic edit source mismatch'}
  Message 0x8015 2;$undoItalic=Metrics 'undo-italic';Styles $undoItalic
  if($undoItalic.source -ne $boldSource){throw 'Formatting polluted undo with a non-text operation'}
  Message 0x8015 3;$redoItalic=Metrics 'redo-italic';Styles $redoItalic
  if($redoItalic.source -ne $bothSource){throw 'Italic redo mismatch'}
  Message 0x8015 5;$typed=Metrics 'wmchar-unicode';Styles $typed
  $typedSource=$bothSource.Replace('Plain text,',($plain+'Plain text,'))
  if($typed.source -ne $typedSource){throw 'WM_CHAR Korean/surrogate pair did not update working source'}
  Message 0x8015 2;$undoTyped=Metrics 'undo-typing';Styles $undoTyped
  if($undoTyped.source -ne $bothSource){throw 'Native typing undo group mismatch'}
  Message 0x8015 3;$redoTyped=Metrics 'redo-typing';Styles $redoTyped
  if($redoTyped.source -ne $typedSource){throw 'Native typing redo mismatch'}
  Message 0x8015 13
  foreach($step in 1..3){Message 0x8015 12;$individual=Metrics ('per-character-'+$step);Styles $individual}
  $individualSource=$typedSource.Replace(('**'+$bold),('**'+$bold.Substring(0,1)+'xxx'+$bold.Substring(1)))
  if($individual.source -ne $individualSource){throw 'Per-character live style/source update mismatch'}
  Message 0x8015 14
  Message 0x8015 2;$undoGroup=Metrics 'undo-live-typing-group';Styles $undoGroup
  if($undoGroup.source -ne $typedSource){throw 'Per-character styling split native typing undo group'}
  Message 0x8015 3;$redoGroup=Metrics 'redo-live-typing-group';Styles $redoGroup
  if($redoGroup.source -ne $individualSource){throw 'Native typing group redo was lost'}
  Message 0x8015 2;$back=Metrics 'undo-live-group-again';Styles $back
  if($back.source -ne $typedSource){throw 'Second group undo mismatch'}
  Message 0x8015 9;$caret=Metrics 'caret';Styles $caret
  if($caret.selection[0] -ne $caret.selection[1]){throw 'Caret not collapsed'}
  Message 0x8015 10;$caretAfter=Metrics 'caret-restyled'
  if(($caret.selection -join ',') -ne ($caretAfter.selection -join ',')){throw 'Styling moved caret'}
  Capture 'normal-edited-caret'
  if([InlineProbe]::GetForegroundWindow() -eq $script:hwnd){Capture 'normal-edited-screen' -Screen}
  $null=[InlineProbe]::SetWindowPos($script:hwnd,[IntPtr]::Zero,0,0,420,840,0x16)
  Start-Sleep -Milliseconds 150
  Capture 'narrow-edited-caret'
  $activeIdle=ObserveIdle 'edited-idle'
  Message 0x8015 7;Capture 'narrow-edited-selection'
  Message 0x8015 6;$unsupported=Metrics 'unsupported'
  if(!$unsupported.projection_error -or !$unsupported.text.Contains('&amp;') -or $unsupported.source -ne $typedSource){throw 'Unsupported input was guessed away or lost'}
  Capture 'narrow-unsupported-retained'
  Message 0x8015 2;$undoUnsupported=Metrics 'undo-unsupported';Styles $undoUnsupported
  if($undoUnsupported.source -ne $typedSource -or $undoUnsupported.text -ne $redoTyped.text){throw 'Unsupported native undo did not return to projection'}
  Message 0x8015 3;$redoUnsupported=Metrics 'redo-unsupported'
  if(!$redoUnsupported.projection_error -or $redoUnsupported.text -ne $unsupported.text){throw 'Unsupported redo was lost'}
  Start-Sleep -Milliseconds 750
  $checkpoint=Metrics 'unsupported-checkpoint'
  if($checkpoint.checkpoint_timer -or $checkpoint.reopened_source -ne $typedSource){throw 'Bounded checkpoint did not finish'}
  $pending=[IO.File]::ReadAllText((Join-Path $script:run 'input.json'))|ConvertFrom-Json
  if(($pending.utf16 -join ',') -ne ($unsupported.utf16 -join ',')){throw 'Latest unprojected UTF-16 not durably retained'}
  CloseOwned
  Launch $true
  $reopenedPending=Metrics 'reopened-unsupported';BaseFormat $reopenedPending
  if($reopenedPending.input_format[0] -ne 0 -or $reopenedPending.input_format[1] -ne 0){throw 'Unprojected recovery falsely implies supported emphasis'}
  if(!$reopenedPending.projection_error -or $reopenedPending.text -ne $unsupported.text -or $reopenedPending.source -ne $typedSource){throw 'Unprojected checkpoint reopen mismatch'}
  Capture 'reopened-unsupported'
  Message 0x8015 11;$repaired=Metrics 'repaired';Styles $repaired
  if($repaired.source -ne $typedSource -or $repaired.text -ne $redoTyped.text){throw 'Receiver-local removal did not recover projection'}
  Message 0x8015 2;$undoRepair=Metrics 'undo-repair'
  if(!$undoRepair.projection_error -or $undoRepair.text -ne $unsupported.text){throw 'Repair was not native undoable'}
  Message 0x8015 3;$redoRepair=Metrics 'redo-repair';Styles $redoRepair
  Start-Sleep -Milliseconds 750
  $completed=Metrics 'completed-checkpoint'
  if($completed.checkpoint_timer -or $completed.reopened_source -ne $typedSource){throw 'Working checkpoint mismatch'}
  CloseOwned
  Launch $true
  $reopened=Metrics 'reopened-supported';Styles $reopened
  if($reopened.source -ne $typedSource -or $reopened.text -ne $redoTyped.text){throw 'Supported checkpoint reopen mismatch'}
  Message 0x8015 9
  $idle=ObserveIdle 'reopened-idle'
  Capture 'reopened-supported'
  if((Get-FileHash $original -Algorithm SHA256).Hash -ne $before){throw 'Copied original changed'}
  [PSCustomObject]@{fixture_sha256=$fixtureBefore;original_before=$before;original_after=(Get-FileHash $original -Algorithm SHA256).Hash;run=$script:run;session=$idle.session}|ConvertTo-Json|Set-Content (Join-Path $out "$theme-integrity.json")
  Write-Output "$theme native projection, caret/selection, undo/redo, unsupported retention, checkpoint/reopen and source integrity passed"
 }catch{Write-Output ($_ | Out-String);throw}finally{CloseOwned}
}
if((Get-FileHash $fixture -Algorithm SHA256).Hash -ne $fixtureBefore -or (Get-FileHash (Join-Path $root 'icon.png') -Algorithm SHA256).Hash -ne $iconBefore){throw 'Fixture or icon changed'}
Write-Output "Inline prototype validation artifacts: $out"
