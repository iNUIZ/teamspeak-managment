# Panel kullanılabilirlik ve test raporu

## Yapılan değişiklikler

- Tasarım, logo, mevcut renkler ve arka plan seçenekleri korunur.
- Türkçe arayüzde “Sanal sunucular” yerine “TeamSpeak sunucuları”; ilgili
  kimlik alanlarında “TeamSpeak sunucu kimliği” kullanılır. Protokol alanlarının
  adları, mevcut veritabanı ve bağlantı kimlikleri değiştirilmez.
- Menü Türkçe veya İngilizce sayfa adıyla aranabilir. Arama sırasında eşleşen
  gruplar açılır; temizleme düğmesi ve Escape ile normal menüye dönülür.
  Sonuç bulunmadığında açıklama gösterilir. Daraltılmış menü aramadan etkilenmez.
- Dashboard'a Kanallar, Kullanıcılar, Bot merkezi ve Bağlantılar hızlı erişimi
  eklenir. Yönetici bağlantıları diğer kullanıcılara gösterilmez.
- Mobil/tablet üst çubuğu iki satıra ayrılır; dil/hesap düğmeleri sunucu
  seçicilerinin metnini birkaç piksele sıkıştıramaz. Masaüstü tek satır kalır.
- Sunucu listesinin açıklaması, çevrimiçi durumu, başlat/durdur düğmeleri ve
  durdurma onayı Türkçeleştirilir. Durdurmanın kullanıcıların bağlantısını
  keseceği belirtilir. Boş liste ayrıca açıklanır.
- Görünüm ayarları, mobil menü ve hesap menüsündeki erişilebilir etiketler
  Türkçeleştirilir. İngilizce seçimde İngilizce etiketler korunur.
- Bot düzenleme → Avatar bölümüne isteğe bağlı **Çalan şarkının kapağı** modu
  eklenir. İlk ses paketi gönderildiğinde TeamSpeak açıklamasına şarkı, sanatçı
  ve süre yazılır; YouTube kaynağında kapak kullanılır. Boşta varsayılan görsele
  dönülür. Özel yüklenen avatar dosyası silinmez; otomatik moddan çıkılabilir.
- Kapak alınamazsa varsayılan görsel kullanılır; kapak yükleme müziği bekletmez.
  Yerel dosya/radyo için var olmayan bir kapak uydurulmaz. Paneldeki avatar
  önizlemesi varsayılan görseli göstermeye devam eder; dinamik kapak TeamSpeak
  istemcisine uygulanır.
- `!ara` listesinde süre gösterilir. Sıralama ses kalitesinin ölçümü olarak
  sunulmaz; başlık, sanatçı ve sürüm uyumu kullanılır. YouTube arama eşleşmeleri
  Spotify'dan doğrudan ses akışı olarak etiketlenmez.
- Numara ile seçim, `!play` ile aynı yetki, flood ve yanıt kanalı kontrollerinden
  geçer. “Seçildi” yerine “Hazırlanıyor” denir; hazırlama başarısız olduğunda
  sohbette hata verilir. Çerez yolu veya proxy bilgisi sohbete yazılmaz.
- Bir YouTube istemcisindeki bot doğrulama hatası sonraki boş yanıtta kaybolmaz.
- TeamSpeak'in `error id=0 msg=ok` başarı yanıtı artık son hata olarak kaydedilmez;
  mevcut gerçek hata da başarı yanıtıyla örtülmez.

## Ne kaldırıldı?

Çalışan bir özellik, sunucu, bot, müzik, çerez veya kullanıcı verisi silinmedi.
Kafa karıştıran Türkçe terimler, başarıyı hata gibi gösteren bildirim ve
YouTube eşleşmesini Spotify sesi gibi sunan kaynak etiketi düzeltildi.
Gelişmiş yönetim araçları mevcut katlanabilir menü gruplarında tutuldu.

## Güvenlik sınırları

Otomatik kapak yalnızca doğrulanmış YouTube video kimliğinden sabit
`https://i.ytimg.com/vi/<id>/mqdefault.jpg` adresiyle oluşturulur. Başka URL ve
yönlendirmeler kabul edilmez; DNS sonuçları özel/rezerve IP'lere karşı kontrol
edilir ve kontrol edilen adrese bağlanılır. En fazla 200 KB görsel, sekiz saniye
zaman sınırı ve iptal desteği vardır. Açıklama UTF-8 karakterleri bölmeden 200
baytla sınırlanır; BBCode köşeli parantezleri ve kontrol karakterleri temizlenir.
Yeni paket veya dışarıya açık servis eklenmedi.

## Doğrulama kapsamı

Test planı: [UX-QA](../tests/UX-QA.md). Yerel üretim derlemesi ve kontrollü API
fixture'ları ile tarayıcı akışları sınanır. Gerçek kullanıcı girdileriyle ayrıca
Playwright Interactive masaüstü/mobil, menü arama, dil ve renk geçişleri incelenir.

Canlı panele giriş ve dashboard/bot merkezi salt okunur incelendi. Canlı
sunucuları durdurma, bot silme, kanal taşıma veya mevcut ayarları değiştirme
işlemleri yapılmadı; bunlar yerel test sunucusunda sınanır.

Bu rapor tüm dış hizmetlerin ve tüm TeamSpeak sürümlerinin sorunsuz olduğunu
iddia etmez. Gerçek sesin duyulması, yeni avatar modunun TS3/TS6 istemcisinde
uçtan uca uygulanması, TeamSpeak dosya aktarım yetkileri, gerçek Linux TTS
çıktısı ve yeni konteyner imajının güvenlik taraması ayrıca doğrulanmalıdır.
YouTube'un sunucu IP'sine uyguladığı doğrulama engeli bu UX değişiklikleriyle
çözülmüş sayılmaz; cookiesiz oynatma başarıyla doğrulanmadı.

Değişiklikler canlı sunucuya otomatik kurulmaz. Bu çalışma veri silme, sürüm
etiketi taşıma veya eski yayınları kaldırma işlemi içermez.

## Yerel test sonuçları

- Ön yüz: 230 birim testi başarılı; ön yüz/arka uç TypeScript kontrolleri ve
  Vite üretim derlemesi başarılı. Mevcut ana JavaScript paketi için 500 KB
  paket boyutu uyarısı sürüyor; performans ölçümü yapılmış sayılmaz.
- Arka uç: 90 hedefli test ve bağlantı grubundaki 18 test başarılı. Avatar
  aktarımı, iptal, şarkı profili, UTF-8 sınırı, komut yetkileri, flood,
  kuyruk ve başarı yanıtı regresyonları kapsanır.
- Kurucu: Bash sözdizimi ve iki kurulum sırası/otomatik bağlantı testleri başarılı.
- Linux shebang kullanan `youtube.progress.test.ts` Windows'ta ENOENT ile
  çalışmadı; Linux üzerinde indirme testi bu çalışmada doğrulanmadı.
- Yeniden bağlanma testindeki zamanlayıcı sırası düzeltildi: asenkron
  bağlantı-kesme tamamlandıktan sonra yeniden-bağlanma zamanlayıcısı ilerletilir.
- Tarayıcı envanterinde 306 senaryo bulunur. İlk geniş koşumun bulguları
  düzeltmelerden sonra hedefli tekrar koşumlarıyla kontrol edilir; tek bir
  kesintisiz koşumun tamamının yeşil olduğu iddia edilmez.
  Son derlemede bot konsolu, mobil üst çubuk, yan menü ve TR/EN menü aramasını
  kapsayan son 37 testin tamamı geçti. Mevcut koşulu nedeniyle atlanan tek
  dokümantasyon ekran görüntüsü senaryosu başarılı sayılmadı.
- İlk taramadaki eski marka adı/logo biçimi beklentileri güncellendi. SVG
  logo renk kontrolü arka plan yerine çizimin devraldığı rengi ölçer. Animasyonlu
  sayfalarda iki sütunun geometrisi aynı karede okunur.
- PWA manifesti/ikonları kontrol edilir. Chrome'un gizli test bağlamında verdiği
  `in-incognito` kısıtı uygulama hatası sayılmaz; diğer kurulum hataları testten
  süzülmez. İşletim sistemine gerçek PWA kurulumu yapılmadı.
- Playwright Interactive ile normal giriş, TR/EN ve renk seçimi/geri alma,
  Türkçe/İngilizce menü araması, boş sonuç, temizleme ve sunucu sayfasına geçiş
  ayrıca uygulandı. 1366×768 masaüstü ve 375×844 mobil son görünümleri incelendi.
  Mobil sunucu seçicilerinde metin alanları 134/152 px; menü arama temizleme
  hedefi 44×44 px. Dar masaüstünde tüm grup başlıkları kaydırmadan görünür.
- Kontrol edilen son görünümlerde yatay sayfa taşması, gizlenen ana işlem veya
  okunamayan menü başlığı görülmedi. Bu değerlendirme bütün muhtemel tema,
  özel CSS veya kullanıcı verisi kombinasyonlarını kapsamaz.
