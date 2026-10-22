import { useEffect } from 'react';
import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { extraTurkish } from './tr-extra';
import { operationsTurkish } from './tr-operations';
import { reviewTurkish } from './tr-review';
import { filesTurkish } from './tr-files';
import { panelTurkish } from './tr-panel';

export type Language = 'tr' | 'en';
const turkish: Record<string, string> = {
  'Ready': 'Hazır',
  'Change YouTube playback method': 'YouTube oynatma yöntemini değiştir',
  'YouTube uses cookies only. Uploaded cookies are used automatically for new searches and playback. Without cookies, YouTube requests are blocked; radio and local media remain available.': 'YouTube yalnızca çerezle çalışır. Yüklenen çerezler yeni arama ve oynatma işlemlerinde otomatik kullanılır. Çerez yoksa YouTube istekleri engellenir; radyo ve yerel medya kullanılabilir.',
  'Cookie file stored — not in use': 'Çerez dosyası kayıtlı — kullanımda değil',
  'Remove': 'Kaldır',
  'Limits saved': 'Sınırlar kaydedildi',
  'Failed to save limits': 'Sınırlar kaydedilemedi',
  'Download & Stream Limits': 'İndirme ve yayın sınırları',
  'Limit playlist imports to at most 500 tracks per import.': 'Her liste içe aktarma işlemini en fazla 500 parçayla sınırlayın.',
  'Stream YouTube videos directly': 'YouTube videolarını doğrudan oynat',
  'Play video directly instead of downloading first. Some networks block direct playback; disable this if video does not start.': 'Videoyu indirmek yerine doğrudan oynatın. Bazı ağlar doğrudan oynatmayı engeller; video başlamıyorsa bu seçeneği kapatın.',
  'Max video duration (seconds, 0 = unlimited)': 'En uzun video süresi (saniye, 0 = sınırsız)',
  'Applies to downloaded videos; not used when YouTube videos are streamed directly.': 'İndirilen videolar için geçerlidir; doğrudan oynatılan YouTube videolarını etkilemez.',
  'Max playlist import tracks (1–500)': 'Bir defada içe aktarılacak parça sınırı (1–500)',
  'Save Limits': 'Sınırları kaydet',
  ...extraTurkish,
  ...operationsTurkish,
  ...reviewTurkish,
  ...filesTurkish,
  'Plain theme background': 'Sade tema arka planı', 'Technical grid lines': 'İnce ızgara çizgileri',
  'Subtle dot matrix': 'Hafif noktalı desen', 'Soft accent-coloured glow': 'Vurgu renginde yumuşak ışıltı',
  'Slow-moving accent gradient': 'Yavaş hareket eden renk geçişi', 'Subtle static texture': 'Hafif sabit doku',
  "Follows your device's reduced-motion setting": 'Cihazınızın hareket tercihine uyar',
  'No decorative background animation': 'Arka plan animasyonları kapalı',
  'Allow decorative background animation': 'Arka plan animasyonları açık',
  'Bundled extensions': 'Yerleşik uzantılar', 'Third-party extensions': 'Üçüncü taraf uzantılar',
  'Install from manifest': 'Manifest dosyasından yükle', 'Validate and install': 'Doğrula ve yükle',
  'No third-party extension installed yet.': 'Henüz üçüncü taraf uzantı yüklenmedi.',
  'Select a server connection to manage extensions.': 'Uzantıları yönetmek için bir sunucu bağlantısı seçin.',
  'Extension installed': 'Uzantı yüklendi', 'Extension could not be installed': 'Uzantı yüklenemedi',
  'Manifest must be valid JSON': 'Manifest geçerli bir JSON dosyası olmalı', Bundled: 'Yerleşik',
  'Server Status': 'Sunucu durumu', 'Live TeamSpeak server overview': 'TeamSpeak sunucunuzun canlı özeti',
  'Availability, utilisation, and connection health': 'Erişilebilirlik, kapasite kullanımı ve bağlantı sağlığı',
  Online: 'Çevrimiçi', 'Online users': 'Çevrimiçi kullanıcılar', Traffic: 'Trafik', Uptime: 'Çalışma süresi',
  'Packet loss': 'Paket kaybı', Incoming: 'Gelen', Outgoing: 'Giden',
  'Runtime & Capacity': 'Sistem ve kapasite', 'Runtime identity and available capacity': 'Sunucu bilgileri ve kullanılabilir kapasite',
  'Server version': 'Sunucu sürümü', Platform: 'Platform', 'Total slots': 'Toplam kapasite', 'Available slots': 'Boş kapasite',
  'Live monitoring active': 'Canlı izleme etkin', 'Refreshing live data…': 'Canlı veriler yenileniyor…',
  'Live updates interrupted': 'Canlı güncellemeler kesildi', 'Waiting for TeamSpeak Query…': 'TeamSpeak Query bekleniyor…',
  'Incoming and outgoing bandwidth over the latest local samples': 'Son ölçümlere göre gelen ve giden bant genişliği',
  'Collecting bandwidth samples…': 'Bant genişliği ölçümleri toplanıyor…',
  'Connect your TeamSpeak server to get started': 'Başlamak için TeamSpeak sunucunuzu bağlayın',
  'Add a connection under Settings to manage channels, clients, bots, and more.': 'Kanalları, kullanıcıları ve botları yönetmek için Ayarlar bölümünden bir bağlantı ekleyin.',
  'Go to Connections': 'Bağlantılara git', 'No server connection configured': 'Henüz sunucu bağlantısı yok',
  'No server selected': 'Sunucu seçilmedi', 'Open connection setup': 'Bağlantı kurulumunu aç',
  'Use the setup wizard in Settings → Connections to add your TeamSpeak server.': 'TeamSpeak sunucunuzu Ayarlar → Bağlantılar bölümündeki sihirbazla ekleyin.',
  'Select a server connection from the header to view the dashboard.': 'Kontrol panelini görmek için üst menüden bir sunucu bağlantısı seçin.',
  Flows: 'Akışlar', 'Chat commands': 'Sohbet komutları', Extensions: 'Uzantılar',
  'From Template': 'Şablondan oluştur', 'New Bot': 'Yeni bot',
  'Change Password': 'Parolayı değiştir', 'Current Password': 'Mevcut parola', 'New Password': 'Yeni parola',
  'Confirm New Password': 'Yeni parolayı doğrula', 'Changing...': 'Değiştiriliyor...',
  Overview: 'Genel bakış', Dashboard: 'Kontrol paneli', 'Virtual Servers': 'TeamSpeak sunucuları',
  Management: 'Yönetim', Channels: 'Kanallar', Clients: 'Kullanıcılar',
  'Server Groups': 'Sunucu grupları', 'Channel Groups': 'Kanal grupları', Permissions: 'Yetkiler',
  Security: 'Güvenlik', Bans: 'Yasaklar', Tokens: 'Yetki anahtarları', Content: 'İçerik',
  Files: 'Dosyalar', Complaints: 'Şikâyetler', Messages: 'Mesajlar', System: 'Sistem',
  Automation: 'Otomasyon', 'Bot Hub': 'Bot merkezi', 'Bot Flows': 'Bot akışları',
  'Media Library': 'Medya kütüphanesi', 'Server Logs': 'Sunucu kayıtları',
  'Audit Log': 'Denetim kaydı', 'Activity Journal': 'Etkinlik günlüğü',
  'Media Diagnostics': 'Medya tanılama', Widgets: 'Bileşenler', Settings: 'Ayarlar',
  Connections: 'Bağlantılar', Connection: 'Bağlantı', Account: 'Hesap', Appearance: 'Görünüm',
  Users: 'Kullanıcılar', About: 'Hakkında', Collapse: 'Daralt', 'Log out': 'Çıkış yap',
  'Server Administration Panel': 'Sunucu yönetim paneli', 'Sign in to continue': 'Devam etmek için giriş yapın',
  Username: 'Kullanıcı adı', Password: 'Parola', 'Sign In': 'Giriş yap', 'Signing in...': 'Giriş yapılıyor...',
  'Invalid credentials. Please try again.': 'Kullanıcı adı veya parola hatalı. Tekrar deneyin.',
  'Unable to sign in. Check your connection and try again.': 'Giriş yapılamadı. Bağlantınızı kontrol edip tekrar deneyin.',
  'Base theme': 'Ana tema', Accent: 'Vurgu rengi', Light: 'Açık', Dark: 'Koyu', Black: 'Siyah',
  Cyan: 'Turkuaz', Violet: 'Mor', Red: 'Kırmızı', Blue: 'Mavi', Emerald: 'Zümrüt', Amber: 'Turuncu',
  Background: 'Arka plan', Motion: 'Hareket', None: 'Yok', Grid: 'Izgara', Dots: 'Noktalar',
  Glow: 'Işıltı', Aurora: 'Aurora', Noise: 'Doku', Subtle: 'Hafif', Normal: 'Normal', Strong: 'Yoğun',
  'Background intensity': 'Arka plan yoğunluğu', Off: 'Kapalı', On: 'Açık',
  'Bright neutral surfaces': 'Aydınlık ve sade yüzeyler', 'Low-glare dark surfaces': 'Göz yormayan koyu yüzeyler',
  'True-black foundation': 'Tam siyah görünüm', 'Semantic colours': 'Durum renkleri',
  'Destructive / error': 'Silme / hata', Success: 'Başarılı', Warning: 'Uyarı',
  'Appearance is stored only in this browser. Operational status colours keep their meaning in every combination.':
    'Görünüm bu tarayıcıda saklanır. Durum renkleri tüm temalarda anlamını korur.',
  'Decorative motion is never required to understand application state.': 'Görsel hareketler uygulama durumunu anlamak için gerekli değildir.',
  'Sits behind content and never affects readability or interaction.': 'İçeriğin arkasında kalır; okunabilirliği ve etkileşimi etkilemez.',
  'Select server...': 'Sunucu seçin...', 'Virtual server': 'TeamSpeak sunucusu',
  'Loading virtual servers…': 'TeamSpeak sunucuları yükleniyor…', 'No virtual server': 'TeamSpeak sunucusu yok',
  'TeamSpeak servers': 'TeamSpeak sunucuları',
  'Quick actions': 'Hızlı işlemler',
  ONLINE: 'Çevrimiçi',
  'Connection failed': 'Bağlantı kurulamadı', 'Could not load virtual servers.': 'TeamSpeak sunucuları yüklenemedi.',
  'Virtual server refresh failed. The last successful list is still displayed.':
    'Sunucu listesi yenilenemedi. Son alınan liste gösteriliyor.',
  Retry: 'Tekrar dene', 'Retrying…': 'Tekrar deneniyor…',
  'Bot deleted': 'Bot silindi', 'Failed to delete bot': 'Bot silinemedi',
  'Open navigation menu': 'Gezinme menüsünü aç', 'Open account menu': 'Hesap menüsünü aç',
  'Account menu': 'Hesap menüsü', 'Switch to light theme': 'Açık temaya geç', 'Switch to dark theme': 'Koyu temaya geç',
  'WebQuery only': 'Yalnızca WebQuery', 'WebQuery + native metrics': 'WebQuery + sunucu ölçümleri',
  'WebQuery · metrics unreachable': 'WebQuery · ölçümlere erişilemiyor',
  'WebQuery · metrics timed out': 'WebQuery · ölçüm zaman aşımı',
  'WebQuery · metrics invalid': 'WebQuery · geçersiz ölçümler',
  'WebQuery · metrics unscoped': 'WebQuery · sunucu eşleşmesi doğrulanamadı',
  'WebQuery · metrics unavailable': 'WebQuery · ölçümler kullanılamıyor',
  'Virtual servers unavailable': 'TeamSpeak sunucularına erişilemiyor',
  'Virtual servers for the selected connection could not be loaded. Open Virtual Servers for details.':
    'Seçili bağlantıdaki TeamSpeak sunucuları yüklenemedi. Ayrıntılar için TeamSpeak sunucuları sayfasını açın.',
  'View virtual servers': 'TeamSpeak sunucularını gör', 'No virtual server available': 'Kullanılabilir TeamSpeak sunucusu yok',
  'The selected connection has no virtual server to manage. Start one or choose another connection in the header.':
    'Bu bağlantıda yönetilebilecek TeamSpeak sunucusu yok. Bir sunucu başlatın veya üst menüden başka bağlantı seçin.',
  'Choose a server connection from the selector in the header.': 'Üst menüden bir sunucu bağlantısı seçin.',
  'No server connection available': 'Erişilebilir sunucu bağlantısı yok',
  'Ask an administrator to add a TeamSpeak connection or grant you access to one.':
    'Yöneticiden TeamSpeak bağlantısı eklemesini veya mevcut bir bağlantıya erişim vermesini isteyin.',
  'This will permanently delete this media bot and disconnect it from the server.':
    'Bu medya botu kalıcı olarak silinecek ve sunucuyla bağlantısı kesilecek.',
  'base theme': 'ana tema', accent: 'vurgu rengi', motion: 'hareket', background: 'arka plan',
  Start: 'Başlat', Stop: 'Durdur', 'Starting…': 'Başlatılıyor…', 'Stopping…': 'Durduruluyor…',
  'Server management': 'Sunucu yönetimi', 'Advanced tools': 'Gelişmiş araçlar',
  'Search menu': 'Menüde ara', 'Search pages…': 'Sayfa ara…',
  'No matching pages': 'Eşleşen sayfa bulunamadı', 'Clear search': 'Aramayı temizle',
  'Connected users will be disconnected.': 'Bağlı kullanıcıların bağlantısı kesilecek.',
  'Stop server': 'Sunucuyu durdur', 'Stop TeamSpeak server': 'TeamSpeak sunucusunu durdur',
  'Server list up to date': 'Sunucu listesi güncel', 'Refreshing servers…': 'Sunucular yenileniyor…',
  'No TeamSpeak servers found': 'TeamSpeak sunucusu bulunamadı',
  'These are the TeamSpeak servers in the selected connection. Each has its own voice port and channels.':
    'Seçili bağlantıdaki TeamSpeak sunucuları burada görünür. Her sunucunun kendi ses portu ve kanalları vardır.',
  'Use default': 'Varsayılanı kullan', Upload: 'Yükle', 'Upload avatar': 'Avatar yükle',
  'Now-playing cover': 'Çalan şarkının kapağı',
  'Uses the YouTube cover while playing; restores the default image when idle. Custom uploads are kept.':
    'TeamSpeak içinde çalarken YouTube kapağını, boşta varsayılan görseli kullanır. Açıklamaya şarkı, sanatçı ve süre bilgisi yazılır. Yüklediğiniz özel görsel korunur.',
  'PNG, JPEG or GIF, at most 200 KB.': 'PNG, JPEG veya GIF; en fazla 200 KB.',
  'Could not update avatar': 'Avatar güncellenemedi',
  'Bot stopped': 'Bot durduruldu', 'Failed to stop bot': 'Bot durdurulamadı',
  'Widget link': 'Bileşen bağlantısı', 'Delete bot': 'Botu sil',
  'Return home (suspend automation)': 'Ana kanala dön (otomasyonu duraklat)',
  ...panelTurkish,
};

const normalizeUiText = (text: string) => text.trim().replace(/\s+/g, ' ');
const normalizedCatalog = Object.fromEntries(Object.entries(turkish).map(([key, value]) => [normalizeUiText(key), value.trim()]));
const catalogValues = new Set(Object.values(normalizedCatalog));

export const useLanguageStore = create<{ language: Language; setLanguage: (language: Language) => void }>()(
  persist((set) => ({ language: 'tr', setLanguage: (language) => set({ language }) }), {
    name: 'teamhub-language',
    merge: (stored, current) => ({ ...current, language: (stored as { language?: unknown })?.language === 'en' ? 'en' : 'tr' }),
  }),
);

export function translate(text: string, ...values: unknown[]): string {
  const normalized = normalizeUiText(text);
  const found = Object.prototype.hasOwnProperty.call(normalizedCatalog, normalized) ? normalizedCatalog[normalized] : undefined;
  let result = useLanguageStore.getState().language === 'tr' && found !== undefined
    ? `${text.match(/^\s*/)?.[0] ?? ''}${found}${text.match(/\s*$/)?.[0] ?? ''}` : text;
  if (values.length) result = result.replace(/\{(\d+)\}/g, (token, index) => Number(index) < values.length ? String(values[Number(index)] ?? '') : token);
  return result;
}

export function useTranslation() {
  const language = useLanguageStore((s) => s.language);
  useEffect(() => { document.documentElement.lang = language; }, [language]);
  return { language, t: translate };
}

/** Catalog membership, independent of active language (also accepts existing Turkish copy). */
export function hasUiTranslation(text: string): boolean {
  const normalized = normalizeUiText(text);
  return Object.prototype.hasOwnProperty.call(normalizedCatalog, normalized) || catalogValues.has(normalized);
}

/** Untrusted service errors are not interface copy; never leak untranslated prose into TR. */
export function translateUiError(text: string, fallback: string): string {
  if (!text) return '';
  if (useLanguageStore.getState().language === 'en' || hasUiTranslation(text)) return translate(text);
  if (hasUiTranslation(fallback)) return translate(fallback);
  // Caller-authored Turkish fallbacks may interpolate user-entered names.
  return /[çğıöşüÇĞİÖŞÜ]/.test(fallback) ? fallback : translate('Something went wrong');
}
