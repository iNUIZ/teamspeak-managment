import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';
import { useEffect, useState } from 'react';
import { useMutation, useQuery } from '@tanstack/react-query';
import { toast } from 'sonner';
import api from '@/api/client';
import { useTranslation } from '@/lib/i18n';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';

type Settings = { enabled: boolean; language: 'tr' | 'en'; maxCharacters: number; cooldownSeconds: number; volume: number };
export function BotTtsSettings({ botId }: { botId: number }) {
  useUiLanguageStore(state => state.language);
  const { language } = useTranslation();
  const tr = language === 'tr';
  const query = useQuery({ queryKey: ['bot-tts-settings', botId], queryFn: () => api.get(`/music-bots/${botId}/tts`).then(r => r.data as { installed: boolean; settings: Settings }), refetchOnWindowFocus: false, refetchOnReconnect: false });
  const [draft, setDraft] = useState<Settings | null>(null);
  useEffect(() => { if (query.data) setDraft(query.data.settings); }, [query.data]);
  const save = useMutation({ mutationFn: (settings: Settings) => api.put(`/music-bots/${botId}/tts`, settings),
    onSuccess: () => { toast.success(tr ? uiText("TTS ayarları kaydedildi") : uiText("TTS settings saved")); void query.refetch(); },
    onError: () => toast.error(tr ? uiText("TTS ayarları kaydedilemedi") : uiText("Could not save TTS settings")),
  });
  if (query.isError) return <div><p role="alert">{tr ? uiText("TTS ayarları yüklenemedi.") : uiText("Could not load TTS settings.")}</p><Button variant="outline" onClick={() => void query.refetch()}>{tr ? uiText("Tekrar dene") : uiText("Retry")}</Button></div>;
  if (!draft) return <p>{tr ? uiText("Yükleniyor…") : uiText("Loading…")}</p>;
  const valid = Number.isInteger(draft.maxCharacters) && draft.maxCharacters >= 20 && draft.maxCharacters <= 500
    && Number.isInteger(draft.cooldownSeconds) && draft.cooldownSeconds >= 5 && draft.cooldownSeconds <= 120
    && Number.isFinite(draft.volume) && draft.volume >= 0.1 && draft.volume <= 1.5;
  const dirty = JSON.stringify(draft) !== JSON.stringify(query.data?.settings);
  return <Card><CardHeader><CardTitle>{tr ? uiText("Sesli okuma · Piper TTS") : uiText("Text to speech · Piper TTS")}</CardTitle></CardHeader>
    <CardContent className="space-y-5">
      <p className="text-sm text-muted-foreground">{tr ? uiText("Bu ayarlar yalnızca seçili botu etkiler. Diğer botların sesleri ve komut ayarları değişmez.") : uiText("These settings apply only to this bot. Other bots keep their own voice and command settings.")}</p>
      <p className="text-sm text-muted-foreground">{tr ? uiText("Türkçe DFKI ve İngilizce Lessac sesleri, kurulumdan sonra internet veya harici API anahtarı olmadan sunucuda çalışır.") : uiText("Turkish DFKI and English Lessac voices run locally after installation, without an external API key.")}</p>
      {!query.data?.installed && <p role="status" className="rounded-lg border border-warning/40 p-3 text-sm">{tr ? uiText("Ses paketi kurulu değil. Linux kurucusunda 14 → TTS paketini kur seçeneğini çalıştırın; ardından bu sayfayı yenileyin. Panel sunucuda root yetkisiyle paket çalıştırmaz.") : uiText("Voice package is not installed. Run installer option 14, then refresh this page. The panel does not run package installation as root.")}</p>}
      <div className="flex items-center gap-3"><Switch id="tts-enabled" disabled={!query.data?.installed} checked={draft.enabled} onCheckedChange={enabled => setDraft({ ...draft, enabled })} /><Label htmlFor="tts-enabled">{tr ? '!say komutunu etkinleştir' : uiText("Enable !say")}</Label></div>
      <div className="grid gap-4 sm:grid-cols-2">
        <div><Label htmlFor="tts-language">{tr ? uiText("Varsayılan ses") : uiText("Default voice")}</Label><select id="tts-language" className="mt-1 h-10 w-full rounded-md border bg-background px-3" value={draft.language} onChange={e => setDraft({ ...draft, language: e.target.value as 'tr'|'en' })}><option value="tr">{tr ? uiText("Türkçe") : uiText("Turkish")} {uiText(" · DFKI")}</option><option value="en">{tr ? uiText("İngilizce") : uiText("English")} {uiText(" · Lessac")}</option></select></div>
        <div><Label htmlFor="tts-volume">{tr ? uiText("Ses çarpanı (0.1–1.5)") : uiText("Volume multiplier (0.1–1.5)")}</Label><Input id="tts-volume" type="number" min="0.1" max="1.5" step="0.1" value={draft.volume} onChange={e => setDraft({ ...draft, volume: Number(e.target.value) })} /></div>
        <div><Label htmlFor="tts-limit">{tr ? uiText("Karakter sınırı (20–500)") : uiText("Character limit (20–500)")}</Label><Input id="tts-limit" type="number" min="20" max="500" value={draft.maxCharacters} onChange={e => setDraft({ ...draft, maxCharacters: Number(e.target.value) })} /></div>
        <div><Label htmlFor="tts-cooldown">{tr ? uiText("Bekleme süresi (5–120 saniye)") : uiText("Cooldown (5–120 seconds)")}</Label><Input id="tts-cooldown" type="number" min="5" max="120" value={draft.cooldownSeconds} onChange={e => setDraft({ ...draft, cooldownSeconds: Number(e.target.value) })} /></div>
      </div>
      <div className="rounded-lg bg-muted/50 p-3 font-mono text-sm">!say Merhaba!<br/>!say --tr Hoş geldiniz.<br/>!say --en Welcome to our server.</div>
      <p className="text-xs text-muted-foreground">{tr ? uiText("TTS için bot TS’ye bağlı olmalı ve mevcut medya durdurulmalıdır. Komut, oynatma yetkilerini kullanır.") : uiText("The bot must be connected and current media must be stopped. The command respects playback permissions.")}</p>
      {!valid && <p role="alert">{tr ? uiText("Değerleri belirtilen sınırlar içinde girin.") : uiText("Enter values within the shown limits.")}</p>}
      <Button disabled={save.isPending || !dirty || !valid || (!!draft.enabled && !query.data?.installed)} onClick={() => save.mutate(draft)}>{save.isPending ? (tr ? uiText("Kaydediliyor…") : uiText("Saving…")) : (tr ? uiText("Kaydet") : uiText("Save"))}</Button>
    </CardContent></Card>;
}
