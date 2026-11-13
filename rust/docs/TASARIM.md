# TeamHub — Rust tasarımı

Projenin tamamı (sunucu, ses botu, görüntü yayını, bot akışları ve web paneli) sıfırdan Rust ile yazılır.
Bu belge neyin nasıl yazılacağını ve mevcut sistemden neyin korunacağını anlatır.

## 1. Temel kararlar

| Konu | Karar | Neden |
|---|---|---|
| Çalışma biçimi | Tek program: `teamhub` | Node + nginx + Go sidecar + supervisor dörtlüsü kalkar; tek çekirdekli sunucu için en büyük kazanç budur |
| Eşzamanlılık | `tokio` | Ses, TS bağlantıları ve API aynı programda çalışır |
| API | `axum` + `tower-http` | WebSocket ve statik dosya dahil tek sunucu |
| Veritabanı | SQLite + `sqlx` | Prisma kalkar; mevcut veritabanı dosyası yerinde açılır |
| Web paneli | `leptos` (WebAssembly, tarayıcıda çalışır) | Tüm proje Rust; paylaşılan tipler sunucuyla aynı |
| Görünüm | Tailwind CSS (mevcut sınıflar) | Görünüm aynı kalır; CSS üretimi Rust dışında bir araçla yapılır |
| TS protokolü | Kendi `ts-proto` crate'imiz | `tsclientlib` 2021'den beri güncellenmiyor; TS6 yayın komutları ve canlıda yaşanan düzeltmeler bizde |
| Görüntü yayını | `str0m` ile WebRTC | Go/pion sidecar'ı yerine programın içinde çalışır |

### Rust'a geçmeyecek dış programlar

Bunlar kütüphane değil, ayrı araçlardır; Rust sürümü de bunları çağırır:

- `ffmpeg`, `ffprobe`: çözme ve kodlama.
- `yt-dlp` ve `deno`: YouTube.
- `piper`: isteğe bağlı TTS (Python).

Opus kodlama `opus` crate'i ile libopus (C) üzerinden yapılır. Bugünkü `opusscript` (WASM) yerine yerel kod çalışır; ses botunun işlemci kazancının bir kısmı buradan gelir. Kazancın ne kadar olduğu ölçülmeden söylenmez (bkz. bölüm 8).

## 2. Mevcut sistemin büyüklüğü

Test dosyaları hariç satır sayıları:

| Parça | Satır | Rust'taki yeri |
|---|---:|---|
| `voice/` (toplam) | 22.310 | aşağıdaki satırlara bölünür |
| `voice/tslib` | 2.537 | `ts-proto` |
| `voice/audio` + `egress` + `playlist` | 6.412 | `media` |
| `voice/commands` + bot kökü | 10.653 | `voice` |
| `voice/streaming` + `sidecar` (Go, 5.361) | 8.069 | `stream` |
| `routes` + `middleware` + `widget` + `ws` | 11.035 | `server` |
| `bot-engine` | 4.643 | `flow` |
| `ts-client` | 2.117 | `ts-query` |
| `audit` + `activity-journal` + `iptv` + `utils` + `bootstrap` | 5.051 | `core`, `server` |
| `common` | 2.535 | `shared` |
| `frontend` | 35.150 | `web` |

Testler: 188 backend, 44 Playwright dosyası.

## 3. Crate yapısı

```text
rust/
  crates/
    shared/    Sunucu ve panelin ortak tipleri (WASM'da da derlenir)
    core/      Ayarlar, alan şifreleme, hata tipleri, denetim kaydı
    db/        sqlx sorguları, migration'lar, eski veritabanını devralma
    ts-proto/  TS istemci protokolü: UDP, şifreleme, kimlik, QuickLZ, komutlar
    ts-query/  WebQuery istemcisi, SSH query (russh), bağlantı havuzu, demo istemci
    media/     ffmpeg/yt-dlp süreçleri, Opus, önbellek, ICY, Spotify/Apple, egress proxy, TTS
    voice/     VoiceBot, bot yöneticisi, sohbet komutları, Auto-DJ, kuyruk
    stream/    TS6 yayın sinyalleşmesi, WebRTC (str0m), kalite ayarı
    flow/      Bot akış motoru, tetikleyiciler, zamanlayıcı, ifade değerlendirme
    server/    axum API, WebSocket, yetki, hız sınırı, widget; `teamhub` programı
    web/       Leptos paneli (yalnızca WASM)
```

Bağımlılık yönü tek taraflıdır: `server → flow → voice → stream, media, ts-proto`; `server → ts-query, db, core`; `web → shared`. Panel, sunucu crate'lerini hiçbir zaman içe aktarmaz.

### Ana kütüphaneler

| İş | Crate |
|---|---|
| Çalışma zamanı, ağ | `tokio`, `reqwest`, `tracing` |
| API | `axum`, `tower-http` |
| Veritabanı | `sqlx` (sqlite) |
| TS şifreleme | `aes`, `cmac`, `eax`, `p256`, `curve25519-dalek`, `ed25519-dalek` |
| SSH query | `russh` |
| Parola, oturum | `bcrypt`, `jsonwebtoken` |
| Ses, görüntü | `opus`, `str0m` |
| Panel | `leptos`, `leptos_router` |
| Panel dosyalarını programa gömme | `rust-embed` |
| Widget görseli | `resvg` |
| Akışlar | `cron`, `evalexpr` |

## 4. Ses botu

Her bot bağımsız bir görev (task) olarak çalışır; dışarıyla kanallar üzerinden konuşur.

```text
              komutlar (mpsc)              durum (watch)
API / sohbet ───────────────► VoiceBot ───────────────► API / WebSocket
                                 │
                 ┌───────────────┼────────────────┐
                 ▼               ▼                ▼
             TsClient       AudioPipeline     StreamSession
          (UDP + yeniden   (ffmpeg → PCM →    (WebRTC, yalnızca
            gönderim)       Opus, 20 ms)       yayın açıkken)
```

- **Durum makinesi:** `stopped`, `starting`, `connected`, `playing`, `paused`, `error` bugünkü adlarıyla bir `enum` olur. 3.151 satırlık `voice-bot.ts` sorumluluklarına bölünür: bağlantı, oynatma, kuyruk, isim/avatar.
- **Ses saati:** Tek görev 20 ms'de bir paket gönderir. Bir yavaş işlem başka botun sesini kesmez, çünkü ağ ve süreç işleri ayrı görevlerdedir.
- **Kimlik üretimi:** Seviye 23 hesaplaması `spawn_blocking` ile çalışır, ses görevini bekletmez.

### Kaybedilmemesi gereken davranışlar

Bunlar canlıda yaşanıp düzeltildi. Her biri Rust'ta da test olarak yazılır:

- Tek paketli komut sınırı `MAX_TS_COMMAND_BYTES = 487`; uzun metin satır sonundan bölünür.
- Sonda tek `\` veya bilinmeyen kaçış karakteri bağlantıyı düşürmez; okunamayan komut atlanır.
- Bilinmeyen `notify` komutları görmezden gelinir.
- 1538 ve 2568 hata döngülerine karşı düzeltmeler, flood bekletmesi (524).
- Şarkı çalarken isim tamamen `♪ Şarkı` olur; 3–30 karakter sınırı ve kısaltma.
- `!here` gönderenin bulunduğu kanalı önceler; kanal ve istemci haritaları olaylarla güncel tutulur.
- Arama: YouTube Music yok; 20 dakika üstü ve geçersiz kimlikli sonuçlar elenir; başlık temizleme ve tekilleştirme.
- Dış sunucuya (`externalHost`) bağlanan botta query özellikleri kapalıdır.
- Hazır "Müzik Botu" ve "Radyo Botu" bağlantı başına bir kez oluşturulur.

## 5. Mevcut sistemle uyum

Geçişte hesaplar, botlar ve şarkılar kaybolmaz:

- **Veritabanı:** 25 tablonun tamamı aynı adlarla kalır. Program yeni dosyada şemayı kurar; var olan dosyada şemayı karşılaştırır, uyumsuzsa dosyaya dokunmadan durur. Veritabanına migration tablosu eklenmez: eski program açılışta `prisma db push` çalıştırıp tanımadığı tablolara takılır, bu da eski imaja dönüşü bozardı. Kolon sırası önemsenmez, çünkü eski programın sonradan eklediği kolonlar tablonun sonundadır. Tarihler milisaniye sayısı, doğru/yanlış değerleri 0/1 olarak aynı biçimde yazılır.
- **Şifreli alanlar:** Aynı biçim korunur: AES-256-GCM, anahtar `scrypt(ENCRYPTION_KEY, "ts6-webui-enc-v1")`, değer `enc:iv:tag:veri`. Çözülebilirliği eski programın ürettiği örneklerle test edilir.
- **Parolalar ve oturumlar:** bcrypt özetleri olduğu gibi geçerlidir. JWT sırrı aynı kalır (15 dakika / 7 gün).
- **Bot kimlikleri:** Aynı kimlik verisi okunur; botlar TS'de yeni kullanıcı gibi görünmez.
- **API:** Yollar ve JSON alanları değişmez. Eski panel Rust sunucuya, yeni panel eski sunucuya bağlanabilir; böylece iki taraf ayrı ayrı denenir.
- **Ayar ve volume'lar:** `.env`, `/data/music` ve veritabanı yolları aynıdır.

## 6. Web paneli

- `leptos` istemci tarafında (CSR) çalışır; `trunk` ile derlenir, çıktı `rust-embed` ile programa gömülür. nginx gerekmez.
- Mevcut Türkçe katalogları (`tr-extra`, `tr-panel`, `tr-operations`) ve Tailwind sınıfları taşınır.
- Rust'ta hazır karşılığı olmayan parçalar kendimiz yazılır:

| React parçası | Rust'ta |
|---|---|
| Radix (dialog, select, tabs, tooltip…) | Erişilebilirlik kurallarıyla kendi bileşen kitaplığımız (`web/ui`) |
| TanStack Query / Table | Leptos `Resource` + küçük tablo bileşeni |
| recharts | SVG ile küçük grafik bileşeni |
| dnd-kit | Pointer olaylarıyla sürükle-bırak |
| Akış düzenleyici (`BotEditor`, 1.710 satır) | Aynı SVG tuval, en son taşınır |
| PWA (workbox) | Mevcut service worker küçük bir JS dosyası olarak kalır |

**Kabul testi:** 44 Playwright dosyası tarayıcıdan çalıştığı için dilden bağımsızdır; aynı testler Rust paneline karşı çalışır. Panel bu testleri geçince tamamdır.

## 7. Dağıtım

- Derleme CI'da yapılır: `aarch64` ve `x86_64`, tek Docker imajı. Tek çekirdekli sunucuda hiçbir şey derlenmez.
- İmaj: küçük taban + `teamhub` + `ffmpeg`, `yt-dlp`, `deno`.
- `install.sh` aynı kalır, yalnızca yeni imajı kullanır.
- Canlıya geçiş: eski imaj silinmez; sorun olursa `.env` içindeki imaj adı değiştirilerek eskisine dönülür. Veritabanı şeması değişmediği için bu dönüş mümkündür.

## 8. Yazım sırası

Her adımın bitiş şartı testtir; bir adım bitmeden sonrakine geçilmez.

| # | Adım | Bitiş şartı |
|---|---|---|
| 0 | İskelet | Tüm crate'ler derlenir, `teamhub` `/api/health` verir, CI çalışır |
| 1 | `ts-proto` | Eski koddan alınan örnek paketlerle uyumlu; yerel TS6 sunucusuna bağlanıp kanalda görünür |
| 2 | `core`, `db` | Canlı veritabanının bir kopyası açılır, şifreli alanlar çözülür |
| 3 | `media` + `voice` çekirdeği | Bot radyo ve YouTube çalar, kulakla doğrulanır; işlemci ve bellek eskisiyle ölçülüp karşılaştırılır |
| 4 | Sohbet komutları, Auto-DJ | Komut testleri geçer |
| 5 | `ts-query`, `server` | Eski panel Rust sunucuyla tam çalışır; Playwright geçer |
| 6 | `flow` | 20 hazır şablon çalışır |
| 7 | `stream` | Yayın TS6 istemcisinde görünür; `str0m` beklenen özellikleri karşılamazsa bu adımda karar yeniden gözden geçirilir |
| 8 | `web` | Playwright Rust paneline karşı geçer |
| 9 | Canlıya geçiş | Eski imaj hazırda bekler, kontrol testi 5/5 |

### Durum

- **Adım 0:** Kod hazır. `.github/workflows/rust.yml` yazıldı, GitHub'da henüz çalışmadı.
- **Adım 1:** Bitti. `ts-proto` eski kodun ürettiği örneklerle bayt bayt uyumlu (`tests/vectors.json`) ve canlı TS6 sunucusuna bağlanıp kanalda göründü. Ses gönderimi kodda var; gerçek ses adım 3'te doğrulanacak.

- **Adım 2:** Bitti. Canlı veritabanının bir kopyası salt okunur açıldı: şema uyuştu, 367 satırın tamamı okundu, şifreli alanlar çözüldü, iki botun kimliği aynı çıktı, admin parolası doğrulandı. Yeniden denemek için: `crates/db/examples/verify_live.rs`. Hazır sorgular şimdilik azdır; geri kalanı ilgili adımda eklenir.

- **Adım 3:** Kod ve ölçüm bitti. `media` ve `voice` çekirdeği canlı TS6 sunucusunda denendi; ikinci bir Rust istemcisi (`crates/voice/examples/listen.rs`) sesi sunucudan alıp ölçtü:

  | Deneme | Sonuç |
  |---|---|
  | 440 Hz test sesi, 20 sn | 1000 paket, saniyede 50, bozuk paket yok, gelen sesin tamamı 440 Hz |
  | Radyo (SomaFM), 19 sn | Saniyede 50 paket, 100 ms üstü boşluk yok; şarkı adı ve şarkı değişimi bot ismine yansıdı |
  | YouTube, ilk çalma | Çözülen adres 403 verdi, dosya indirildi, ses yaklaşık 15 saniyede başladı; ileri sarma çalıştı |
  | YouTube, ikinci çalma | Önbellekten hemen başladı |
  | Duraklat, devam, sar, ses | Hepsi çalıştı |

  İşlemci ve bellek, aynı dosya aynı sunucuda 65 saniye, iki tur (ffmpeg hariç, Windows x86_64):

  | | İşlemci (tek çekirdek) | Bellek (ortalama) |
  |---|---:|---:|
  | Eski bot | %11,2 – %11,8 | 75 MB |
  | Rust bot | %2,33 – %2,35 | 8 MB |

  Bu ölçüm yalnızca ses botunu karşılaştırır: API ve panel iki tarafta da yoktur. Eski bot `tsx` ile çalıştığı için belleğinin bir kısmı derleyicidendir. Sunucunun kendisi `aarch64` olduğundan oradaki oran farklı çıkabilir. İşlemci, çevrim sayacından (`QueryProcessCycleTime`) okundu: Windows'un süre sayacı 20 ms'lik kısa işleri sayamıyor ve Rust için yanlış olarak %0 gösteriyor.

  Henüz yazılmayanlar: Medya çıkış proxy'si (`egress`, canlıya geçmeden önce şart), Spotify ve Apple Music, arama, oynatma listesi, TTS, indirme ilerlemesi, yeniden bağlanma, boş kanalda durma, avatar. Bunlar kullanıldıkları adımda eklenir. `address.rs` içindeki kurallar proxy için hazırdır.

- **Adım 4:** Komutlar ve Auto-DJ yazıldı, testleri geçiyor ve canlı TS6 sunucusunda ikinci bir istemci kanala yazarak denedi (`crates/voice/examples/chat_live.rs`): `!np`, `!radio`, `!play`, `!seek`, `!queue`, `!history`, `!lyrics`, `!pause`, `!vol`, `!stop`, Auto-DJ'in sessizlik süresinden sonra radyoyu açması. Sohbet komutları `BotControl`, `Tracks` ve `ServerQuery` arayüzleri arkasındadır; böylece her komut gerçek sunucu olmadan test edilir.

  Spotify ve Apple Music bağlantıları da taşındı (`crates/media/src/links.rs`); gerçek şarkı, albüm ve çalma listesi bağlantılarıyla denendi. Şarkının kendisini YouTube'dan bulma kısmı testlerde sahte kaynakla, YouTube indirmesi ise ayrı olarak canlıda denendi; ikisi art arda canlıda çalıştırılmadı.

  Bu adımda bilerek kalanlar: Kanal dışından gelen komutlar ve SSH yardımcısı (`channel-ownership`), kimlik sorgusu ve `!voteskip` için gerçek `ServerQuery` ile dinleyici uzaktan kumandası `!remote` 5. adımda; `!stream`, `!tv` ve `!viewers` 7. adımda; `!say` (TTS) panel ayarlarıyla birlikte. Bunlar şimdilik kanala "not available yet" der. Kısıtlı komutlar `ServerQuery` bağlanana kadar kapalı kalır.

  Açık sorun: Uzun cevaplar (`!help` 7 parça, `!lyrics` 8 parça) TS6'nın sel korumasına (524) takılabilir; bot o zaman 30 saniye susar ve sonra haber verir. Parçalama eski programla aynıdır. Gönderimi aralıklı yapmak ayrıca değerlendirilecek.

- **Adım 5:** Başladı. `ts-query` içinde adres denetimi ve WebQuery istemcisi yazıldı: tek bağlantı, öncelikli sıra, komutlar arası 350 ms, sel bekletmesi, kopan bağlantıda tek yeniden deneme. Sahte sunucuyla test edildi, gerçek TS6'ya karşı henüz denenmedi. `server` tarafında hata biçimleri, oturum (eski JWT ve bcrypt geçerli), hız sınırı, `TRUST_PROXY`, denetim kaydı ile kurulum, oturum ve kullanıcı yolları yazıldı; sırada kalan yollar var.

  Yazılan yollar: sunucu bağlantıları (kayıt işleri), sanal sunucular, kanallar, istemciler, sunucu ve kanal grupları, izinler, yasaklar, yetki anahtarları, şikayetler, mesajlar, örnek bilgisi. Sahte WebQuery sunucusuyla test edildi; gerçek TS6'ya karşı denenmedi.

  Müzik botu yönetimi de yazıldı: bot yöneticisi (`voice/src/manager.rs`), müzik botu, radyo, özel komut ve istek geçmişi yolları, kim ne yaptı kaydı (`voice/src/activity.rs`), yeni bağlantıya hazır gelen 10 radyo ve 2 bot (`server/src/bootstrap.rs`). Gerçek program boş bir veritabanıyla çalıştırılıp bu yollarla canlı TS6 sunucusunda denendi: bot bağlandı, radyo çaldı, şarkı adı isme yansıdı, ses, duraklatma, geçmişten tekrar çalma, arama, durdurma ve silme çalıştı; bot durunca sunucudan ayrıldı. Yeniden denemek için ayrı bir araç henüz depoda yok.

  Program artık Docker'ın gönderdiği durdurma sinyalini de dinler ve botları sunucudan düzgünce çıkarır.

  Bu yollarda bilerek eksik kalanlar: avatar ve TTS ayarları ile video yayını yolları yok (yayın durumu "yayın yok" der), panel canlı güncellemesi (WebSocket) yok. Avatarlar taşınana kadar hazır botlar avatarsız oluşur.

  5. adımda kalanlar: bağlantı testleri ve tanılama, SSH query, demo sunucu, anlık yedek (snapshot) şifreleme, yetki anahtarı geçmişi, kütüphane, çalma listeleri, eklentiler, dosyalar, ikonlar, kayıtlar, ana sayfa ve ölçümler, ayarlar, IPTV, widget ve banner, akış yolları, denetim listesi, etkinlik günlüğü, dinleyici kumandası, WebSocket.

  Eskiden taşınan açık sorun: Kullanılmış refresh token geri gelince tüm oturumları kapatan koruma eski kodda da çalışmıyor, çünkü kullanılan satır hemen siliniyor. Düzeltmek için satırın saklanması gerekir, ama o zaman aynı anda yenilenen iki sekme birbirini oturumdan atabilir ve eski imaja dönüşte kullanılmış satır yeniden geçerli sayılır. Karar kullanıcıda.

  Eskisinden bilerek farklı: Var olan kullanıcı adı 500 yerine 409, olmayan kullanıcı 500 yerine 404 döner. Radyo silme yalnızca o bağlantının istasyonunu siler; eskisi adresteki bağlantıya bakmıyordu. Radyo numaralarını sıfırlama gerçekten küçültür; eskisinde sayaç yeni satırlardan sonra sıfırlandığı için numaralar küçülmüyordu. Radio Browser yansı listesi DNS ters sorgusu yerine servisin kendi `/json/servers` listesinden alınır.

Eski koddan bilerek farklı olanlar:

- Sunucu paket numarası başa dönünce (65.536 paket) anahtar nesli de ilerler; eski kod gelen paketlerde bunu hiç yapmıyordu.
- Bozuk QuickLZ verisi hataya döner, sessizce yanlış veri üretmez.
- Ses saati gecikince en çok 100 ms'lik geri kalanı hızlı gönderip kapatır; eski kod her gecikmede bir paket aralığı daha bekliyordu.
- ffmpeg hata verince bot `error` durumunda takılı kalmaz: bağlı kalır, hata panele yazılır. Eski kodda bot yeniden başlatılana kadar çalamıyordu.
- Canlı yayın duraklatılınca ffmpeg kapatılır, devamda yeniden açılır; eski kodda arka planda çalışmaya devam ediyordu.
- Daha önce indirilmiş YouTube parçası `yt-dlp` çağrılmadan önbellekten çalar.
- ICY isteği ffmpeg'in adıyla gider: SomaFM eski programın adına 403 verirken sesini ffmpeg'e veriyordu. Başlıktaki kesme işareti de artık başlığı kesmez ("Guns N' Roses").
- Önbellek kilit dosyası üretmez, çünkü klasörü aynı anda tek program kullanır; aynı parçayı isteyenler program içinde tek indirmeyi paylaşır. Dosya düzeni, anahtar ve manifest eskisiyle aynıdır, iki yönde de okunur.

Eski koddan aynen taşınan eksik: Gelen komutlar sıraya konmaz ve tekrar gelenler elenmez. Canlıda sorun çıkarmadı ama uzun bağlantılarda izlenmeli.

## 9. Riskler

- **Görüntü yayını en belirsiz kısım:** pion'un sunduğu bazı özellikler `str0m`'de farklı veya eksik olabilir; denenmeden bilinmiyor. Bu yüzden adım 7'de ayrıca doğrulanır.
- **Panel en büyük iş:** 35.000 satır ve Radix'in hazır verdiği erişilebilirlik elle yazılacak.
- **Üst proje bağı kopar:** `uniskela/ts6-manager` güncellemeleri artık otomatik alınamaz; her düzeltme elle taşınır. Lisans MIT'tir, `LICENSE` ve `THIRD_PARTY_NOTICES.md` korunur.
- **Yeni özellikler:** Geçiş boyunca TypeScript sürümü canlıda kalır. Oraya eklenen her özellik Rust'a da eklenmelidir.
