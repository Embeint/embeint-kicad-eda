# Export the clean HEAD as raw LF Git blobs: the pinned oracle reference (prepare.py) and the full
# source tree, with process-local core.autocrlf=false and core.eol=lf (no Git config is written).
$ErrorActionPreference = 'Stop'
$repo = 'C:\Users\KarlVonRichter\.t3\hash-wsl-recovery-20261123'
$msg = 'C:\Users\KarlVonRichter\.t3\hash-wsl-msg'
$env:GIT_CONFIG_COUNT = '2'
$env:GIT_CONFIG_KEY_0 = 'core.autocrlf'; $env:GIT_CONFIG_VALUE_0 = 'false'
$env:GIT_CONFIG_KEY_1 = 'core.eol'; $env:GIT_CONFIG_VALUE_1 = 'lf'
try {
    if (git -C $repo status --porcelain) { throw 'checkout is not clean' }
    $stage = Join-Path $msg 'oracle-stage'
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $stage
    Push-Location $repo
    try { python port/oracle/prepare.py --output-dir $stage } finally { Pop-Location }
    $tarPath = Join-Path $msg 'src.tar'
    Remove-Item -Force -ErrorAction SilentlyContinue $tarPath
    git -C $repo archive --format=tar "--output=$tarPath" HEAD
    if ($LASTEXITCODE -ne 0) { throw 'git archive failed' }
} finally {
    Remove-Item Env:GIT_CONFIG_COUNT, Env:GIT_CONFIG_KEY_0, Env:GIT_CONFIG_VALUE_0, Env:GIT_CONFIG_KEY_1, Env:GIT_CONFIG_VALUE_1
}
$m = Get-Content (Join-Path $msg 'oracle-stage\source-manifest.json') -Raw | ConvertFrom-Json
Write-Output "head=$((git -C $repo rev-parse HEAD).Trim()) tree=$((git -C $repo rev-parse 'HEAD^{tree}').Trim()) overlay=$($m.overlay_revision) dirty=$($m.overlay_dirty_at_export) archive=$($m.archive_sha256) src.tar=$((Get-Item (Join-Path $msg 'src.tar')).Length)"
