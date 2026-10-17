# TeamHub Universal Voice Stack

[English](README.en.md) · Türkçe

![TeamHub Universal Voice Stack menüsü](docs/menu.svg)

Temiz bir Linux sunucusuna TeamSpeak 3, TeamSpeak 6, TS6 Manager ve Manager'ın
müzik botu modülünü menü üzerinden kurmak için hazırlanmış etkileşimli kurucu.

Bu paket mevcut bir sunucunun kopyası değildir. İçinde veritabanı, SSH anahtarı,
API anahtarı, kullanıcı parolası, bot kimliği veya canlı sunucu yapılandırması yoktur.
Her kurulum kendi sırlarını hedef makinede üretir. Linux'ta `/opt/teamhub/.env`,
Windows'ta `~/.teamhub/.env` kullanılır; dosya repoya dahil edilmez.

### TeamHub Universal Voice Stack

Version

**0.2.0**

Developer

**Berat Akar**

Source

[GitHub @iNUIZ](https://github.com/iNUIZ)

## Desteklenen sistemler

- Ubuntu/Debian (x86_64 veya arm64)
- Windows 10/11 (Docker Desktop + Git Bash ya da WSL)
- Root/sudo erişimi
- Açık internet bağlantısı

Docker yoksa kurucu Docker'ın resmi APT deposunu ekleyerek Docker Engine ve Compose
eklentisini kurar.

TS3'ün resmî Docker imajı yalnız `amd64/x86_64` yayımlar. ARM64 üzerinde kurucu
Docker'ın amd64 emülasyonunu sınar; emülasyon çalışıyorsa TS3'ü bu modda kurar,
çalışmıyorsa neyin etkinleştirilmesi gerektiğini söyleyerek güvenli biçimde durur.
Manager için önce mevcut mimariye uygun GHCR imajı aranır. ARM imajı yayımlanmamışsa
kurucu upstream kaynak kodunu geçici dizine indirip `Dockerfile.all-in-one` ile doğal
ARM64 imajını yerelde derler; indirilen kaynak kurulum verisine karıştırılmaz.

## Kurulum

```bash
chmod +x install.sh
sudo ./install.sh
```

Windows PowerShell'de Docker Desktop çalışırken:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1
```

Windows başlatıcısı Git Bash'i bulup aynı `install.sh` akışını çalıştırır. Windows
verileri kullanıcı profilindeki `~/.teamhub`, Linux verileri `/opt/teamhub` altında
tutulur. WSL kullanılıyorsa Linux kuralları geçerlidir.

Menüden tek tek bileşen kurabilir veya `Önerilen tam paket` seçeneğini
kullanabilirsiniz. TS3 ve TS6 aynı varsayılan ses/dosya portlarını kullandığından
kurucu her portu sorar ve çalışan bir süreç tarafından kullanılan portu reddeder.

## Bileşenler

| Bileşen | Konteyner imajı | Kalıcı veri |
|---|---|---|
| TeamSpeak 3 | `teamspeak:3.13.8` | Docker volume `teamhub_ts3-data` |
| TeamSpeak 6 | `teamspeaksystems/teamspeak6-server:6.0.0-beta13.1` | `teamhub_ts6-data` |
| TS6 Manager + müzik botu | `ghcr.io/uniskela/ts6-manager/all-in-one:1.11.0` | `teamhub_manager-data`, `teamhub_manager-music` |

Her bileşen için reboot sonrası otomatik başlatma ayrı sorulur. Hayır seçilirse
Compose restart policy `no`, evet seçilirse `unless-stopped` olur.

Kurucu işlemci sayısına göre TS6 worker/query thread değerlerini güvenli sınırlar
içinde ayarlar. Konteynerlerde log rotasyonu, düzgün kapanma süresi ve init işlem
yönetimi kullanılır; böylece disk şişmesi ve zombie process riski azaltılır.

Query portları güvenli varsayılan olarak yalnız `127.0.0.1` adresine bağlanır.
Manager Docker iç ağından sunuculara erişmeye devam eder. Manager web arayüzü de
varsayılan olarak yalnız yereldir; başka cihazlara açılması kurulumda ayrıca sorulur.

## Ağ mimarisi ve IP seçimi

Aynı VPS'e kurulan TeamSpeak sunucusu, Manager ve müzik botu dışarıdan aynı public
IP'yi paylaşır; bu normaldir ve bot için ikinci bir public IP gerekmez. Bileşenler
Docker içinde ayrı konteyner IP'lerine sahiptir ve birbirlerine public IP yerine
özel servis adlarıyla ulaşır:

```text
İnternet ── ses/dosya portları ──> TS3 veya TS6
Kullanıcı ── HTTPS/domain ───────> reverse proxy ──> Manager (127.0.0.1:3000)
Manager + müzik botu ── özel Docker ağı ──> ts3:10080 veya ts6:10080
```

- TeamSpeak istemcisine VPS'in public IP'si veya ona yönlenen domain yazılır.
- Manager bağlantısında aynı paket içindeki sunucu için host `ts3` ya da `ts6`
  seçilir; VPS'in public IP'si kullanılmaz.
- Query portları internete açılmaz. Manager bunlara `teamhub` ağı üzerinden erişir.
- Panel varsayılan olarak `127.0.0.1:3000` adresindedir. Uzak erişim için önerilen
  yapı, paneli doğrudan açmak yerine bir domain ve HTTPS reverse proxy kullanmaktır.
- Bot başka bir fiziksel sunucudaysa yalnız o durumda farklı public IP olabilir;
  mümkünse VPN veya özel ağ üzerinden Query erişimi verilmelidir.

## İlk kullanım

1. Manager kurulumunun sonunda gösterilen `/setup` adresini açın.
2. İlk yönetici hesabını oluşturun; varsayılan kullanıcı/parola yoktur.
3. `Settings > Connections` altında TeamSpeak sunucusunu ekleyin.
4. Aynı paket içindeki TS6 için host `ts6`, WebQuery portu `10080` olmalıdır.
5. API anahtarı Linux'ta `/opt/teamhub/.env`, Windows'ta `~/.teamhub/.env`
   içindeki `TS6_API_KEY` değeridir.
6. `Music Bots > Add Bot` ile temiz bir bot oluşturun ve kanalını seçin.

TS3 kurulumunda `query_protocols=raw,http` etkinleştirilir, `10080/tcp` WebQuery
portu yayımlanır ve Manager için `manage` kapsamlı API anahtarı otomatik üretilip
`/opt/teamhub/.env` içindeki `TS3_API_KEY` alanına yazılır. Böylece Manager ve müzik
botu TS3'e de `host=ts3`, `port=10080` ile bağlanabilir.

## Yönetim

Kurucuyu tekrar çalıştırarak durum, log, servis kontrolü, yedekleme ve seçili
bileşeni kaldırma menülerine ulaşabilirsiniz:

```bash
sudo /path/to/install.sh
```

Doğrudan Compose kullanmak için:

```bash
cd /opt/teamhub
sudo docker compose --env-file .env -f compose.yml ps
sudo docker compose --env-file .env -f compose.yml logs -f manager
```

## Güvenlik notları

- `/opt/teamhub/.env` dosyasını repoya yüklemeyin veya paylaşmayın.
- Manager portunu doğrudan internete açmak yerine TLS kullanan bir reverse proxy
  ve güvenlik duvarı kullanın.
- Konteyner imajları tekrarlanabilir kurulum için test edilmiş sürümlere sabitlenir.
  İndirilen içerik ayrıca immutable SHA-256 manifest digest ile doğrulanır. Yeni
  sürüme geçmeden önce değişiklik notlarını ve güvenlik duyurularını inceleyin.
- YouTube oturum çerezleri gerekiyorsa yalnız Manager arayüzünden ekleyin; pakete
  `cookies.txt` koymayın.

## Kapsam

Kurucu altyapıyı ve temiz kalıcı depolamayı hazırlar. İlk Manager yöneticisinin
oluşturulması ve botun kanal/kimlik seçimi güvenlik nedeniyle web arayüzündeki ilk
kurulum sihirbazında yapılır. Böylece pakette önceden tanımlı hesap veya gizli veri
bulunmaz.

## Upstream bağımlılıkları

`TeamHub Universal Voice Stack` bu reponun adıdır. `teamspeak`, `teamspeaksystems` ve
`uniskela/ts6-manager` ifadeleri başka bir projeye yanlış yönlendirme değil;
kurulan resmî veya açık kaynak bileşenlerin gerçek imaj/kaynak adlarıdır. Bu
projeler TeamHub tarafından sahiplenilmiş gibi gösterilmez ve kendi lisanslarına
tabidir.
