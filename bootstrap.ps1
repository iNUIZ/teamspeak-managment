$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repository = 'iNUIZ/teamspeak-managment'
$apiHeaders = @{ Accept = 'application/vnd.github+json'; 'User-Agent' = 'Teamspeak-Managment-Installer' }
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("teamspeak-managment-" + [guid]::NewGuid().ToString('N'))

try {
    New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repository/releases/latest" -Headers $apiHeaders
    $zipAsset = $release.assets | Where-Object { $_.name -match '\.zip$' } | Select-Object -First 1
    $checksumAsset = $release.assets | Where-Object { $_.name -match 'SHA256SUMS\.txt$' } | Select-Object -First 1
    if (-not $zipAsset -or -not $checksumAsset) {
        throw 'Son sürümde ZIP veya SHA-256 doğrulama dosyası bulunamadı.'
    }

    $zipPath = Join-Path $temporaryRoot $zipAsset.name
    $checksumPath = Join-Path $temporaryRoot $checksumAsset.name
    Invoke-WebRequest -Uri $zipAsset.browser_download_url -OutFile $zipPath
    Invoke-WebRequest -Uri $checksumAsset.browser_download_url -OutFile $checksumPath

    $checksumLine = Get-Content -LiteralPath $checksumPath | Where-Object { $_ -match ([regex]::Escape($zipAsset.name) + '$') } | Select-Object -First 1
    if (-not $checksumLine -or $checksumLine -notmatch '^([a-fA-F0-9]{64})\s+') {
        throw 'ZIP için geçerli SHA-256 kaydı bulunamadı.'
    }
    $expectedHash = $matches[1].ToUpperInvariant()
    $actualHash = (Get-FileHash -LiteralPath $zipPath -Algorithm SHA256).Hash
    if ($actualHash -ne $expectedHash) {
        throw 'İndirilen paketin SHA-256 doğrulaması başarısız.'
    }

    $extractPath = Join-Path $temporaryRoot 'package'
    Expand-Archive -LiteralPath $zipPath -DestinationPath $extractPath
    & (Join-Path $extractPath 'install.ps1')
    exit $LASTEXITCODE
}
finally {
    if (Test-Path -LiteralPath $temporaryRoot) {
        Remove-Item -LiteralPath $temporaryRoot -Recurse -Force -ErrorAction SilentlyContinue
    }
}
