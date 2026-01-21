Param(
    [string]$ConfigPath = (Join-Path $PSScriptRoot "..\buddy\config.toml")
)

$ErrorActionPreference = "SilentlyContinue"
$BaseUrl = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main"
$ScriptDir = Split-Path -Path $MyInvocation.MyCommand.Path -Parent
$RootDir = Split-Path -Path $ScriptDir -Parent
$curl = Get-Command curl.exe -ErrorAction SilentlyContinue

function Get-ModelPath {
    if (-not (Test-Path $ConfigPath)) {
        return "models\ggml-base.en.bin"
    }
    $inTranscription = $false
    foreach ($line in Get-Content $ConfigPath) {
        $trim = $line.Trim()
        if ($trim -match '^\[(.+)\]$') {
            $inTranscription = $matches[1].Trim().ToLower() -eq "transcription"
            continue
        }
        if ($inTranscription -and $trim -match '^model_path\s*=\s*"([^"]+)"') {
            return $matches[1]
        }
    }
    return "models\ggml-base.en.bin"
}

function Resolve-ModelPath {
    param(
        [string]$PathValue
    )
    if ([System.IO.Path]::IsPathRooted($PathValue)) {
        return $PathValue
    }
    return Join-Path $RootDir $PathValue
}

$modelPath = Get-ModelPath
$resolvedPath = Resolve-ModelPath -PathValue $modelPath
$existing = Get-Item $resolvedPath -ErrorAction SilentlyContinue
if ($existing) {
    if ($existing.Length -ge 1000000) {
        exit 0
    }
    Remove-Item $resolvedPath -ErrorAction SilentlyContinue
}

$modelName = Split-Path -Path $resolvedPath -Leaf
if ([string]::IsNullOrWhiteSpace($modelName)) {
    Write-Host "Whisper model path is invalid: $resolvedPath"
    exit 1
}
if (-not ($modelName -like "ggml-*.bin")) {
    Write-Host "Whisper model not found at $resolvedPath"
    Write-Host "Set transcription.model_path to a valid ggml-*.bin file."
    exit 1
}

$targetDir = Split-Path -Path $resolvedPath -Parent
if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Path $targetDir | Out-Null
}

Write-Host "Downloading Whisper model $modelName..."
$downloadUrl = "$BaseUrl/$modelName"
if ($curl) {
    & $curl.Source -fL -o $resolvedPath $downloadUrl
    if ($LASTEXITCODE -ne 0) {
        Write-Host "Failed to download Whisper model from $downloadUrl"
        exit 1
    }
} else {
    try {
        Invoke-WebRequest -Uri $downloadUrl -OutFile $resolvedPath -UseBasicParsing | Out-Null
    } catch {
        Write-Host "Failed to download Whisper model from $downloadUrl"
        exit 1
    }
}

$info = Get-Item $resolvedPath -ErrorAction SilentlyContinue
if (-not $info -or $info.Length -lt 1000000) {
    if ($info) {
        Remove-Item $resolvedPath -ErrorAction SilentlyContinue
    }
    Write-Host "Downloaded file looks too small to be a Whisper model."
    Write-Host "Check your network or try again later."
    exit 1
}

exit 0
