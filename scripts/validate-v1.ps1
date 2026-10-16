param([string]$Binary='target/release/rusty-note.exe')
# Essential v1 smoke only: small copied notes, two themes/widths, no stress matrix.
. (Join-Path $PSScriptRoot 'validate-inline-sticker.ps1') -Binary $Binary -HelpersOnly
$out=Join-Path $root ('target/v1-smoke/'+[Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $out|Out-Null
$script:viewerModeArgs=@()
function ClickPoint($m,[double]$x,[double]$y){$px=[int][math]::Round($x*$m.scale);$py=[int][math]::Round($y*$m.scale);$packed=[long]$px -bor ([long]$py -shl 16);Message 0x201 0 $packed;Message 0x202 0 $packed;Start-Sleep -Milliseconds 120}
function ClickKind([string]$kind){$m=Metrics 'pick';$item=@($m.inline.paragraphs|Where-Object {$_.supported -and $_.kind -like $kind})[0];if(!$item){throw "No editable $kind"};ClickPoint $m ($item.x+35) ($item.y+$item.height/2);$active=Metrics 'active';if(!$active.inline.active){throw "Could not edit $kind : $($active.error)"};return $active}
foreach($theme in @('dark','light')){
 $case=Join-Path $out $theme;$notes=Join-Path $case 'notes';New-Item -ItemType Directory -Force $notes|Out-Null
 $original=Join-Path $notes 'V1.md';$other=Join-Path $notes 'Other.md';$state=Join-Path $case 'state.json';$script:metrics=Join-Path $case 'live.json'
 $before="# Heading`n`nWrite **bold** and *italic* here.`n`n- [ ] Ship v1`n- [x] Keep native`n`n![Local](fixture.png)`n`nFinal paragraph.`n"
 if($theme -eq 'light'){$before=([string][char]0xfeff)+$before.Replace("`n","`r`n")}
 [IO.File]::WriteAllText($original,$before,[Text.UTF8Encoding]::new($false));[IO.File]::WriteAllText($other,'# Other')
 Copy-Item (Join-Path $root 'tests/fixtures/fixture.png') (Join-Path $notes 'fixture.png')
 [IO.File]::WriteAllText($state,'{"x":120,"y":100,"width":600,"height":760,"pinned":false}')
 try{
  Launch
  $initial=Metrics 'initial';if(!$initial.inline.enabled -or $initial.read_only){throw 'Default must be editable'}
  $m=ClickKind 'Heading*';SelectRange 0 $m.inline.input_units;TypeText 'Version One'
  Message 0x8021 4
  $m=ClickKind 'Paragraph';SelectRange 0 $m.inline.input_units
  $literal='Hello [v1] * literal & '+[char]::ConvertFromUtf32(0x1f600)
  TypeText $literal
  $m=Metrics 'selection-replaced';if($m.inline.pending){throw "Plain punctuation/cross-style replacement failed: $($m.error)"}
  SelectRange 0 5;Message 0x8020 2
  Start-Sleep -Milliseconds 150
  Message 0x8021 3
  $m=Metrics 'bold';if(!@($m.inline.styles|Where-Object bold -eq -1).Count -or (Working $m) -notmatch '\*\*Hello\*\*'){throw 'Native bold formatting/source failed'}
  SelectRange $m.inline.input_units $m.inline.input_units
  $child=[InlineStickerProbe]::GetDlgItem($script:hwnd,101);Send $child 0x100 13;Send $child 0x102 13;Start-Sleep -Milliseconds 200
  TypeText 'Second line'
  $m=Metrics 'new-paragraph';if($m.inline.pending -or !$m.inline.active){throw "Enter/new paragraph failed: $($m.error)"}
  Message 0x8021 3
  $m=Metrics 'newline-checkpoint'
  if((Working $m) -notmatch '(\r?\n){2}Second line' -or (Working $m) -notmatch '\*\*Hello\*\*'){throw 'Newline or earlier bold source was lost'}
  Capture 'editing'
  Message 0x8021 4
  $m=Metrics 'tasks';$task=@($m.inline.tasks|Where-Object checked -eq $false)[0]
  ClickPoint $m $task.x $task.y
  Message 0x8021 3;$m=Metrics 'task-checked';if((Working $m) -notmatch '\[x\] Ship v1'){throw 'Task toggle failed'}
  Message 0x111 230;Message 0x8021 3;$m=Metrics 'task-undone';if((Working $m) -notmatch '\[ \] Ship v1'){throw 'Task undo failed'}
  Message 0x111 231;Message 0x8021 3;$m=Metrics 'task-redone';$expected=Working $m
  if((ReadSource $original) -cne $before){throw 'Editing wrote original before Save'}
  CrashOwned;Launch
  $m=Metrics 'recovered';if(!$m.inline.dirty -or (Working $m) -cne $expected){throw 'Draft recovery failed'}
  Message 0x111 201;$m=Metrics 'saved'
  if($m.inline.dirty -or (ReadSource $original) -cne $expected -or (ReadSource ($original+'.bak')) -cne $before){throw 'Explicit Save/backup failed'}
  Capture 'wide'
  $null=[InlineStickerProbe]::SetWindowPos($script:hwnd,[IntPtr]::Zero,0,0,360,500,0x16);Start-Sleep -Milliseconds 150;Capture 'narrow'
  Message 0x100 0x23;Start-Sleep -Milliseconds 150
  $m=Metrics 'image-location';$image=@($m.inline.image_boxes)[0]
  ClickPoint $m ($image.x+$image.width/2) ($image.y+$image.height/2)
  $popup=[InlineStickerProbe]::Find($script:p.Id,'RustyNote.Image');if($popup -eq [IntPtr]::Zero){throw "Image enlargement failed: $($m.error)"}
  $owner=$script:hwnd;$script:hwnd=$popup;Capture 'image-fit';Send $popup 0x20a ([long]120 -shl 16);Start-Sleep -Milliseconds 100;Capture 'image-zoom'
  if((Get-FileHash (Join-Path $case 'image-fit.png')).Hash -eq (Get-FileHash (Join-Path $case 'image-zoom.png')).Hash){throw 'Image zoom did not change rendered pixels'}
  Send $popup 0x100 27;$script:hwnd=$owner
  if([InlineStickerProbe]::Find($script:p.Id,'RustyNote.Native') -ne $owner){throw 'Image close stopped main window'}
  Message 0x111 202;Start-Sleep -Milliseconds 200
  $m=Metrics 'new-note';if($m.selected -ne 'Note-001' -or !$m.inline.active){throw "New note not ready for input: $($m.error)"}
  TypeText 'My sticky note'
  $m=Metrics 'new-note-typed';if($m.inline.pending){throw "Empty/new note typing failed: $($m.error)"};Capture 'new-note'
  Message 0x111 201
  if((ReadSource (Join-Path $notes 'Note-001.md')) -notmatch 'My sticky note'){throw 'New note Save failed'}
  Message 0x8021 4
  Start-Sleep -Milliseconds 350
  $quiet=Metrics 'idle-before';$script:p.Refresh();$cpu=$script:p.TotalProcessorTime.TotalMilliseconds
  Start-Sleep -Milliseconds 1500;$script:p.Refresh();$delta=$script:p.TotalProcessorTime.TotalMilliseconds-$cpu
  $idle=Metrics 'idle-after'
  if($idle.paint_count -ne $quiet.paint_count -or $idle.inline.checkpoint_timer -or $idle.animation_active){throw 'Unexpected idle render/timer'}
  [PSCustomObject]@{theme=$theme;interval_ms=1500;cpu_ms=$delta;paints=$idle.paint_count-$quiet.paint_count;working_set_bytes=$script:p.WorkingSet64;private_bytes=$script:p.PrivateMemorySize64}|ConvertTo-Json|Set-Content (Join-Path $case 'profile.json')
  CloseOwned
  Write-Output "$theme essential v1 edit/newline/task/undo/recovery/Save/image/new-note smoke passed"
 }catch{Write-Output ($_|Out-String);throw}finally{if($script:p -and !$script:p.HasExited){$script:hwnd=[InlineStickerProbe]::Find($script:p.Id,'RustyNote.Native');CloseOwned 7}}
}
Write-Output "V1 smoke artifacts: $out"
