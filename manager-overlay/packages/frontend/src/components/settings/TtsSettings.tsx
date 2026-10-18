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
export function TtsSettings() {
  const { language } = useTranslation();
  const tr = language === 'tr';
  const query = useQuery({ queryKey: ['tts-settings'], queryFn: () => api.get('/settings/tts').then(r => r.data as { installed: boolean; settings: Settings }) });
  const [draft, setDraft] = useState<Settings | null>(null);
  useEffect(() => { if (query.data) setDraft(query.data.settings); }, [query.data]);
  const save = useMutation({ mutationFn: () => api.put('/settings/tts', draft),
    onSuccess: () => { toast.success(tr ? 'TTS ayarları kaydedildi' : 'TTS settings saved'); void query.refetch(); },
    onError: () => toast.error(tr ? 'TTS ayarları kaydedilemedi' : 'Could not save TTS settings'),
  });
  if (query.isError) return <p role="alert">{tr ? 'TTS ayarları yüklenemedi.' : 'Could not load TTS settings.'}</p>;
  if (!draft) return <p>{tr ? 'Yükleniyor…' : 'Loading…'}</p>;
  return <Card className="max-w-2xl"><CardHeader><CardTitle>{tr ? 'Sesli okuma · Piper TTS' : 'Text to speech · Piper TTS'}</CardTitle></CardHeader>
    <CardContent className="space-y-5">
      <p className="text-sm text-muted-foreground">{tr ? 'Türkçe DFKI ve İngilizce Lessac sesleri, kurulumdan sonra internet veya harici API anahtarı olmadan sunucuda çalışır.' : 'Turkish DFKI and English Lessac voices run locally after installation, without an external API key.'}</p>
      {!query.data?.installed && <p role="status" className="rounded-lg border border-warning/40 p-3 text-sm">{tr ? 'Ses paketi kurulu değil. Linux kurucusunda 14 → TTS paketini kur seçeneğini çalıştırın; ardından bu sayfayı yenileyin. Panel sunucuda root yetkisiyle paket çalıştırmaz.' : 'Voice package is not installed. Run installer option 14, then refresh this page. The panel does not run package installation as root.'}</p>}
      <div className="flex items-center gap-3"><Switch id="tts-enabled" disabled={!query.data?.installed} checked={draft.enabled} onCheckedChange={enabled => setDraft({ ...draft, enabled })} /><Label htmlFor="tts-enabled">{tr ? '!say komutunu etkinleştir' : 'Enable !say'}</Label></div>
      <div className="grid gap-4 sm:grid-cols-2">
        <div><Label htmlFor="tts-language">{tr ? 'Varsayılan ses' : 'Default voice'}</Label><select id="tts-language" className="mt-1 h-10 w-full rounded-md border bg-background px-3" value={draft.language} onChange={e => setDraft({ ...draft, language: e.target.value as 'tr'|'en' })}><option value="tr">Türkçe · DFKI</option><option value="en">English · Lessac</option></select></div>
        <div><Label htmlFor="tts-volume">{tr ? 'Ses çarpanı (0.1–1.5)' : 'Volume multiplier (0.1–1.5)'}</Label><Input id="tts-volume" type="number" min="0.1" max="1.5" step="0.1" value={draft.volume} onChange={e => setDraft({ ...draft, volume: Number(e.target.value) })} /></div>
        <div><Label htmlFor="tts-limit">{tr ? 'Karakter sınırı (20–500)' : 'Character limit (20–500)'}</Label><Input id="tts-limit" type="number" min="20" max="500" value={draft.maxCharacters} onChange={e => setDraft({ ...draft, maxCharacters: Number(e.target.value) })} /></div>
        <div><Label htmlFor="tts-cooldown">{tr ? 'Bekleme süresi (5–120 saniye)' : 'Cooldown (5–120 seconds)'}</Label><Input id="tts-cooldown" type="number" min="5" max="120" value={draft.cooldownSeconds} onChange={e => setDraft({ ...draft, cooldownSeconds: Number(e.target.value) })} /></div>
      </div>
      <div className="rounded-lg bg-muted/50 p-3 font-mono text-sm">!say Merhaba!<br/>!say --tr Hoş geldiniz.<br/>!say --en Welcome to our server.</div>
      <p className="text-xs text-muted-foreground">{tr ? 'TTS için bot TS’ye bağlı olmalı ve mevcut medya durdurulmalıdır. Komut, oynatma yetkilerini kullanır.' : 'The bot must be connected and current media must be stopped. The command respects playback permissions.'}</p>
      <Button disabled={save.isPending} onClick={() => save.mutate()}>{tr ? 'Kaydet' : 'Save'}</Button>
    </CardContent></Card>;
}
