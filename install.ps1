$ErrorActionPreference = 'Stop'

function Assert-Command {
    param([Parameter(Mandatory)][string]$Name, [Parameter(Mandatory)][string]$Message)
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw $Message
    }
}

Assert-Command -Name 'docker' -Message 'Docker Desktop bulunamadı. Docker Desktop ve WSL2 entegrasyonunu kurun.'
Assert-Command -Name 'wsl.exe' -Message "WSL2 bulunamadı. Yönetici PowerShell ile wsl --install çalıştırıp Windows'u yeniden başlatın."

docker compose version | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "Docker Compose eklentisi bulunamadı. Docker Desktop'ı güncelleyin."
}

$scriptPath = (Resolve-Path (Join-Path $PSScriptRoot 'install.sh')).Path
$linuxScript = (& wsl.exe wslpath -a $scriptPath).Trim()
if ($LASTEXITCODE -ne 0 -or -not $linuxScript) {
    throw 'Proje yolu WSL yoluna dönüştürülemedi.'
}

& wsl.exe bash -lc "docker compose version >/dev/null 2>&1"
if ($LASTEXITCODE -ne 0) {
    throw 'Docker WSL içinde erişilebilir değil. Docker Desktop > Settings > Resources > WSL Integration bölümünden dağıtımınızı etkinleştirin.'
}

Write-Host 'Teamspeak Managment kurulumu WSL2 üzerinden başlatılıyor...' -ForegroundColor Cyan
Write-Host 'Manager imajı çerezsiz YouTube PO Token hizmetini içerir; Windows üzerine ayrıca Python veya token hizmeti kurmanız gerekmez.' -ForegroundColor DarkCyan
if ($linuxScript.Contains("'")) {
    throw "Proje yolu tek tırnak içeremez: $linuxScript"
}
$escapedScript = $linuxScript
& wsl.exe bash -lc "chmod +x '$escapedScript' && sudo '$escapedScript'"
exit $LASTEXITCODE
