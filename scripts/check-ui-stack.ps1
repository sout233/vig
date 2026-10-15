$ErrorActionPreference = 'Stop'
$vigPreviousStack = [Environment]::GetEnvironmentVariable('RUST_MIN_STACK', 'Process')
Push-Location (Split-Path -Parent $PSScriptRoot)
try {
    # Match the Windows executable's 1 MB main-thread stack.
    $env:RUST_MIN_STACK = '1048576'
    cargo test --features ui-verify 'views::timeline::' -- --nocapture
    if ($LASTEXITCODE -ne 0) {
        throw "UI stack regression failed with exit code $LASTEXITCODE"
    }
}
finally {
    [Environment]::SetEnvironmentVariable('RUST_MIN_STACK', $vigPreviousStack, 'Process')
    Pop-Location
}
