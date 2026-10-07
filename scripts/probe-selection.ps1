param([Parameter(Mandatory=$true)][string]$OutputDirectory)
$ErrorActionPreference='Stop'
Add-Type -AssemblyName PresentationFramework
Add-Type -AssemblyName PresentationCore
[System.Windows.Media.RenderOptions]::ProcessRenderMode=[System.Windows.Interop.RenderMode]::SoftwareOnly
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class SelectionFixtureInput {
  [DllImport("user32.dll")] public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
  [StructLayout(LayoutKind.Sequential)] public struct KEYBDINPUT { public ushort vk,scan; public uint flags,time; public IntPtr extra; }
  [StructLayout(LayoutKind.Sequential)] public struct MOUSEINPUT { public int x,y; public uint data,flags,time; public IntPtr extra; }
  [StructLayout(LayoutKind.Explicit)] public struct INPUTUNION { [FieldOffset(0)] public KEYBDINPUT key; [FieldOffset(0)] public MOUSEINPUT mouse; }
  [StructLayout(LayoutKind.Sequential)] public struct INPUT { public uint type; public INPUTUNION data; }
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window,out uint process);
  public static uint ForegroundProcess() { uint process;GetWindowThreadProcessId(GetForegroundWindow(),out process);return process; }
  [DllImport("user32.dll")] static extern IntPtr GetProcessWindowStation();
  [DllImport("user32.dll",SetLastError=true)] static extern IntPtr OpenInputDesktop(uint flags,bool inherit,uint access);
  [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr desktop);
  [DllImport("user32.dll",CharSet=CharSet.Unicode,SetLastError=true)] static extern bool GetUserObjectInformation(IntPtr handle,int index,StringBuilder text,uint length,out uint needed);
  static string ObjectName(IntPtr handle) { uint needed;var text=new StringBuilder(256);return GetUserObjectInformation(handle,2,text,512,out needed)?text.ToString():"error:"+Marshal.GetLastWin32Error(); }
  public static string DesktopStatus() { var desktop=OpenInputDesktop(0,false,1);var inputError=Marshal.GetLastWin32Error();var station=ObjectName(GetProcessWindowStation());if(desktop==IntPtr.Zero)return station+"/input-error:"+inputError;try{return station+"/"+ObjectName(desktop);}finally{CloseDesktop(desktop);} }
  [DllImport("user32.dll",SetLastError=true)] static extern uint SendInput(uint count,INPUT[] inputs,int size);
  public static void Trigger(IntPtr fixture,ushort letter) {
    if(GetForegroundWindow()!=fixture) throw new InvalidOperationException("Fixture is not foreground; no keys sent.");
    var keys=new ushort[]{0x11,0x12,0x10,letter,letter,0x10,0x12,0x11};var inputs=new INPUT[8];
    for(int i=0;i<8;i++){inputs[i].type=1;inputs[i].data.key.vk=keys[i];inputs[i].data.key.flags=(uint)(i<4?0:2);}
    if(SendInput(8,inputs,Marshal.SizeOf(typeof(INPUT)))!=8) throw new InvalidOperationException("Keyboard injection failed.");
  }
}
'@
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
$fixtureDirectory=(Resolve-Path -LiteralPath $OutputDirectory).Path
$fixtureWindow=New-Object System.Windows.Window
$fixtureWindow.Title='SubtitleVocabularyList capture fixture'
$fixtureWindow.Topmost=$true
$fixtureWindow.Background=[System.Windows.Media.Brushes]::White
$fixtureWindow.Width=720;$fixtureWindow.Height=230
$fixtureWindow.Left=90;$fixtureWindow.Top=90
$fixturePanel=New-Object System.Windows.Controls.StackPanel
$fixturePanel.Margin='24'
$fixtureLabel=New-Object System.Windows.Controls.TextBlock
$fixtureLabel.Text='Synthetic English text for selection validation. No user documents are opened.'
$fixtureLabel.Margin='0,0,0,15';$fixturePanel.Children.Add($fixtureLabel)|Out-Null
$fixtureText=New-Object System.Windows.Controls.TextBox
$fixtureText.Text='I was reluctant to ask for help.';$fixtureText.FontSize=24;$fixtureText.IsReadOnly=$true
$fixtureText.Foreground=[System.Windows.Media.Brushes]::Black;$fixtureText.Background=[System.Windows.Media.Brushes]::White
$fixturePanel.Children.Add($fixtureText)|Out-Null;$fixtureWindow.Content=$fixturePanel
$fixtureWindow.Add_Loaded({$fixtureWindow.Activate()|Out-Null;$fixtureText.Focus()|Out-Null;$fixtureText.Select(6,9)})
$fixtureTimer=New-Object System.Windows.Threading.DispatcherTimer
$fixtureTimer.Interval=[TimeSpan]::FromMilliseconds(150)
$fixtureTimer.Add_Tick({
  $fixtureCommandFile=Join-Path $fixtureDirectory 'command.txt'
  if(Test-Path -LiteralPath $fixtureCommandFile){
    $fixtureCommand=[IO.File]::ReadAllText($fixtureCommandFile).Trim()
    Remove-Item -LiteralPath $fixtureCommandFile
    try {
      switch($fixtureCommand){
        'select' {$fixtureWindow.Activate()|Out-Null;$fixtureText.Focus()|Out-Null;$fixtureText.Select(6,9)}
        'empty' {$fixtureWindow.Activate()|Out-Null;$fixtureText.Focus()|Out-Null;$fixtureText.Select(0,0)}
        'bounds' {$fixtureText.Select(0,0)}
        'trigger' {$fixtureHandle=(New-Object System.Windows.Interop.WindowInteropHelper($fixtureWindow)).Handle;[SelectionFixtureInput]::Trigger($fixtureHandle,0x57)}
        'quit' {$fixtureWindow.Close()}
        default {throw 'Unknown fixture action'}
      }
      $fixtureTopLeft=$fixtureText.PointToScreen([System.Windows.Point]::new(0,0))
      $fixtureBottomRight=$fixtureText.PointToScreen([System.Windows.Point]::new($fixtureText.ActualWidth,$fixtureText.ActualHeight))
      @{action=$fixtureCommand;selected=$fixtureText.SelectedText;pid=$PID;active=$fixtureWindow.IsActive;foregroundProcess=[SelectionFixtureInput]::ForegroundProcess();foregroundHandle=[SelectionFixtureInput]::GetForegroundWindow().ToInt64();fixtureHandle=(New-Object System.Windows.Interop.WindowInteropHelper($fixtureWindow)).Handle.ToInt64();bounds=@{x=$fixtureTopLeft.X;y=$fixtureTopLeft.Y;width=$fixtureBottomRight.X-$fixtureTopLeft.X;height=$fixtureBottomRight.Y-$fixtureTopLeft.Y}}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $fixtureDirectory 'status.json') -Encoding utf8
    }catch{ @{action=$fixtureCommand;error=$_.Exception.Message;active=$fixtureWindow.IsActive;desktop=[SelectionFixtureInput]::DesktopStatus();foregroundProcess=[SelectionFixtureInput]::ForegroundProcess();foregroundHandle=[SelectionFixtureInput]::GetForegroundWindow().ToInt64();fixtureHandle=(New-Object System.Windows.Interop.WindowInteropHelper($fixtureWindow)).Handle.ToInt64()}|ConvertTo-Json|Set-Content -LiteralPath (Join-Path $fixtureDirectory 'status.json') -Encoding utf8 }
  }
})
$fixtureTimer.Start()
Write-Output "Selection fixture ready: process $PID"
$fixtureWindow.ShowDialog()|Out-Null
$fixtureTimer.Stop()
