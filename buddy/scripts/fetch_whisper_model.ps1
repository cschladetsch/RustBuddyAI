Param(
    [string]$ModelName = "ggml-base.en.bin"
)

$BaseUrl = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main"
$ScriptDir = Split-Path -Path $MyInvocation.MyCommand.Path -Parent
$RootDir = Split-Path -Path $ScriptDir -Parent
$ModelDir = Join-Path $RootDir "models"
$TargetFile = Join-Path $ModelDir $ModelName

if (-not (Test-Path $ModelDir)) {
    New-Item -ItemType Directory -Path $ModelDir | Out-Null
}

if (Test-Path $TargetFile) {
    Write-Host "Model $ModelName already exists at $TargetFile."
    exit 0
}

if (-not (Get-Command Invoke-WebRequest -ErrorAction SilentlyContinue)) {
    Write-Error "PowerShell's Invoke-WebRequest is required to download Whisper models"
    exit 1
}

Write-Host "Downloading $ModelName..."
$DownloadUrl = "$BaseUrl/$ModelName?download=1"
Invoke-WebRequest -Uri $DownloadUrl -OutFile $TargetFile

Write-Host "Model saved to $TargetFile"
