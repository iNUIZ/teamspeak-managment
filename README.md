# Teamspeak Managment

[English](README.en.md) · Türkçe

![Teamspeak Managment menüsü](docs/menu.svg)

TeamSpeak 3, TeaSpeak, TeamSpeak 6, TS6 Manager ve müzik botu bileşenlerini Docker
Compose ile kuran ve yöneten etkileşimli dağıtım aracı.

**Alpha önizleme:** Uygulama henüz ilk kararlı sürümüne ulaşmadı. Özellikler ve kurulum akışı değişebilir.

| Bilgi | Değer |
|---|---|
| Sürüm | **0.3.8-alpha.1** |
| Geliştirici | **Berat Akar** |
| Kaynak | [GitHub @iNUIZ](https://github.com/iNUIZ) |

## İndirmeler

- [GitHub Releases](https://github.com/iNUIZ/teamspeak-managment/releases): Windows için ZIP, Linux/ARM için TAR.GZ ve imzalı SHA-256 manifesti
- [GitHub Packages](https://github.com/users/iNUIZ/packages?repo_name=teamspeak-managment): npm paketi

Windows arşivi `*-windows.zip`, Ubuntu/Debian x86_64 ve ARM64 arşivi
`*-linux.tar.gz` adıyla yayımlanır. Windows ZIP'i `install.ps1` ile birlikte,
WSL2 içinde gereken ortak `install.sh` ve Docker tanımlarını içerir; Linux arşivi
PowerShell ve npm geliştirme dosyalarını içermez.

## Bileşenler

- TeamSpeak 3 Server ve HTTP WebQuery
- TeaSpeak Server
- TeamSpeak 6 Server ve WebQuery
- TS6 Manager web paneli
- Manager müzik botu desteği
- `!ara` ile karma müzik araması ve güvenli özel komut/eklenti yönetimi
- Servis yönetimi, log görüntüleme, yedekleme ve güvenli güncelleme

Kurulumlar temiz Docker volume'larında oluşturulur. Repo; parola, API anahtarı,
SSH anahtarı, bot kimliği, veritabanı veya mevcut sunucu verisi içermez.

TS3, TeaSpeak ve TS6 birbirinden farklı portlarla aynı makinede birlikte
kurulabilir. Aynı bileşen yeniden seçildiğinde kurucu ikinci bağımsız örneği
`/opt/teamhub-2` ve `teamhub2` Compose projesinde açmayı önerir. İkinci örnek
ayrı volume/ağ kullanır ve port çakışmaları yeniden denetlenir. İstenirse mevcut
örnek de verileri korunarak yeniden oluşturulabilir; yanlışlıkla tekrar seçmek
veriyi silmez.

## Gereksinimler

| Platform | Gereksinimler |
|---|---|
| Ubuntu/Debian x86_64 | Root/sudo, internet bağlantısı |
| Ubuntu/Debian ARM64 | Root/sudo, internet bağlantısı; TS3 için QEMU/binfmt hazırlanır; TeaSpeak desteklenmez |
| Windows 10/11 | Docker Desktop, WSL2, Node.js 18+ ve npm 9+ |

Linux kurucusu Docker bulunmadığında Docker'ın resmî APT deposunu kullanır.
Windows'ta Docker Desktop'ın WSL Integration özelliği etkin olmalıdır.
TS3 ve TS6 taban imajlarına kurulum sırasında dağıtım güvenlik güncellemeleri
uygulanır; CI aynı hardened imajları HIGH/CRITICAL CVE'lere karşı tarar.

## Kurulum

### Ubuntu veya Debian

```bash
git clone https://github.com/iNUIZ/teamspeak-managment.git
cd teamspeak-managment
chmod +x install.sh
sudo ./install.sh
```

### ARM64 Linux

ARM64 kurulumu aynı komutları kullanır. Kurucu Manager imajını sabitlenmiş ve
doğrulanmış kaynak kod ile bu depodaki panel katmanından yerel olarak üretir. Resmî TS3 imajı
amd64 olduğundan TS3 seçeneği Docker binfmt/QEMU emülasyonunu otomatik kurar ve doğrular.

### Windows 10/11 — PowerShell ve npm

1. Docker Desktop'ı kurun ve **Settings > Resources > WSL Integration** altında
   kullandığınız WSL dağıtımını etkinleştirin.
2. Node.js LTS sürümünü ve npm'i kurun.
3. PowerShell'de proje klasöründe çalıştırın:

```powershell
npm install
npm run setup
```

`npm run setup`, PowerShell başlatıcısını çalıştırır ve kurulum motorunu WSL2
üzerinden açar. Git Bash gerekli değildir.

En son Release paketini indirip önce Ed25519 imzasını, ardından SHA-256 değerini
doğrulayan tek satırlık alternatif:

```powershell
irm https://raw.githubusercontent.com/iNUIZ/teamspeak-managment/v0.3.8-alpha.1/bootstrap.ps1 | iex
```

Komut değiştirilemeyen `v0.3.8-alpha.1` sürüm etiketine sabitlenmiştir. Üretim ortamında
çalıştırmadan önce `bootstrap.ps1` içeriğinin incelenmesi yine de önerilir.

### WSL2 terminali

```bash
chmod +x install.sh
sudo ./install.sh
```

## Ağ modeli

TeamSpeak, Manager ve bot aynı VPS üzerinde tek public IP'yi paylaşabilir.
Bileşenler birbirlerine `teamhub` Docker ağı üzerinden bağlanır:

```text
İnternet ── ses/dosya portları ──> TS3, TeaSpeak veya TS6
HTTPS/domain ── reverse proxy ──> Manager (varsayılan 127.0.0.1:3000)
Manager + bot ── özel Docker ağı ──> ts3:10080 veya ts6:10080
```

`0.0.0.0` bir bağlantı adresi değil, servisin tüm ağ arayüzlerinde dinlediğini
belirten bağlama değeridir. İstemciler sunucunun public IP'sini veya alan adını
kullanır. Query portları varsayılan olarak localhost'a bağlanır. Manager kurucusu
iki erişim modu sunar: güvenli varsayılan SSH tüneli veya doğrudan ağ erişimi.
SSH tüneli seçildiğinde Windows PowerShell için hazır komut gösterilir; Linux'a
grafik arayüz kurulması gerekmez. Doğrudan erişimde TLS reverse proxy ve güvenlik
duvarı kullanılması kuvvetle önerilir.

## Veri konumları

- Linux ve WSL2: `/opt/teamhub`
- İkinci bağımsız örnek: `/opt/teamhub-2`
- Windows başlatıcı yapılandırması: WSL2 içindeki `/opt/teamhub`
- Kalıcı veriler: `teamhub_ts3-data`, `teamhub_ts6-data`,
  `teamhub_teaspeak-*`, `teamhub_manager-data`, `teamhub_manager-music`

Üretilen `.env` dosyası gizli anahtarları içerir ve repoya eklenmemelidir.
Menüdeki yedekleme işlemi önce gerçekten çalışan servisleri belirler, yalnızca
onları durdurur ve işlem bitince yine yalnızca onları başlatır. Kalıcı volume'lar,
`.env` ve `compose.yml`; AES-256-CBC + PBKDF2 ile şifrelenmiş `.tar.gz.enc`
arşivine yazılır. Yedek parolası saklanmaz ve en az 16 karakter olmalıdır.

Yedeği açmak için:

```bash
openssl enc -d -aes-256-cbc -pbkdf2 -iter 600000 \
  -in teamhub-TARIH.tar.gz.enc | tar xzf -
```

Manager, UID/GID `10001` ile root olmadan çalışır. TS3, TS6, TeaSpeak ve Manager tüm Linux
capability'lerini bırakır ve `no-new-privileges` korumasını kullanır. Manager
ayrıca salt okunur kök dosya sistemiyle çalışır.

## İlk Manager bağlantısı

1. Manager kurulurken terminalde yönetici kullanıcı adı ve parolasını belirleyin.
   Parola gizli alınır; ilk açılıştan önce bcrypt özeti hazırlanır. Mevcut hesaplar korunur.
2. Kurucunun gösterdiği `/login` adresinden doğrudan giriş yapın.
3. Kurucunun yönettiği TS3 ve TS6 sunucuları Manager'a otomatik eklenir. Önce sunucu,
   önce Manager veya 7 numaralı tam paket kurulabilir; API anahtarlarını elle taşımanız gerekmez.
4. Aynı makinedeki `teamhub` ve `teamhub2` kurulumları mevcut Manager üzerinden yönetilebilir.
   Kurucu gerekli özel Docker ağını bağlar; Query portunu internete açmaz. Bağlantılar
   aynı kayıt üzerinde güncellenir; kullanıcı tarafından verilen adlar ve botlar korunur.
   Yeni kayıtlar için Manager kısa süre yeniden başlatılır; çalışan botların bağlantısı kesilebilir.
5. Uzak sunucuları **Ayarlar → Bağlantılar** bölümünden ekleyin. Sunucu seçicisinden TS3/TS6
   bağlantıları ve sanal sunucular arasında geçiş yapın; her bağlantıda ayrı botlar ve komutlar yönetilebilir.

Otomatik eşleştirme yalnız bu kurucunun oluşturduğu, API anahtarı mevcut TS3/TS6
kurulumlarını kapsar. TeaSpeak ve başka makinelerdeki sunucular otomatik keşfedilmez.
Eksik API anahtarı veya hazır olmayan Manager açık bir uyarı üretir; sahte bağlantı oluşturulmaz.

## Lisans ve bağımlılıklar

Kurucu kodu [MIT lisansı](LICENSE) ile yayımlanır. TeamSpeak yazılımları ve
`uniskela/ts6-manager` kendi lisansları ve kullanım koşullarına tabidir.
TeaSpeak ücretsiz kullanılabilir; varsa lisans kodu kurulum sırasında gizli
olarak alınır ve yalnız `/opt/teamhub/.env` içinde saklanır. TeaSpeak 1.5.6
arşivi sabit SHA-256 değeri doğrulandıktan sonra yerel imaja alınır.
