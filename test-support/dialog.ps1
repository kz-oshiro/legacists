param([Parameter(Mandatory)][int]$TargetProcessId,[Parameter(Mandatory)][string]$CreationTime,[Parameter(Mandatory)][ValidateSet('select','cancel')][string]$Action,[string]$Value,[string]$ReportPath)
$ErrorActionPreference = 'Stop'
$ErrorView = 'NormalView'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class DialogOwnership {
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr hWnd, uint uCmd);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] static extern bool AttachThreadInput(uint from,uint to,bool attach);
  [DllImport("user32.dll")] static extern IntPtr SetActiveWindow(IntPtr hWnd);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr hWnd,IntPtr after,int x,int y,int cx,int cy,uint flags);
  [StructLayout(LayoutKind.Sequential)] public struct Point { public int x,y; }
  [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point p);
  [DllImport("user32.dll")] static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr context);
  [DllImport("user32.dll")] static extern void mouse_event(uint flags,uint dx,uint dy,uint data,UIntPtr extra);
  public static void Click(int x,int y,uint expectedPid) {
    uint actual;GetWindowThreadProcessId(WindowFromPoint(new Point{x=x,y=y}),out actual);
    if(actual!=expectedPid)throw new Exception("Observed dialog control is covered: expected PID "+expectedPid+", actual PID "+actual+", point "+x+","+y);
    SetCursorPos(x,y);mouse_event(2,0,0,0,UIntPtr.Zero);mouse_event(4,0,0,0,UIntPtr.Zero);
  }
  public static void Activate(IntPtr hWnd) {
    SetWindowPos(hWnd,new IntPtr(-1),0,0,0,0,0x43);
    uint unused; uint current=GetCurrentThreadId(), foreground=GetWindowThreadProcessId(GetForegroundWindow(),out unused),target=GetWindowThreadProcessId(hWnd,out unused);
    bool a=foreground!=0&&foreground!=current&&AttachThreadInput(current,foreground,true);
    bool b=target!=current&&target!=foreground&&AttachThreadInput(current,target,true);
    try {SetForegroundWindow(hWnd);SetActiveWindow(hWnd);}
    finally {if(b)AttachThreadInput(current,target,false);if(a)AttachThreadInput(current,foreground,false);}
  }
}
'@
[void][DialogOwnership]::SetProcessDpiAwarenessContext([IntPtr](-4))
function Assert-Process {
    $processInfo = Get-CimInstance Win32_Process -Filter "ProcessId = $TargetProcessId"
    if (-not $processInfo -or $processInfo.CreationDate.ToFileTimeUtc().ToString() -ne $CreationTime) { throw 'Process ownership changed' }
}
function Find-Dialog {
    Assert-Process
    $condition = [Windows.Automation.AndCondition]::new([Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ProcessIdProperty,$TargetProcessId),[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ClassNameProperty,'#32770'))
    $dialogs = [Windows.Automation.AutomationElement]::RootElement.FindAll([Windows.Automation.TreeScope]::Descendants,$condition)
    foreach ($candidate in $dialogs) {
        if ($candidate.Current.IsOffscreen) { continue }
        $owner = [DialogOwnership]::GetWindow([IntPtr]$candidate.Current.NativeWindowHandle,4)
        if ($owner -eq [IntPtr]::Zero) { continue }
        [uint32]$ownerProcessId = 0
        [void][DialogOwnership]::GetWindowThreadProcessId($owner,[ref]$ownerProcessId)
        if ($ownerProcessId -eq $TargetProcessId) { return $candidate }
    }
    return $null
}
function Find-Id($Element,[string]$AutomationId) {
    $Element.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::AutomationIdProperty,$AutomationId))
}
function Click-Control($Element) {
    Assert-Process
    $point=$Element.GetClickablePoint()
    [DialogOwnership]::Click([int]$point.X,[int]$point.Y,[uint32]$TargetProcessId)
}
$deadline = [DateTime]::UtcNow.AddSeconds(20)
$dialog = $null
while ([DateTime]::UtcNow -lt $deadline -and -not $dialog) { $dialog = Find-Dialog; if (-not $dialog) { Start-Sleep -Milliseconds 100 } }
if (-not $dialog) { throw 'Native dialog unavailable for owned process' }
$observed = @($dialog.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition) | ForEach-Object { @{id=$_.Current.AutomationId;type=$_.Current.ControlType.ProgrammaticName;class=$_.Current.ClassName} })
if ($ReportPath) { @{processId=$TargetProcessId;creationTime=$CreationTime;action=$Action;controls=$observed;status='observed'} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ReportPath -Encoding utf8NoBOM }
if ($Action -eq 'cancel') {
    $button = Find-Id $dialog '2'
    if (-not $button) { throw 'Cancel control unavailable' }
    ([Windows.Automation.InvokePattern]$button.GetCurrentPattern([Windows.Automation.InvokePattern]::Pattern)).Invoke()
} else {
    $edit = Find-Id $dialog '1152'
    if (-not $edit) { $edit = Find-Id $dialog '1148' }
    if ($edit -and $edit.Current.ControlType -ne [Windows.Automation.ControlType]::Edit) { $edit = $edit.FindFirst([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::Edit)) }
    if (-not $edit) { throw 'File/folder input control unavailable' }
    $isFolder=Test-Path -LiteralPath $Value -PathType Container
    $entryValue=if($isFolder){Split-Path -Parent $Value}else{$Value}
    [DialogOwnership]::Activate([IntPtr]$dialog.Current.NativeWindowHandle)
    Start-Sleep -Milliseconds 200
    Click-Control $edit
    $edit.SetFocus()
    ([Windows.Automation.ValuePattern]$edit.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).SetValue($entryValue)
    if (([Windows.Automation.ValuePattern]$edit.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).Current.Value -ne $entryValue) { throw 'Native edit did not accept requested path' }
    $button = Find-Id $dialog '1'
    if (-not $button) { throw 'Accept control unavailable' }
    Assert-Process
    [uint32]$foregroundProcessId = 0
    [void][DialogOwnership]::GetWindowThreadProcessId([DialogOwnership]::GetForegroundWindow(),[ref]$foregroundProcessId)
    if($isFolder) {
        if ($foregroundProcessId -ne $TargetProcessId){throw 'Owned dialog lost keyboard focus'}
        [System.Windows.Forms.SendKeys]::SendWait('{ENTER}')
    }else{Click-Control $button}
    # IFileOpenDialog navigates to a typed directory first. Its next Select
    # invocation accepts that current directory; do not repeat for file saves.
    if ($isFolder) {
        Start-Sleep -Milliseconds 500
        $current = Find-Dialog
        if ($current -and $current.Current.NativeWindowHandle -eq $dialog.Current.NativeWindowHandle) {
            $address = Find-Id $current '1001'
            if (-not $address -or -not $address.Current.Name.Contains($entryValue,[StringComparison]::OrdinalIgnoreCase)) { throw 'Folder dialog did not navigate to the requested parent' }
            $items=$current.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::ControlTypeProperty,[Windows.Automation.ControlType]::ListItem))
            $item=@($items | Where-Object {$_.Current.Name -eq (Split-Path -Leaf $Value)})
            if ($item.Count -ne 1) {throw 'Requested folder list item unavailable'}
            ([Windows.Automation.SelectionItemPattern]$item[0].GetCurrentPattern([Windows.Automation.SelectionItemPattern]::Pattern)).Select()
            $button = Find-Id $current '1'
            [DialogOwnership]::Activate([IntPtr]$current.Current.NativeWindowHandle)
            Click-Control $button
        }
    }
}
$deadline = [DateTime]::UtcNow.AddSeconds(10)
while ([DateTime]::UtcNow -lt $deadline) { if (-not (Find-Dialog)) { if ($ReportPath) { @{processId=$TargetProcessId;creationTime=$CreationTime;action=$Action;controls=$observed;status='passed'} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ReportPath -Encoding utf8NoBOM }; Write-Output 'Owned native dialog operation passed'; exit 0 }; Start-Sleep -Milliseconds 100 }
if ($ReportPath) {
    $remaining = Find-Dialog
    $after = @($remaining.FindAll([Windows.Automation.TreeScope]::Descendants,[Windows.Automation.Condition]::TrueCondition) | Where-Object {$_.Current.AutomationId -in '1152','1148','1','2'} | ForEach-Object { @{id=$_.Current.AutomationId;type=$_.Current.ControlType.ProgrammaticName;enabled=$_.Current.IsEnabled;hwnd=$_.Current.NativeWindowHandle} })
    $editValue=([Windows.Automation.ValuePattern]$edit.GetCurrentPattern([Windows.Automation.ValuePattern]::Pattern)).Current.Value
    @{processId=$TargetProcessId;creationTime=$CreationTime;action=$Action;controls=$observed;after=$after;editValue=$editValue;hwnd=$remaining.Current.NativeWindowHandle;foregroundProcessId=$foregroundProcessId;status='failed'} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ReportPath -Encoding utf8NoBOM
}
throw 'Native dialog did not close after control invocation'
