# Panel, bot ve konuşma sentezi

Teamspeak Managment alpha sürümüdür. Güncellemeden önce şifreli yedek alın;
anahtarını yedekten ayrı tutun. GitHub'a push, çalışan sunucuyu güncellemez.

## Arayüz

Panel özgün altıgen ses dalgası logosunu kullanır. TR/EN seçimi kalıcıdır;
vurgu rengi ve arka plan tercihleri Ayarlar > Görünüm bölümündedir.
Widget oluşturma, izin sekmeleri ve bot özetleri Türkçeleştirilmiştir.
HTTP üzerinde ayrıcalık anahtarı kopyalama için Clipboard API bulunmadığında
uyumlu kopyalama yolu kullanılır. Tarayıcı engellerse hata gösterilir.

## Sunucu ve bot adresleri

TeamSpeak istemcisine erişilebilir IP/alan adı ve ses portunu yazın:
`sunucu.example:9988`. `0.0.0.0` dinleme adresidir, hedef adres değildir.
`127.0.0.1` her zaman bağlandığınız bilgisayarın kendisidir.
Manager yalnızca loopback üzerinde açıksa HTTPS ters proxy veya SSH tüneli
kullanın: `ssh -L 3000:127.0.0.1:3000 kullanici@SUNUCU_IP`.

Bot oluştururken ses portu `0`, seçilen sanal sunucunun portunu Query üzerinden
bulur. NAT farklı bir dış port gerektiriyorsa portu açıkça girin.
Botun kayıtlı olması bağlı olduğu anlamına gelmez. Panelde durum, hedef adres,
istemci kimliği ve bağlantı hatasını kontrol edin. Bot ayrı bir IP gerektirmez.

Kurulum özeti Manager kullanıcı adını ve gizli bilgilerin konumunu gösterir.
Parola özetinden düz metin parola geri alınamaz. İsteğe bağlı TeamSpeak UID,
ses istemcisine Server Admin atamak içindir; ServerQuery hesabı değildir.
Saklanan ilk yönetici ayrıcalık anahtarını başkalarıyla paylaşmayın.

## İsteğe bağlı TTS

Manager kurulumu TTS paketini sorar. Sonradan terminal menüsündeki **14**
seçeneğiyle imajı yeniden oluşturabilirsiniz. Mevcut verileri koruyan güncelleme
akışı kullanılır. Panel yöneticisi **Bot merkezi → ilgili botun ayarları →
Eklentiler ve komutlar → Sesli okuma (TTS)** alanında etkinleştirir;
dil, ses seviyesi, metin sınırı ve bekleme süresini o bot için değiştirir.
Genel Ayarlar ekranında TTS sekmesi bulunmaz. Eski ortak ayarlar, bot için
ayrı bir ayar kaydedilene kadar okunur; diğer botların değerleri değiştirilmez.

**Komutlar** sekmesindeki anahtarlar yalnızca seçili botu etkiler ve komut
işlenirken sunucuda da uygulanır. Takma adlar aynı anahtarı paylaşır.
**Özel komut kitaplığı** ve **Eklenti paketleri** ise aynı TeamSpeak bağlantısı
için ortaktır; bu ekranlarda yapılan tanım/kurulum değişiklikleri bağlantıdaki
diğer botları da etkileyebilir. Bir komutu yalnızca tek botta kapatmak için
Komutlar sekmesini kullanın. Manifest paketleri metin komutlarıdır; keyfî
JavaScript/Python çalıştıran üçüncü taraf eklentiler desteklenmez.

```text
!say Merhaba, sunucuya hoş geldiniz.
!say --tr Türkçe konuşma örneği.
!say --en Welcome to our server.
```

Piper yerel çalışır. Modeller kurulumda indirilir, sabit commit ve SHA-256 ile
doğrulanır; model kartları korunur. TR/EN modelleri yaklaşık 130 MB, çalışma
ortamı ayrıca yer kaplar. Motor GPL-3.0 lisanslıdır; model lisanslarını kendi
MODEL_CARD dosyalarından inceleyin. Harici TTS API anahtarı gerekmez.

`!say` mevcut oynatma yetkilerine tabidir. Metin, eşzamanlı iş, süre ve dosya
boyutu sınırları uygulanır; metin shell komutu olarak çalıştırılmaz. Konuşma
metni komut günlüğünde gizlenir. Müzik çalarken mevcut yayını kesmez.

## Doğrulama sınırı

Yerel UI ve birim testleri gerçek bir TS ses oturumunun yerini tutmaz.
İsteğe bağlı motorun amd64/ARM64 imaj testleri ve güvenlik taramaları CI'da
çalışır. Başarılı sonuç görülmeden sürümü üretim için doğrulanmış saymayın.
Hiçbir sürüm için sıfır güvenlik açığı garantisi verilmez.

Kaynak: https://github.com/iNUIZ/teamspeak-managment

Wiki: https://nuiz.mintlify.site/index

## Panel kullanım kolaylığı ve şarkı profili

YouTube için Ayarlar → YouTube bölümünden cookies.txt yüklenir. Dosya yeni
işlemlerde otomatik kullanılır; çerezsiz alternatif hizmet bulunmaz.
Platform gereksinimleri ve sınırlamalar: [Çerezle oynatma](youtube-cookies.md).

Kontrol panelindeki hızlı işlemlerle kanallara, kullanıcılara, bot merkezine ve
yöneticiyseniz bağlantılara ulaşabilirsiniz. Yan menüde Türkçe veya İngilizce
sayfa adıyla arama yapılabilir; Escape aramayı temizler.

Bot merkezi → bot menüsü → Botu düzenle → Avatar → **Çalan şarkının kapağı**,
TeamSpeak içindeki otomatik şarkı profilini etkinleştirir. Açıklama ve kapak
ilk ses paketi gönderilince güncellenir. Kapak yoksa varsayılan görsel kullanılır;
özel yüklediğiniz dosya korunur. Varsayılan veya Yok seçimiyle otomatik modu
kapatabilirsiniz. Sunucunun bot grubunda avatar yükleme ve açıklama değiştirme
yetkileri bulunmalıdır. Web panelindeki önizleme dinamik kapağı göstermez.

Ayrıntılı kapsam ve sınırlamalar: [Panel UX raporu](panel-ux-review.md).
