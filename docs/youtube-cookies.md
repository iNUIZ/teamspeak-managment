# YouTube: yalnızca çerezle oynatma / cookie-only playback

YouTube arama ve oynatma yalnızca yöneticinin sağladığı `cookies.txt` ile çalışır.
Token hizmeti, otomatik anonim alternatif veya ek proxy kurulmaz.

## Kullanım

1. Ayarlar → YouTube bölümünden kendi Netscape biçimli `cookies.txt` dosyanızı yükleyin.
2. Yüklenen dosya yeni arama ve oynatma işlemlerinde otomatik kullanılır; ayrıca yöntem seçilmez.
3. Çerez yoksa veya dosya okunamıyorsa YouTube işlemleri açık bir uyarıyla durur.
4. Süresi dolan veya reddedilen çerezleri aynı ekrandan yenileyin. Çerezler ağ, hesap veya bölge kısıtlamalarının aşılmasını garanti etmez.

Radyo ve yerel medya çerez gerektirmez. Spotify/Apple Music eşleşmeleri ses için
YouTube'a yöneliyorsa aynı çerez koşulu geçerlidir; doğrudan Spotify ses indirmesi yapılmaz.

Dosya özel hesap bilgisi içerir: Git'e eklemeyin, loglamayın, paylaşmayın.
Panel yüklemesi kalıcı veri dizininde 0600 izinleriyle tutulur. Güncelleme veri
volümlerini korumalıdır; çalışan indirmeler yerine yeni işlemler güncel dosyayı kullanır.

Docker Linux/Windows/macOS kurulumları aynı davranışı kullanır. Yerel backend'de
`YT_COOKIE_FILE` okunabilir bir dosyaya işaret edebilir. Panel yüklemesi önceliklidir.
Panel yalnızca kendi yüklediği dosyayı kaldırır; ortam değişkeniyle tanımlanan dosyayı
silmez. Bu dosya varsa panel yüklemesi kaldırıldıktan sonra kullanılır ve durum ekranında gösterilir.
Tüm çerez kullanımını kaldırmak için operatör bu ortam ayarını da kaldırmalıdır.
Eski yöntem ayarı yok sayılır; eski `/yt-cookies/mode` API'si 410 döndürür.

## English

Upload your own Netscape-format `cookies.txt` under Settings → YouTube. The file is
used immediately for new YouTube searches and playback. Missing or unreadable
cookies block YouTube requests, without an anonymous/provider fallback. Radio and
local media remain independent. Refresh expired cookies; access is not guaranteed.
Never commit, log or share cookies. Preserve the encrypted backups and data volumes.
Native deployments can use `YT_COOKIE_FILE`; panel uploads take precedence. Removing
a panel upload does not delete an operator-managed environment file.
