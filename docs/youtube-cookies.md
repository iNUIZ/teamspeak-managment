# YouTube: yalnızca çerezle oynatma / cookie-only playback

YouTube arama ve oynatma yalnızca yöneticinin sağladığı `cookies.txt` ile çalışır.
Token hizmeti, otomatik anonim alternatif veya ek proxy kurulmaz.

## Kullanım

1. Ayarlar → YouTube bölümünden kendi Netscape biçimli `cookies.txt` dosyanızı yükleyin.
2. Yüklenen dosya yeni arama ve oynatma işlemlerinde otomatik kullanılır; ayrıca yöntem seçilmez.
3. Çerez yoksa veya dosya okunamıyorsa YouTube işlemleri açık bir uyarıyla durur.
4. Süresi dolan veya reddedilen çerezleri aynı ekrandan yenileyin. Çerezler ağ, hesap veya bölge kısıtlamalarının aşılmasını garanti etmez.

## Çerezi uzun ömürlü almak (önemli)

Veri merkezi IP'lerinden (ör. Oracle, Hetzner, AWS) YouTube girişsiz indirmeye izin
vermez: arama çalışır, ancak oynatma "Sign in to confirm you're not a bot" ile durur.
2026-10-29'da Oracle sunucusunda çerezsiz istemci ve önceki PO Token servisi
(bgutil 2.0.2) denendi; ikisi de bu hatayı aldı. Bu yüzden giriş yapılmış bir oturum
gerekir.

Google, aynı oturum hem kişisel tarayıcıda hem sunucuda kullanıldığında oturumu
birkaç saat içinde kapatır. Kapanan çerez dosyası, ilk istekten sonra giriş
çerezlerini kaybeder ve küçülür. Bunu önlemek için:

1. Bot için ayrı bir Google hesabı kullanın; kişisel hesabınızı kullanmayın.
2. Tarayıcıda gizli pencere açın ve yalnızca bu hesapla youtube.com'a girin.
3. Aynı pencerede çerezleri Netscape biçiminde dışa aktarın (ör. "Get cookies.txt LOCALLY").
4. Gizli pencereyi hemen kapatın; oturumu kapatmayın ve bu hesabı başka yerde açmayın.
5. Dosyayı Ayarlar → YouTube bölümünden yükleyin, ardından yerel kopyayı silin.

Oturum yine kapanırsa yalnızca aynı yöntemle yeni dosya yüklemek yeterlidir.

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
Datacenter IPs cannot download anonymously (cookieless and the former bgutil PO Token
provider were both refused on Oracle on 2026-10-29). Google closes a session that is
also used in a personal browser within hours, so export cookies for a dedicated bot
account from a private window, close that window without signing out, and never use
that account elsewhere.
Never commit, log or share cookies. Preserve the encrypted backups and data volumes.
Native deployments can use `YT_COOKIE_FILE`; panel uploads take precedence. Removing
a panel upload does not delete an operator-managed environment file.
