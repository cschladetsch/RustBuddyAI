$ErrorActionPreference = "SilentlyContinue"

$endpoint = "http://localhost:11434/api/tags"

$curl = Get-Command curl.exe -ErrorAction SilentlyContinue

function Test-DeepSeek {
    if ($curl) {
        $status = & $curl.Source -s -o NUL -w "%{http_code}" $endpoint
        return $status -eq "200"
    }
    try {
        Invoke-WebRequest -TimeoutSec 2 -Uri $endpoint | Out-Null
        return $true
    } catch {
        return $false
    }
}

if (Test-DeepSeek) {
    exit 0
}

if (-not (Get-Command ollama -ErrorAction SilentlyContinue)) {
    Write-Host "Ollama not found on PATH. Please install it from https://ollama.ai"
    exit 0
}

Write-Host "DeepSeek not running; starting Ollama..."

$existing = Get-Process -Name "ollama" -ErrorAction SilentlyContinue
if (-not $existing) {
    Start-Process -WindowStyle Minimized -FilePath "cmd.exe" -ArgumentList "/c", "ollama serve"
}

for ($i = 0; $i -lt 120; $i++) {
    if (Test-DeepSeek) {
        exit 0
    }
    Start-Sleep -Milliseconds 500
}

Write-Host "DeepSeek server did not respond on $endpoint"
Write-Host "Try running: ollama serve"
exit 0
