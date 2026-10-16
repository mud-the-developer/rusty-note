param([string]$Binary='target/release/rusty-note.exe')
$ErrorActionPreference='Stop'
$root=(Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$exe=if([IO.Path]::IsPathRooted($Binary)){$Binary}else{Join-Path $root $Binary}
$out=Join-Path $root ('target/mermaid-validation/'+[Guid]::NewGuid().ToString('N'))
$notes=Join-Path $out 'notes';New-Item -ItemType Directory -Force $notes | Out-Null
$md=Join-Path $notes 'Mermaid.md'
Copy-Item (Join-Path $root 'tests/fixtures/mermaid.md') $md
Copy-Item (Join-Path $root 'tests/fixtures/fixture.png') (Join-Path $notes 'fixture.png')
$before=(Get-FileHash -LiteralPath $md -Algorithm SHA256).Hash
# Resource-limit data exists only in this isolated target directory.
$limits=Join-Path $notes 'Limits.md'
# Keep the literal fence construction explicit (PowerShell backticks are escapes).
$fence=[string]::new([char]96,3)
$limitSource=$fence+"mermaid`ngraph TD; A["+('x'*8200)+"]`n"+$fence+"`n`n"
$limitSource+=($fence+"mermaid`ngraph TD; A-->B`n"+$fence+"`n`n")*40
[IO.File]::WriteAllText($limits,$limitSource)
$limitsBefore=(Get-FileHash -LiteralPath $limits -Algorithm SHA256).Hash
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Text;
using System.Runtime.InteropServices;
public static class MermaidProbe {
 delegate bool Callback(IntPtr h,IntPtr p);
 [DllImport("user32.dll")] static extern bool EnumWindows(Callback cb,IntPtr p);
 [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h,out uint pid);
 [DllImport("user32.dll",CharSet=CharSet.Unicode)] static extern int GetClassNameW(IntPtr h,StringBuilder s,int n);
 [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h,uint m,IntPtr w,IntPtr l);
 [DllImport("user32.dll",EntryPoint="SendMessageW",CharSet=CharSet.Unicode)] public static extern IntPtr SetText(IntPtr h,uint m,IntPtr w,string s);
 [DllImport("user32.dll")] public static extern IntPtr GetDlgItem(IntPtr h,int id);
 [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr c);
 [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr a,int x,int y,int w,int z,uint f);
 [StructLayout(LayoutKind.Sequential)] public struct RECT{public int L,T,R,B;}
 [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out RECT r);
 [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h,IntPtr dc,uint f);
 public static IntPtr Find(int pid,string name){IntPtr found=IntPtr.Zero;int count=0;EnumWindows((h,p)=>{uint owner;GetWindowThreadProcessId(h,out owner);if(owner!=(uint)pid)return true;var s=new StringBuilder(128);GetClassNameW(h,s,128);if(s.ToString()==name){found=h;count++;}return true;},IntPtr.Zero);return count==1?found:IntPtr.Zero;}
}
'@
$null=[MermaidProbe]::SetThreadDpiAwarenessContext([IntPtr](-4))
function Message([IntPtr]$h,[uint32]$m,[int]$w=0){$null=[MermaidProbe]::SendMessageW($h,$m,[IntPtr]$w,[IntPtr]::Zero)}
function Metrics([string]$name){Message $hwnd 0x8001;$text=[IO.File]::ReadAllText($metrics);[IO.File]::WriteAllText((Join-Path $out ($theme+'-'+$name+'.json')),$text);return $text|ConvertFrom-Json}
function Capture([string]$name){
 $r=New-Object MermaidProbe+RECT
 if(![MermaidProbe]::GetWindowRect($hwnd,[ref]$r)){throw 'Owned Mermaid window missing'}
 $bitmap=New-Object Drawing.Bitmap ($r.R-$r.L),($r.B-$r.T)
 try{$g=[Drawing.Graphics]::FromImage($bitmap);$dc=$g.GetHdc();try{if(![MermaidProbe]::PrintWindow($hwnd,$dc,2)){throw 'Owned Mermaid capture failed'}}finally{$g.ReleaseHdc($dc);$g.Dispose()};$bitmap.Save((Join-Path $out ($theme+'-'+$name+'.png')),[Drawing.Imaging.ImageFormat]::Png)}finally{$bitmap.Dispose()}
}
function FindText([string]$query){
 $null=[MermaidProbe]::PostMessageW($hwnd,0x111,[IntPtr]215,[IntPtr]::Zero);Start-Sleep -Milliseconds 180
 $prompt=[MermaidProbe]::Find($p.Id,'RustyNote.Prompt');if($prompt -eq [IntPtr]::Zero){throw 'Owned find prompt missing'}
 $null=[MermaidProbe]::SetText([MermaidProbe]::GetDlgItem($prompt,10),0x0c,[IntPtr]::Zero,$query)
 $null=[MermaidProbe]::PostMessageW($prompt,0x111,[IntPtr]1,[IntPtr]::Zero);Start-Sleep -Milliseconds 180
}
function CheckGeometry($m){
 if($m.diagram_prepared -ne 4 -or $m.diagram_rendered -lt 3){throw 'Native Mermaid diagrams missing'}
 if($m.table_cells -ne 6 -or $m.images -ne 1){throw 'Surrounding Markdown structure lost'}
 foreach($node in $m.diagram_nodes){if($node.x -lt 0 -or $node.x+$node.width -gt $m.width_dip-56+0.1){throw 'Diagram node outside content width'}}
 foreach($line in $m.diagram_geometry){if($line.ink_width -gt $line.box_width+0.75 -or $line.ink_height -gt $line.height+0.75){throw 'Real DirectWrite label does not fit cached native bounds'}}
}
foreach($theme in @('dark','light')){
 $state=Join-Path $out "$theme-state.json";$metrics=Join-Path $out "$theme-live.json"
 [IO.File]::WriteAllText($state,'{"x":120,"y":100,"width":600,"height":760,"pinned":false}')
 $argv=@('--read-only',('"'+$md+'"'),'--state-file',('"'+$state+'"'),'--metrics-file',('"'+$metrics+'"'),'--smoke-drop-file',('"'+$limits+'"'),'--reduced-motion')
 if($theme -eq 'light'){$argv+='--light'}
 $p=Start-Process -FilePath $exe -ArgumentList $argv -PassThru
 try{
  if(!$p.WaitForInputIdle(15000)){throw 'Mermaid app not ready'}
  $hwnd=[MermaidProbe]::Find($p.Id,'RustyNote.Native');if($hwnd -eq [IntPtr]::Zero){throw 'Owned Mermaid HWND missing'}
  Start-Sleep -Milliseconds 400
  $first=Metrics 'initial';CheckGeometry $first
  if($first.diagram_rendered -ne 4 -or $first.diagram_arrows -ne 10 -or $first.diagram_nodes.Count -ne 13){throw 'Wide graph topology lost'}
  foreach($width in @(600,360)){
   $height=if($width -eq 600){760}else{500}
   $null=[MermaidProbe]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,$width,$height,0x16)
   Message $hwnd 0x100 0x24;Start-Sleep -Milliseconds 150
   $current=Metrics "$width-top";CheckGeometry $current
   if($current.font_measurements -ne $first.font_measurements){throw 'Resize measured diagram fonts'}
   Capture "$width-top"
   # Focus the first node to capture the full branch in a narrow viewport.
   $korean=([string][char]0xC2DC)+([string][char]0xC791)+' Start'
   FindText $korean;$found=Metrics "$width-unicode"
   if($found.find_count -ne 1){throw 'Native Unicode diagram labels not searchable'}
   Capture "$width-unicode";Message $hwnd 0x100 0x1b
   FindText 'Left to right';Capture "$width-lr";Message $hwnd 0x100 0x1b
   FindText 'Merge and chain';Capture "$width-merge";Message $hwnd 0x100 0x1b
   FindText 'sequenceDiagram';$fallback=Metrics "$width-fallback"
   if($fallback.find_count -ne 1){throw 'Unsupported Mermaid did not remain readable code'}
   Capture "$width-fallback";Message $hwnd 0x100 0x1b
   FindText 'https://example.com/never-opened';$unsafe=Metrics "$width-unsafe"
   if($unsafe.find_count -ne 1){throw 'Unsafe Mermaid source missing'}
   Capture "$width-unsafe";Message $hwnd 0x100 0x1b
   FindText 'unclosed';$invalid=Metrics "$width-invalid"
   if($invalid.find_count -ne 1){throw 'Malformed Mermaid source missing'}
   Capture "$width-invalid";Message $hwnd 0x100 0x1b
   Message $hwnd 0x100 0x23;Capture "$width-bottom"
   $after=Metrics "$width-after"
   if($after.font_measurements -ne $first.font_measurements){throw 'Find/scroll measured diagram fonts'}
  }
  foreach($step in 1..4){Message $hwnd 0x111 211}
  FindText $korean;$zoom=Metrics 'zoom-narrow';CheckGeometry $zoom
  if([math]::Abs($zoom.zoom-1.4) -gt 0.001 -or $zoom.font_measurements -le $first.font_measurements){throw 'Zoom did not prepare scaled native labels'}
  Capture 'zoom-narrow';Message $hwnd 0x100 0x1b
  Message $hwnd 0x100 0x22;Capture 'zoom-narrow-lower'
  $null=[MermaidProbe]::SetWindowPos($hwnd,[IntPtr]::Zero,0,0,600,760,0x16)
  $wide=Metrics 'zoom-wide';CheckGeometry $wide
  if($wide.font_measurements -ne $zoom.font_measurements){throw 'Zoomed resize measured fonts'}
  Capture 'zoom-wide'
  # Same-source F5 must not prepare again.
  Message $hwnd 0x100 0x74;Start-Sleep -Milliseconds 200
  $quiet=Metrics 'quiet'
  if($quiet.font_measurements -ne $wide.font_measurements){throw 'Unchanged reload measured fonts'}
  $p.Refresh();$cpu=$p.TotalProcessorTime.TotalMilliseconds
  Start-Sleep -Milliseconds 2500;$p.Refresh();$delta=$p.TotalProcessorTime.TotalMilliseconds-$cpu
  $idle=Metrics 'idle'
  if($idle.paint_count -ne $quiet.paint_count -or $idle.animation_active){throw 'Mermaid idle kept painting'}
  $report=[PSCustomObject]@{theme=$theme;interval_ms=2500;idle_cpu_ms=$delta;idle_paints=$idle.paint_count-$quiet.paint_count;working_set_bytes=$p.WorkingSet64;private_bytes=$p.PrivateMemorySize64;font_measurements=$idle.font_measurements;scale=$idle.scale}
  $report|ConvertTo-Json|Set-Content (Join-Path $out "$theme-profile.json")
  $report|Format-List
  Message $hwnd 0x8007 1;Start-Sleep -Milliseconds 300
  $bounded=Metrics 'limits'
  if($bounded.selected -ne 'Limits' -or $bounded.diagram_prepared -ne 32 -or $bounded.diagram_rendered -ne 32){throw 'Document-wide preparation bounds failed'}
  Capture 'limits'
  FindText 'graph TD';$retained=Metrics 'limits-fallback'
  if($retained.find_count -ne 9){throw 'Oversize/cache-limit source fallback lost'}
  Capture 'limits-fallback'
  if((Get-FileHash -LiteralPath $md -Algorithm SHA256).Hash -ne $before -or (Get-FileHash -LiteralPath $limits -Algorithm SHA256).Hash -ne $limitsBefore){throw 'Viewer changed source bytes'}
  Write-Output "$theme Mermaid native geometry, source integrity, cached resize, zoom and fallback checks passed"
 }finally{
  if(!$p.HasExited){$prompt=[MermaidProbe]::Find($p.Id,'RustyNote.Prompt');if($prompt -ne [IntPtr]::Zero){$null=[MermaidProbe]::PostMessageW($prompt,0x111,[IntPtr]2,[IntPtr]::Zero);Start-Sleep -Milliseconds 100};$null=[MermaidProbe]::PostMessageW([MermaidProbe]::Find($p.Id,'RustyNote.Native'),0x10,[IntPtr]::Zero,[IntPtr]::Zero);if(!$p.WaitForExit(10000)){Stop-Process -Id $p.Id -Force;throw 'Owned Mermaid process did not close'}}
 }
}
Write-Output "Mermaid validation artifacts: $out"
