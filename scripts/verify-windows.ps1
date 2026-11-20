# Runs the real app on a GitHub Windows desktop and checks what a user would see:
#   1. the panel opened from a bottom-right tray click lands fully on screen,
#   2. the posture reminder shows over a blurred desktop,
#   3. the caffeine reminder shows its Took it / Not now buttons,
#   4. the app downloads a newer version, swaps it in and relaunches as that version.
# Screenshots and the app's log go to ./check for review. Fails the build if the update doesn't land.
param([Parameter(Mandatory)] [string]$Version, [Parameter(Mandatory)] [string]$Out)
$ErrorActionPreference = 'Stop'

$app = 'C:\focus'; $srv = 'C:\focus-srv'; $check = Join-Path $PWD 'check'
New-Item -ItemType Directory -Force $app, $srv, $check | Out-Null
Expand-Archive (Join-Path $Out 'Focus-windows.zip') -DestinationPath $app -Force
Copy-Item (Join-Path $Out 'next-resources.neu') $srv
$next = "$Version.1"
@{ applicationId = 'app.posturenudge'; version = $next; resourcesURL = 'http://127.0.0.1:8765/next-resources.neu' } |
  ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $srv 'update.json')
Start-Process python -ArgumentList '-m', 'http.server', '8765', '--bind', '127.0.0.1' -WorkingDirectory $srv -WindowStyle Hidden

# Something recognisable on the desktop, so the blur is visible.
$note = Join-Path $srv 'desktop.txt'
(1..60 | ForEach-Object { "Line $_  —  The quick brown fox jumps over the lazy dog. FOCUS BLUR CHECK 0123456789" }) | Set-Content $note
Start-Process notepad $note -WindowStyle Maximized
Start-Sleep -Seconds 3

Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class Dpi { [DllImport("user32.dll")] public static extern bool SetProcessDPIAware(); }'
[Dpi]::SetProcessDPIAware() | Out-Null
function Shot([string]$name) {
  $v = [System.Windows.Forms.SystemInformation]::VirtualScreen
  $bmp = New-Object System.Drawing.Bitmap $v.Width, $v.Height
  [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($v.Left, $v.Top, 0, 0, $bmp.Size)
  $bmp.Save((Join-Path $check "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  Write-Host "screenshot $name ($($v.Width)x$($v.Height))"
}
$log = Join-Path $app 'focus-test.log'
function WaitFor([string]$pattern, [int]$seconds) {
  $deadline = (Get-Date).AddSeconds($seconds)
  while ((Get-Date) -lt $deadline) {
    if ((Test-Path $log) -and (Select-String -Path $log -Pattern $pattern -Quiet)) { return $true }
    Start-Sleep -Milliseconds 250
  }
  Write-Host "timed out waiting for: $pattern"; return $false
}

Shot 'desktop'
$env:FOCUS_TEST = '1'
$env:FOCUS_MANIFEST = 'http://127.0.0.1:8765/update.json'
Start-Process (Join-Path $app 'Focus-win_x64.exe') -WorkingDirectory $app

$ok = $true
if (WaitFor 'panel shown' 40) { Start-Sleep -Milliseconds 800; Shot '1-panel' } else { $ok = $false }
if (WaitFor 'popup shown kind=posture' 40) { Start-Sleep -Milliseconds 900; Shot '2-posture' } else { $ok = $false }
if (WaitFor 'popup shown kind=caffeine' 40) { Start-Sleep -Milliseconds 900; Shot '3-caffeine' } else { $ok = $false }
$updated = WaitFor "started $([regex]::Escape($next)) " 120
Start-Sleep -Seconds 2
Shot '4-after-update'

$want = (Get-FileHash (Join-Path $srv 'next-resources.neu')).Hash
$have = (Get-FileHash (Join-Path $app 'resources.neu')).Hash
Write-Host "resources.neu swapped: $($want -eq $have)"
if (Test-Path $log) { Copy-Item $log $check; Get-Content $log | Write-Host }
Get-Process Focus-win_x64 -ErrorAction SilentlyContinue | Stop-Process -Force

if (-not $updated -or $want -ne $have) { Write-Error 'Self-update did not complete' }
if (-not $ok) { Write-Error 'The app did not show every screen' }
Write-Host 'Windows check passed'
