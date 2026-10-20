import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import api from '@/api/client';
import { useTranslation } from '@/lib/i18n';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { BotTtsSettings } from './BotTtsSettings';
import { CommandsTab } from '@/pages/media-bots/CommandsTab';
import { ExtensionsTab } from '@/pages/extensions/ExtensionsTab';

type BotExtensions = { serverConfigId: number; disabledCommands: string[];
  builtin: { name: string; canonical: string; usage: string }[];
  custom: { name: string; description: string; enabled: boolean }[] };

export function BotExtensionsDialog({ botId, botName, onClose }: { botId: number; botName: string; onClose: () => void }) {
  const { language, t } = useTranslation();
  const tr = language === 'tr';
  const [search, setSearch] = useState('');
  const qc = useQueryClient();
  const queryKey = ['bot-extensions', botId];
  const query = useQuery({ queryKey, queryFn: () => api.get(`/music-bots/${botId}/extensions`).then(r => r.data as BotExtensions), retry: false });
  const toggle = useMutation({
    mutationFn: (disabledCommands: string[]) => api.put(`/music-bots/${botId}/extensions`, { disabledCommands }),
    onSuccess: (_response, disabledCommands) => {
      qc.setQueryData<BotExtensions>(queryKey, previous => previous ? { ...previous, disabledCommands } : previous);
      toast.success(tr ? 'Bu botun komut ayarı kaydedildi' : 'Command setting saved for this bot');
    },
    onError: () => toast.error(tr ? 'Komut ayarı kaydedilemedi' : 'Could not save command setting'),
  });
  const commands = [...(query.data?.builtin ?? []).filter(c => c.name === c.canonical && c.name !== 'say').map(c => ({ name: c.name, usage: c.usage, enabled: true })),
    ...(query.data?.custom ?? []).map(c => ({ ...c, usage: `!${c.name}` }))].filter(c => c.name.includes(search.toLowerCase().replace(/^!/, '')));
  return <Dialog open onOpenChange={open => { if (!open) onClose(); }}>
    <DialogContent className="max-w-4xl">
      <DialogHeader className="pr-10 text-left"><DialogTitle className="break-words leading-snug">{botName} · {tr ? 'Eklentiler ve komutlar' : 'Extensions and commands'}</DialogTitle></DialogHeader>
      <p className="text-sm text-muted-foreground">{tr ? 'Botun özelliklerini buradan yönetin. Komut anahtarları yalnızca bu botu etkiler; TTS dil ve ses ayarları da bota özeldir.' : 'Manage this bot’s features here. Command switches and TTS voice settings apply only to this bot.'}</p>
      {query.isPending ? <p>{tr ? 'Yükleniyor…' : 'Loading…'}</p> : query.isError ? <div><p role="alert">{tr ? 'Eklentiler yüklenemedi.' : 'Could not load extensions.'}</p><Button onClick={() => void query.refetch()}>{tr ? 'Tekrar dene' : 'Retry'}</Button></div> : <Tabs defaultValue="commands" onValueChange={tab => { if (tab === 'commands') void query.refetch(); }}>
        <TabsList className="w-full flex-wrap h-auto">
          <TabsTrigger value="commands">{tr ? 'Komutlar' : 'Commands'}</TabsTrigger>
          <TabsTrigger value="tts">{tr ? 'Sesli okuma (TTS)' : 'Text to speech (TTS)'}</TabsTrigger>
          <TabsTrigger value="custom">{tr ? 'Özel komut kitaplığı' : 'Custom command library'}</TabsTrigger>
          <TabsTrigger value="packages">{tr ? 'Eklenti paketleri' : 'Extension packages'}</TabsTrigger>
        </TabsList>
        <TabsContent value="commands" className="space-y-3">
          <Input aria-label={tr ? 'Komutlarda ara' : 'Search commands'} placeholder={tr ? 'Komutlarda ara…' : 'Search commands…'} value={search} onChange={e => setSearch(e.target.value)} />
          <p className="text-xs text-muted-foreground">{tr ? 'Takma komutlar aynı anahtarı kullanır (ör. !skip ve !next). !say, Sesli okuma sekmesinden yönetilir.' : 'Aliases share one switch (e.g. !skip and !next). Configure !say in the text-to-speech tab.'}</p>
          {commands.length === 0 && <p>{tr ? 'Eşleşen komut yok.' : 'No matching commands.'}</p>}
          <div className="grid gap-2 sm:grid-cols-2">{commands.map(c => <div key={c.name} className="flex min-w-0 items-center justify-between gap-3 rounded-lg border p-3">
            <code className="break-all text-sm">{t(c.usage)}</code>
            <Switch aria-label={`!${c.name}`} disabled={toggle.isPending || !c.enabled} checked={c.enabled && !query.data!.disabledCommands.includes(c.name)} onCheckedChange={enabled => toggle.mutate(enabled ? query.data!.disabledCommands.filter(n => n !== c.name) : [...query.data!.disabledCommands, c.name])} />
          </div>)}</div>
        </TabsContent>
        <TabsContent value="tts"><BotTtsSettings key={botId} botId={botId} /></TabsContent>
        <TabsContent value="custom">
          <p className="my-3 rounded-md border p-3 text-sm">{tr ? 'Bu kitaplıktaki tanımlar aynı TeamSpeak bağlantısındaki botlar arasında ortaktır. Bir komutu yalnızca bu botta kapatmak için Komutlar sekmesini kullanın.' : 'Definitions in this library are shared by bots on the same TeamSpeak connection. Use the Commands tab to disable one only for this bot.'}</p>
          <CommandsTab fixedConfigId={query.data!.serverConfigId} />
        </TabsContent>
        <TabsContent value="packages">
          <p className="my-3 rounded-md border p-3 text-sm">{tr ? 'Paket kurma ve kaldırma bu sunucu bağlantısının ortak kitaplığını değiştirir. Paket komutlarını bu botta Komutlar sekmesinden açıp kapatabilirsiniz.' : 'Installing or removing packages changes the shared connection library. Enable or disable their commands for this bot in the Commands tab.'}</p>
          <ExtensionsTab fixedConfigId={query.data!.serverConfigId} />
        </TabsContent>
      </Tabs>}
    </DialogContent>
  </Dialog>;
}
