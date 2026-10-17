$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$repository = 'iNUIZ/teamspeak-managment'
$apiHeaders = @{ Accept = 'application/vnd.github+json'; 'User-Agent' = 'Teamspeak-Managment-Installer' }
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("teamspeak-managment-" + [guid]::NewGuid().ToString('N'))
$releaseSigningPublicKey = @'
-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEASPwsjRuEXYxMgbrwFPKFcouJaqd2lZZmhw+7nOwNzQE=
-----END PUBLIC KEY-----
'@

try {
    New-Item -ItemType Directory -Path $temporaryRoot | Out-Null
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repository/releases/latest" -Headers $apiHeaders
    $zipAsset = $release.assets | Where-Object { $_.name -match '\.zip$' } | Select-Object -First 1
    $checksumAsset = $release.assets | Where-Object { $_.name -match 'SHA256SUMS\.txt$' } | Select-Object -First 1
    $signatureAsset = $release.assets | Where-Object { $_.name -match 'SHA256SUMS\.txt\.sig$' } | Select-Object -First 1
    if (-not $zipAsset -or -not $checksumAsset -or -not $signatureAsset) {
        throw 'Son sürümde ZIP, SHA-256 manifesti veya imza dosyası bulunamadı.'
    }

    $zipPath = Join-Path $temporaryRoot $zipAsset.name
    $checksumPath = Join-Path $temporaryRoot $checksumAsset.name
    $signaturePath = Join-Path $temporaryRoot $signatureAsset.name
    $publicKeyPath = Join-Path $temporaryRoot 'release-signing-public.pem'
    Invoke-WebRequest -Uri $zipAsset.browser_download_url -OutFile $zipPath
    Invoke-WebRequest -Uri $checksumAsset.browser_download_url -OutFile $checksumPath
    Invoke-WebRequest -Uri $signatureAsset.browser_download_url -OutFile $signaturePath
    Set-Content -LiteralPath $publicKeyPath -Value $releaseSigningPublicKey -Encoding Ascii

    if (-not (Get-Command wsl.exe -ErrorAction SilentlyContinue)) {
        throw 'Sürüm imzasını doğrulamak için WSL2 gereklidir.'
    }
    $wslPublicKeyPath = (& wsl.exe wslpath -a $publicKeyPath).Trim()
    $wslChecksumPath = (& wsl.exe wslpath -a $checksumPath).Trim()
    $wslSignaturePath = (& wsl.exe wslpath -a $signaturePath).Trim()
    if ($LASTEXITCODE -ne 0 -or -not $wslPublicKeyPath -or -not $wslChecksumPath -or -not $wslSignaturePath) {
        throw 'Doğrulama dosyalarının WSL yolları çözülemedi.'
    }
    & wsl.exe openssl pkeyutl -verify -pubin -inkey $wslPublicKeyPath -rawin -in $wslChecksumPath -sigfile $wslSignaturePath | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw 'Sürüm imzası doğrulanamadı; paket güvenilir değil.'
    }

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
