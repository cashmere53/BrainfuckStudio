# OS の操作の補助スクリプト(Windows PowerShell 5.1。windowsHost.ts から固定の引数で起動する)。
# 動作ごとに結果の 1 行の JSON を標準出力へ書く。失敗は {"error": "..."} を書いて終了コード 1。
#   runtime : WebView2 の版(EdgeUpdate の pv。HKLM の WOW6432Node、HKCU の順)
#   window  : プロセスの主ウィンドウ(表示中で所有者の無いトップレベル)のタイトル
#   close   : 主ウィンドウへ WM_SYSCOMMAND/SC_CLOSE を送る(× ボタンと同じ OS の経路)
#   keys    : WScript.Shell の AppActivate で前面化し、前面になったら SendKeys を送る
#   menu    : 対象プロセスと子孫に属する既定のコンテキストメニュー(UI Automation の Menu)の有無
param(
    [Parameter(Mandatory = $true)]
    [ValidateSet('runtime', 'window', 'close', 'keys', 'menu')]
    [string]$Action,
    [int]$ProcessId = 0,
    [string]$Keys = ''
)

$ErrorActionPreference = 'Stop'

function Write-Line([string]$Line) {
    $bytes = [System.Text.Encoding]::UTF8.GetBytes($Line + "`n")
    $stream = [Console]::OpenStandardOutput()
    $stream.Write($bytes, 0, $bytes.Length)
    $stream.Flush()
}

# Win32 の型の読み込みも try の中に置き、失敗を {"error": ...} の 1 行の JSON で返す
try {
    Add-Type -TypeDefinition @'
using System;
using System.Text;
using System.Runtime.InteropServices;

public static class BfHostWin32
{
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern IntPtr GetWindow(IntPtr hWnd, uint uCmd);

    [DllImport("user32.dll")]
    public static extern int GetWindowTextLength(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern bool PostMessage(IntPtr hWnd, uint msg, IntPtr wParam, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern IntPtr GetForegroundWindow();

    private const uint GW_OWNER = 4;
    private const uint WM_SYSCOMMAND = 0x0112;
    private const long SC_CLOSE = 0xF060;

    public static IntPtr FindMainWindow(uint processId)
    {
        IntPtr found = IntPtr.Zero;
        EnumWindows(delegate (IntPtr h, IntPtr l)
        {
            uint pid;
            GetWindowThreadProcessId(h, out pid);
            if (pid == processId && IsWindowVisible(h) && GetWindow(h, GW_OWNER) == IntPtr.Zero)
            {
                found = h;
                return false;
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }

    public static string GetTitle(IntPtr h)
    {
        int length = GetWindowTextLength(h);
        StringBuilder sb = new StringBuilder(length + 1);
        GetWindowText(h, sb, sb.Capacity);
        return sb.ToString();
    }

    public static bool PostClose(IntPtr h)
    {
        return PostMessage(h, WM_SYSCOMMAND, (IntPtr)SC_CLOSE, IntPtr.Zero);
    }

    public static uint ForegroundProcessId()
    {
        uint pid;
        GetWindowThreadProcessId(GetForegroundWindow(), out pid);
        return pid;
    }
}
'@
} catch {
    Write-Line (ConvertTo-Json -Compress @{ error = "Win32 の型の読み込みに失敗しました: $($_.Exception.Message)" })
    exit 1
}

function Get-MainWindowHandle {
    return [BfHostWin32]::FindMainWindow([uint32]$ProcessId)
}

# 対象プロセスとその子孫(WebView2 の子プロセスを含む)の ID の集合
function Get-ProcessTreeIds([int]$RootId) {
    $children = @{}
    foreach ($process in (Get-CimInstance -ClassName Win32_Process)) {
        $parent = [int]$process.ParentProcessId
        if (-not $children.ContainsKey($parent)) {
            $children[$parent] = New-Object System.Collections.Generic.List[int]
        }
        $children[$parent].Add([int]$process.ProcessId)
    }
    $ids = New-Object System.Collections.Generic.HashSet[int]
    $queue = New-Object System.Collections.Generic.Queue[int]
    [void]$ids.Add($RootId)
    $queue.Enqueue($RootId)
    while ($queue.Count -gt 0) {
        $current = $queue.Dequeue()
        if ($children.ContainsKey($current)) {
            foreach ($child in $children[$current]) {
                if ($ids.Add($child)) {
                    $queue.Enqueue($child)
                }
            }
        }
    }
    return ,$ids
}

function Invoke-Runtime {
    $registryPaths = @(
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
    )
    $version = $null
    foreach ($registryPath in $registryPaths) {
        if (Test-Path -LiteralPath $registryPath) {
            $pv = (Get-ItemProperty -LiteralPath $registryPath -Name pv -ErrorAction SilentlyContinue).pv
            if (-not [string]::IsNullOrEmpty($pv)) {
                $version = [string]$pv
                break
            }
        }
    }
    Write-Line (ConvertTo-Json -Compress @{ action = 'runtime'; webView2Version = $version })
}

function Invoke-Window {
    $handle = Get-MainWindowHandle
    if ($handle.ToInt64() -eq 0) {
        Write-Line (ConvertTo-Json -Compress @{ action = 'window'; found = $false; title = $null })
        return
    }
    $title = [BfHostWin32]::GetTitle($handle)
    Write-Line (ConvertTo-Json -Compress @{ action = 'window'; found = $true; title = $title })
}

function Invoke-Close {
    $handle = Get-MainWindowHandle
    if ($handle.ToInt64() -eq 0) {
        Write-Line (ConvertTo-Json -Compress @{ action = 'close'; delivered = $false; reason = '主ウィンドウが見つかりません' })
        return
    }
    if (-not [BfHostWin32]::PostClose($handle)) {
        Write-Line (ConvertTo-Json -Compress @{ action = 'close'; delivered = $false; reason = '閉じる要求を送れませんでした' })
        return
    }
    Write-Line (ConvertTo-Json -Compress @{ action = 'close'; delivered = $true; reason = $null })
}

function Write-KeysResult([bool]$Delivered, [object]$Reason) {
    Write-Line (ConvertTo-Json -Compress @{ action = 'keys'; delivered = $Delivered; reason = $Reason })
}

function Invoke-Keys {
    $handle = Get-MainWindowHandle
    if ($handle.ToInt64() -eq 0) {
        Write-KeysResult $false '主ウィンドウが見つかりません'
        return
    }
    $target = [uint32]$ProcessId
    Add-Type -AssemblyName System.Windows.Forms
    $shell = New-Object -ComObject WScript.Shell
    # AppActivate の戻り値は、最小化からの復帰の途中では偽になりうるため、判定は前面の確認で行う
    [void]$shell.AppActivate($ProcessId)
    # 最小化からの復帰には時間がかかるため、前面になるまで最大 2 秒ほど待つ
    $deadline = [DateTime]::UtcNow.AddSeconds(2)
    while (([BfHostWin32]::ForegroundProcessId() -ne $target) -and ([DateTime]::UtcNow -lt $deadline)) {
        Start-Sleep -Milliseconds 50
    }
    if ([BfHostWin32]::ForegroundProcessId() -ne $target) {
        Write-KeysResult $false '前面化できません'
        return
    }
    # 送信の直前にも前面を確かめる(待ちの後に別のウィンドウへ移っていないか)
    if ([BfHostWin32]::ForegroundProcessId() -ne $target) {
        Write-KeysResult $false '送信の直前に前面が変わりました'
        return
    }
    [System.Windows.Forms.SendKeys]::SendWait($Keys)
    # 送信の後にも確かめる。前面が変わっていれば、キーは別のウィンドウへ届いた可能性がある
    if ([BfHostWin32]::ForegroundProcessId() -ne $target) {
        Write-KeysResult $false '送信の後に前面が変わりました(キーが別のウィンドウへ届いた可能性があります)'
        return
    }
    Write-KeysResult $true $null
}

function Invoke-Menu {
    Add-Type -AssemblyName UIAutomationClient
    Add-Type -AssemblyName UIAutomationTypes
    $treeIds = Get-ProcessTreeIds $ProcessId
    $menuCondition = New-Object System.Windows.Automation.PropertyCondition(
        [System.Windows.Automation.AutomationElement]::ControlTypeProperty,
        [System.Windows.Automation.ControlType]::Menu
    )
    $menus = [System.Windows.Automation.AutomationElement]::RootElement.FindAll(
        [System.Windows.Automation.TreeScope]::Children,
        $menuCondition
    )
    $menuOpen = $false
    foreach ($menu in $menus) {
        if ($treeIds.Contains([int]$menu.Current.ProcessId)) {
            $menuOpen = $true
            break
        }
    }
    Write-Line (ConvertTo-Json -Compress @{ action = 'menu'; menuOpen = $menuOpen })
}

try {
    if ($Action -ne 'runtime' -and $ProcessId -le 0) {
        throw "対象のプロセス ID が不正です: $ProcessId"
    }
    switch ($Action) {
        'runtime' { Invoke-Runtime }
        'window' { Invoke-Window }
        'close' { Invoke-Close }
        'keys' { Invoke-Keys }
        'menu' { Invoke-Menu }
    }
} catch {
    Write-Line (ConvertTo-Json -Compress @{ error = $_.Exception.Message })
    exit 1
}
