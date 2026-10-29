param([string]$InstallerPath)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Run this check on Windows.' }
if ($env:GITHUB_ACTIONS -ne 'true') {
    throw 'Run this installer check only on a disposable GitHub Actions Windows runner.'
}
if (-not $InstallerPath) {
    $InstallerPath = (Get-ChildItem 'target/release/bundle/nsis/*-setup.exe' | Select-Object -First 1).FullName
}
if (-not $InstallerPath) { throw 'No Windows installer was built.' }
$smokeRoot = Join-Path ([IO.Path]::GetTempPath()) ('rample-smoke-' + [guid]::NewGuid().ToString('N'))
$installRoot = Join-Path $smokeRoot 'app'
$env:RAMPLE_DATA_DIR = Join-Path $smokeRoot 'data'
$sessionPath = Join-Path $env:RAMPLE_DATA_DIR 'session.json'
$stdoutPath = Join-Path $smokeRoot 'startup.stdout.log'
$stderrPath = Join-Path $smokeRoot 'startup.stderr.log'
$appProcess = $null
$passed = $false
function Install-App {
    $installer = Start-Process -FilePath $InstallerPath -ArgumentList @('/S', "/D=$installRoot") -PassThru -Wait
    if ($installer.ExitCode -ne 0) { throw "Installer failed with code $($installer.ExitCode)." }
    if (-not (Test-Path (Join-Path $installRoot 'RamplePreview.exe'))) { throw 'Installed app is missing.' }
}
function Start-App {
    $script:appProcess = Start-Process -FilePath (Join-Path $installRoot 'RamplePreview.exe') -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    $startup = [Diagnostics.Stopwatch]::StartNew()
    $ready = $false
    while ($startup.Elapsed.TotalSeconds -lt 60) {
        Start-Sleep -Milliseconds 500
        $script:appProcess.Refresh()
        if ($script:appProcess.HasExited) { throw "Installed app exited during startup: $($script:appProcess.ExitCode)." }
        # First-run WebView2 initialization can exceed five seconds on hosted runners.
        # Wait for native setup and a real window, while still checking process health.
        if ($startup.Elapsed.TotalSeconds -ge 5 -and (Test-Path $sessionPath) -and $script:appProcess.MainWindowHandle -ne 0) {
            $ready = $true
            break
        }
    }
    if (-not $ready) { throw 'Native app setup did not create a session and window within 60 seconds.' }
    $session = Get-Content -Raw $sessionPath | ConvertFrom-Json
    if ($session.schemaVersion -ne 2 -or $session.voices.Count -ne 4) { throw 'Invalid first-launch session.' }
    Write-Output "Native session and window ready after $([math]::Round($startup.Elapsed.TotalSeconds, 1)) seconds."
}
function Stop-App {
    if ($script:appProcess -and -not $script:appProcess.HasExited) {
        Stop-Process -Id $script:appProcess.Id -Force
        $script:appProcess.WaitForExit()
    }
    $script:appProcess = $null
}
try {
    New-Item -ItemType Directory -Path $smokeRoot | Out-Null
    Install-App
    Start-App
    Stop-App
    $sessionHash = (Get-FileHash $sessionPath).Hash
    Install-App
    if ((Get-FileHash $sessionPath).Hash -ne $sessionHash) { throw 'Reinstall changed user preferences.' }
    Start-App
    Write-Output 'Windows installer, native startup and preference-preserving reinstall passed.'
    $passed = $true
} finally {
    Stop-App
    if (-not $passed) {
        foreach ($logPath in @($stdoutPath, $stderrPath)) {
            if (Test-Path $logPath) {
                Write-Output "Startup diagnostics: $logPath"
                Get-Content $logPath -Tail 100 | Write-Output
            }
        }
        Get-ChildItem $smokeRoot -Recurse -File | Select-Object -First 50 FullName, Length | Format-Table | Out-String | Write-Output
    }
    $uninstaller = Join-Path $installRoot 'uninstall.exe'
    if (Test-Path $uninstaller) {
        Start-Process -FilePath $uninstaller -ArgumentList '/S' -Wait | Out-Null
    }
    Remove-Item Env:RAMPLE_DATA_DIR -ErrorAction SilentlyContinue
    Remove-Item -Recurse -Force $smokeRoot -ErrorAction SilentlyContinue
}
