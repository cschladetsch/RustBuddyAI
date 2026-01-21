$ErrorActionPreference = "SilentlyContinue"

Param(
    [string]$ConfigPath = (Join-Path $PSScriptRoot "..\buddy\config.toml")
)

$endpoint = "http://localhost:11434/api/tags"
$chatEndpoint = "http://localhost:11434/api/chat"
$ollamaLog = Join-Path $env:TEMP "ollama-serve.log"

$curl = Get-Command curl.exe -ErrorAction SilentlyContinue
$ollamaCmd = $null

function Test-DeepSeek {
    if ($curl) {
        $status = & $curl.Source -s -o NUL -w "%{http_code}" $endpoint
        return $status -eq "200"
    }
    try {
        Invoke-WebRequest -TimeoutSec 2 -Uri $endpoint -UseBasicParsing | Out-Null
        return $true
    } catch {
        return $false
    }
}

function Test-DeepSeekChat {
    param(
        [string]$Model
    )
    $payload = @{
        model = $Model
        messages = @(@{ role = "user"; content = "ping" })
        stream = $false
    } | ConvertTo-Json -Compress
    if ($curl) {
        $status = & $curl.Source -s -o NUL -w "%{http_code}" -m 60 -H "Content-Type: application/json" -d $payload $chatEndpoint
        return $status -eq "200"
    }
    try {
        Invoke-WebRequest -TimeoutSec 60 -Method Post -Uri $chatEndpoint -Body $payload -ContentType "application/json" -UseBasicParsing | Out-Null
        return $true
    } catch {
        return $false
    }
}

function Wait-For-Chat {
    param(
        [string]$Model,
        [int]$Attempts = 5,
        [int]$DelayMs = 1000
    )
    for ($i = 0; $i -lt $Attempts; $i++) {
        if (Test-DeepSeekChat -Model $Model) {
            return $true
        }
        Start-Sleep -Milliseconds $DelayMs
    }
    return $false
}

function Warm-Model {
    param(
        [string]$Model
    )
    if (-not $ollamaCmd) {
        return $false
    }
    Write-Host "Warming model $Model..."
    $warmLog = Join-Path $env:TEMP "ollama-warm.log"
    & $ollamaCmd run $Model "ping" *>&1 | Out-File -FilePath $warmLog -Encoding utf8
    if ($LASTEXITCODE -eq 0) {
        return $true
    }
    Write-Host "Ollama warm-up failed. Log (last 20 lines):"
    if (Test-Path $warmLog) {
        Get-Content -Tail 20 $warmLog
    }
    return $false
}

function Resolve-OllamaCommand {
    $cmd = Get-Command ollama -ErrorAction SilentlyContinue
    if ($cmd) {
        return $cmd.Source
    }
    $candidates = @(
        (Join-Path $env:ProgramFiles "Ollama\ollama.exe"),
        (Join-Path $env:LOCALAPPDATA "Programs\Ollama\ollama.exe")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            return $candidate
        }
    }
    return $null
}

function Ensure-Ollama {
    $resolved = Resolve-OllamaCommand
    if ($resolved) {
        return $resolved
    }
    $winget = Get-Command winget -ErrorAction SilentlyContinue
    if (-not $winget) {
        Write-Host "Ollama not found and winget is unavailable. Install it from https://ollama.ai"
        return $null
    }
    Write-Host "Ollama not found; installing via winget..."
    & $winget.Source install -e --id Ollama.Ollama --accept-source-agreements --accept-package-agreements
    $resolved = Resolve-OllamaCommand
    if (-not $resolved) {
        Write-Host "Ollama install completed but it is still not available on PATH."
        Write-Host "Try restarting your terminal or reinstalling from https://ollama.ai"
        return $null
    }
    return $resolved
}

function Get-DeepSeekModel {
    if (-not (Test-Path $ConfigPath)) {
        return "deepseek-r1:latest"
    }
    $inDeepSeek = $false
    foreach ($line in Get-Content $ConfigPath) {
        $trim = $line.Trim()
        if ($trim -match '^\[(.+)\]$') {
            $inDeepSeek = $matches[1].Trim().ToLower() -eq "deepseek"
            continue
        }
        if ($inDeepSeek -and $trim -match '^model\s*=\s*"([^"]+)"') {
            return $matches[1]
        }
    }
    return "deepseek-r1:latest"
}

function Get-AvailableModels {
    $models = @()
    try {
        $resp = Invoke-WebRequest -TimeoutSec 2 -Uri $endpoint -UseBasicParsing
        $json = $resp.Content | ConvertFrom-Json
        $models += @($json.models | ForEach-Object { $_.name })
    } catch {
    }
    if ($ollamaCmd) {
        $lines = & $ollamaCmd list 2>$null
        foreach ($line in $lines) {
            if ($line -match '^\s*NAME\s+ID\s+SIZE\s+MODIFIED') {
                continue
            }
            if ($line -match '^\s*([^\s]+)\s+') {
                $models += $matches[1]
            }
        }
    }
    return $models | Select-Object -Unique
}

function Ensure-Model {
    param(
        [string]$Model
    )
    if (-not $ollamaCmd) {
        $script:ollamaCmd = Ensure-Ollama
    }
    $models = Get-AvailableModels
    if ($models -contains $Model) {
        return $true
    }
    if (-not $ollamaCmd) {
        return $false
    }
    Write-Host "Pulling model $Model..."
    & $ollamaCmd pull $Model
    return $LASTEXITCODE -eq 0
}

if (Test-DeepSeek) {
    $model = Get-DeepSeekModel
    if (Ensure-Model -Model $model) {
        if (Warm-Model -Model $model) {
            exit 0
        }
        exit 1
    }
    Write-Host "DeepSeek model '$model' not available."
    exit 1
}

$ollamaCmd = Ensure-Ollama
if (-not $ollamaCmd) {
    exit 1
}

Write-Host "DeepSeek not running; starting Ollama..."

$existing = Get-Process -Name "ollama" -ErrorAction SilentlyContinue
if (-not $existing) {
    Start-Process -WindowStyle Minimized -FilePath $ollamaCmd -ArgumentList "serve" -RedirectStandardOutput $ollamaLog -RedirectStandardError $ollamaLog | Out-Null
}

for ($i = 0; $i -lt 120; $i++) {
    if (Test-DeepSeek) {
        $model = Get-DeepSeekModel
        if (Ensure-Model -Model $model) {
            if (Warm-Model -Model $model) {
                exit 0
            }
            exit 1
        }
        Write-Host "DeepSeek model '$model' not available."
        exit 1
    }
    Start-Sleep -Milliseconds 500
}

Write-Host "DeepSeek server did not respond on $endpoint"
Write-Host "Try running: ollama serve"
exit 1
