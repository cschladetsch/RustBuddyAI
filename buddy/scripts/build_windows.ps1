Param(
    [string[]]$CargoArgs
)

$ScriptDir = Split-Path -Path $MyInvocation.MyCommand.Path -Parent
$RootDir = Split-Path -Path $ScriptDir -Parent
Push-Location $RootDir
try {
    cargo build --release --target x86_64-pc-windows-gnu @CargoArgs
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
