# Panel inceleme raporu — 20 Ekim 2026

## Düzeltilenler

- Regex yasaklarında ad kalıbı görünür. Sunucunun onayladığı silme işlemi satırı yenileme gerektirmeden kaldırır. Başarısız silme satırı kaldırmaz. Sonuç, işlemin başladığı bağlantının önbelleğine uygulanır.
- Bağlantıdan oynatma ekranı ve hata mesajları Türkçeleştirildi. Başarısızlık pencere içinde de görünür; düğme tekrar kullanılabilir. İndiricinin boş formatlarla sıfır çıkış kodu döndürdüğü durumda, önemsiz STDIN bildirimi yerine sağlayıcının gerçek doğrulama uyarısı korunur.
- Mesajlarda çevrimiçi kullanıcı adıyla seçim ve anlık özel sohbet eklendi. UID ile çevrimdışı posta ayrı bir gönderim türüdür. Query hesabının gelen kutusu, kullanıcıların gelen kutusu veya gönderilen mesajlar gibi gösterilmez. Başarı bildirimi okundu/teslim garantisi vermez.
- Mesaj ayrıntısı gerçek içerik uç noktasından alınır. Yazma işlemleri yönetici yetkisi gerektirir; mesaj gövdesi, konusu ve alıcının UID bilgisi denetim kaydına yazılmaz.
- Yönetici işlem kayıtlarında yönetici ve bağlantı adıyla filtreleme bulunur. İşlem türleri ve sonuçlar Türkçedir; satırlarda bağlantı adı gösterilir.
- Etkinlik kaydı kapalıyken eski yeniden bağlanma hatası gösterilmez. Açma/kapatma, gelecekteki giriş/çıkışları toplama ve Query bildirim yetkisi gereksinimi açıkça anlatılır. Geriye dönük kayıt üretilmez.
- Ekran metinleri, tablo sonuç sayıları, yasak süreleri ve göreli zamanlar için Türkçe düzeltmeleri yapıldı. Kullanıcı adları, kullanıcıların yazdığı açıklamalar ve ham günlükler otomatik çevrilmez.

## Düzenlenen menüler

- Sunucu günlükleri, yönetici işlem kayıtları, kullanıcı etkinlik geçmişi ve medya tanılama tek **Raporlar ve tanılama** merkezinde, ayrı türler olarak bulunur.
- Eski rapor adresleri yeni merkeze yönlenir; adresin sorgu parametreleri korunur. Menü aramasında eski günlük/tanılama terimleri de rapor merkezini bulur.
- Ayrı **Instance** sayfası yerine **TeamSpeak sunucuları → Sunucu hizmeti bilgileri** kullanılır.
- Yeni sunucuların grup şablonları **Yetkiler → Yeni sunucu grup şablonları** bölümüne taşındı. Bunlar mevcut kullanıcıların yetkileri değildir.
- Medya tanılamasında araç durumu Türkçe gösterilir. Sürüm ve ayrıntılı program çıktısı, özgün dilinde açılabilir teknik ayrıntılardadır. Hazır araçlar, YouTube erişiminin çalıştığı anlamına gelmez.

## Günlük türlerinin farkı

**TeamSpeak sunucu günlüğü:** Seçilen tek TeamSpeak sunucusunun olaylarıdır.

**Sunucu hizmeti günlüğü:** Aynı bağlantıdaki bütün sunucuları çalıştıran sürecin olaylarıdır. Tek sunucuyla sınırlı değildir; işletim sistemi systemd günlüğüyle aynı şey değildir.

**Yönetici işlem kaydı:** Manager üzerinden hangi hesabın hangi işlemi yaptığıdır. TeamSpeak istemcisinden doğrudan yapılan her işlemi kapsamaz.

**Etkinlik geçmişi:** Etkinleştirildikten sonraki kullanıcı giriş ve çıkışlarıdır. Sohbet mesajı ve IP adresi saklanmaz.

## Canlı doğrulama — 21 Ekim 2026

- YouTube artık yalnızca yönetici tarafından yüklenen çerezle çalışır; çerezsiz sağlayıcı kaldırıldı. Çerez yoksa istek başlamadan açık bir hata gösterilir. Canlı sunucuda geçerli çerezle sesli oynatma ayrıca doğrulanmalıdır; çerezler ağ veya hesap kısıtlamalarını her koşulda aşamaz.
- İleriye dönük `notifytokenused` takibi eklendi. Gerçek, izole bir test anahtarı tüketildi; kullanım olayı kalıcı kayda ve paneldeki kullanım geçmişine ulaştı. Manager yeniden başladıktan sonra kayıt korundu. Ham anahtar saklanmaz; anahtar izi, kullanıcı adı, UID ve veritabanı kimliği tutulur. Saklama sınırı sunucu başına 30 gün / 200 kayıttır. Takip panelden açılıp kapatılır. Etkinleştirmeden önceki bilinmeyen kullanımlar geriye dönük uydurulmaz.
- Gerçek bir geçici TeamSpeak ses istemcisiyle anlık özel mesajın alındığı doğrulandı. Query kabulü ile alıcıdaki gerçek `notifytextmessage` olayı ayrı ayrı kontrol edildi. Çevrimdışı posta ayrı tutulur; istemcilerin bu özelliği göstermesi kendi desteklerine bağlıdır.
- İstenen GPU kodlayıcısı çalışmıyorsa testten geçmiş yazılım kodlayıcısına geçiş eklendi. Canlı sistemde NVENC isteği H.264 yazılım kodlayıcısına geçti ve FFmpeg ile gerçek kısa kodlama testi başarılı oldu.
- Mevcut bağlantıdaki eksik Query SSH kimlik bilgileri, kurulumdaki yönetici bilgileriyle doğrulanarak şifreli biçimde eşleştirildi. Yoğun isteklerdeki Query kopmaları için yalnızca Manager'ın iç Docker IP'sine flood muafiyeti eklendi; genel koruma kapatılmadı. TeamSpeak listeyi kendiliğinden yeniden okudu ve Query bağlantısı sunucu yeniden başlatılmadan toparlandı. Aynı eşleme kurulum/güncelleme akışına eklendi; açık deny kuralları korunur.

## Doğrulama ve yayın durumu

Ön yüz ve arka uç TypeScript kontrolleri ile üretim derlemesi geçti. 232 ön yüz birim testi, 19 hedefli arka uç testi ve 16 farklı Playwright tarayıcı testi geçti. Masaüstünde dolu yönetici raporu ve mesaj pencereleri; mobilde etkinlik raporu ayrıca görsel olarak incelendi.

Değişiklikler `manager-overlay` kaynaklarına aktarıldı ve canlı Manager'a `0.3.8-alpha.3+review.20261021` inceleme derlemesi olarak kuruldu. Önce şifreli tutarlı yedekler alındı. Manager root olmayan kullanıcıyla çalışmaya devam ediyor; TeamSpeak sunucusunun konteyner kimliği ve başlangıç zamanı değişmedi. GitHub push/yeni release yapılmadı. Son eklemelerde 26 hedefli arka uç testi, 11 hedefli tarayıcı testi, 232 ön yüz birim testi ve Windows/Linux üzerinde Query allowlist testleri geçti.
