param([string]$InstallerPath)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'Run this check on Windows.' }
if ($env:GITHUB_ACTIONS -ne 'true') {
    throw 'Run this installer check only on a disposable GitHub Actions Windows runner.'
}
if (-not $InstallerPath) {
    $InstallerPath = (Get-ChildItem 'target/*/release/bundle/nsis/*-setup.exe', 'target/release/bundle/nsis/*-setup.exe' -ErrorAction SilentlyContinue | Select-Object -First 1).FullName
}
if (-not $InstallerPath) { throw 'No Windows installer was built.' }
$smokeRoot = Join-Path ([IO.Path]::GetTempPath()) ('rample-smoke-' + [guid]::NewGuid().ToString('N'))
$installRoot = Join-Path $smokeRoot 'app'
$env:RAMPLE_DATA_DIR = Join-Path $smokeRoot 'data'
$storePath = Join-Path $env:RAMPLE_DATA_DIR 'Library/library.sqlite3'
$stdoutPath = Join-Path $smokeRoot 'startup.stdout.log'
$stderrPath = Join-Path $smokeRoot 'startup.stderr.log'
$appProcess = $null
$passed = $false
function Install-App {
    $installer = Start-Process -FilePath $InstallerPath -ArgumentList @('/S', "/D=$installRoot") -PassThru -Wait
    if ($installer.ExitCode -ne 0) { throw "Installer failed with code $($installer.ExitCode)." }
    if (-not (Test-Path (Join-Path $installRoot 'RampleManager.exe'))) { throw 'Installed app is missing.' }
}
function Start-App {
    $script:appProcess = Start-Process -FilePath (Join-Path $installRoot 'RampleManager.exe') -PassThru -RedirectStandardOutput $stdoutPath -RedirectStandardError $stderrPath
    $startup = [Diagnostics.Stopwatch]::StartNew()
    $ready = $false
    while ($startup.Elapsed.TotalSeconds -lt 60) {
        Start-Sleep -Milliseconds 500
        $script:appProcess.Refresh()
        if ($script:appProcess.HasExited) { throw "Installed app exited during startup: $($script:appProcess.ExitCode)." }
        # First-run WebView2 initialization can exceed five seconds on hosted runners.
        # Readiness uses the native Library store and the window.
        if ($startup.Elapsed.TotalSeconds -ge 5 -and (Test-Path $storePath -PathType Leaf) -and $script:appProcess.MainWindowHandle -ne 0) {
            $ready = $true
            break
        }
    }
    if (-not $ready) { throw 'Native app setup did not create a Library store and window within 60 seconds.' }
    Write-Output "Native Library store and window ready after $([math]::Round($startup.Elapsed.TotalSeconds, 1)) seconds."
}
function Stop-App {
    param([switch]$Force)
    if ($script:appProcess -and -not $script:appProcess.HasExited) {
        if ($Force) {
            # Failed checks still clean up their disposable runner process.
            Stop-Process -Id $script:appProcess.Id -Force -ErrorAction SilentlyContinue
            $script:appProcess.WaitForExit()
        } else {
            if (-not $script:appProcess.CloseMainWindow()) { throw 'Could not request a graceful app close.' }
            if (-not $script:appProcess.WaitForExit(30000)) { throw 'App did not close gracefully within 30 seconds.' }
            if ($script:appProcess.ExitCode -ne 0) { throw "App close failed with code $($script:appProcess.ExitCode)." }
        }
    }
    $script:appProcess = $null
}
function Assert-Library {
    if (-not (Test-Path $storePath -PathType Leaf)) { throw 'The Library store is missing after a graceful close.' }
}
try {
    New-Item -ItemType Directory -Path $smokeRoot | Out-Null
    Install-App
    Start-App
    Stop-App
    Assert-Library
    $storeHash = (Get-FileHash $storePath).Hash
    Install-App
    if ((Get-FileHash $storePath).Hash -ne $storeHash) { throw 'Reinstall changed the Library.' }
    Start-App
    Stop-App
    Assert-Library
    Write-Output 'Windows installer, native startup, graceful close and Library-preserving reinstall passed.'
    $passed = $true
} finally {
    Stop-App -Force
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
