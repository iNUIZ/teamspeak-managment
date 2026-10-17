$ErrorActionPreference = 'Stop'

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    throw 'Docker Desktop bulunamadı. Önce Docker Desktop kurup başlatın.'
}

docker compose version | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw 'Docker Compose eklentisi bulunamadı.'
}

$bashCandidates = @(
    (Get-Command bash.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
    "$env:ProgramFiles\Git\bin\bash.exe"
) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }

if (-not $bashCandidates) {
    throw 'Git Bash bulunamadı. Git for Windows kurun veya install.sh dosyasını WSL içinde çalıştırın.'
}

$bash = $bashCandidates[0]
$script = (Join-Path $PSScriptRoot 'install.sh') -replace '\\', '/'
& $bash $script
exit $LASTEXITCODE
