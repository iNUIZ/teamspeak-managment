import { useEffect } from 'react';
import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { extraTurkish } from './tr-extra';

export type Language = 'tr' | 'en';
const turkish: Record<string, string> = {
  ...extraTurkish,
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
  Overview: 'Genel bakış', Dashboard: 'Kontrol paneli', 'Virtual Servers': 'Sanal sunucular',
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
  'Select server...': 'Sunucu seçin...', 'Virtual server': 'Sanal sunucu',
  'Loading virtual servers…': 'Sanal sunucular yükleniyor…', 'No virtual server': 'Sanal sunucu yok',
};

export const useLanguageStore = create<{ language: Language; setLanguage: (language: Language) => void }>()(
  persist((set) => ({ language: 'tr', setLanguage: (language) => set({ language }) }), {
    name: 'teamhub-language',
    merge: (stored, current) => ({ ...current, language: (stored as { language?: unknown })?.language === 'en' ? 'en' : 'tr' }),
  }),
);

export function translate(text: string): string {
  return useLanguageStore.getState().language === 'tr' ? turkish[text] ?? text : text;
}

export function useTranslation() {
  const language = useLanguageStore((s) => s.language);
  useEffect(() => { document.documentElement.lang = language; }, [language]);
  return { language, t: translate };
}
