import { translate as tr, useTranslation as useUiTranslation } from '@/lib/i18n';
/**
 * One simple screen to run music bots, in the spirit of SinusBot: pick a bot, see what is
 * playing and who asked for it, control playback and volume, search or paste a link, and
 * decide what happens when the queue ends. Everything else stays in the other pages.
 */

import { useEffect, useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import axios from 'axios';
import {
  Music2, Pause, Play, Plus, Power, PowerOff, Radio, Repeat, Search, Settings2, SkipBack, SkipForward, Square, Trash2, VolumeX, Volume2,
} from 'lucide-react';
import { Link } from 'react-router-dom';
import { DEFAULT_BOT_AUTOMATION_SETTINGS, type BotAutomationSettings, type MusicBotSummary } from '@ts6/common';
import { musicBotsApi, radioStationsApi, type BotSearchResult } from '@/api/music.api';
import { useMusicBots, useMusicBotState, useStartMusicBot, useStopMusicBot } from '@/hooks/use-music-bots';
import { BotSettingsMenu } from '@/pages/bot-hub/BotSettingsMenu';
import { PageHeader } from '@/components/shared/PageHeader';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Slider } from '@/components/ui/slider';
import { Badge } from '@/components/ui/badge';
import { BotFormDialog } from '@/pages/bot-hub/BotFormDialog';

type QueueEnd = 'quiet' | 'radio' | 'repeat';

function clock(seconds: number | undefined): string {
  if (!seconds || !Number.isFinite(seconds) || seconds <= 0) return '0:00';
  const s = Math.floor(seconds);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

function timeOfDay(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? '' : d.toLocaleTimeString('tr-TR', { hour: '2-digit', minute: '2-digit' });
}

function errorText(err: unknown, fallback: string): string {
  return axios.isAxiosError(err) && typeof err.response?.data?.error === 'string' ? tr(err.response.data.error) : tr(fallback);
}

const isLink = (text: string) => /^https?:\/\//i.test(text.trim());

function statusLabel(status: string): string {
  if (status === 'playing') return tr('Playing');
  if (status === 'paused') return tr('Paused');
  if (status === 'connected') return tr('Idle');
  if (status === 'starting') return tr('Connecting');
  if (status === 'error') return tr('Error');
  return tr('Offline');
}

function where(bot: MusicBotSummary): string {
  return bot.externalHost || bot.serverConfig?.name || '';
}

export default function BotControl() {
  useUiTranslation();
  const queryClient = useQueryClient();
  const { data: bots } = useMusicBots();
  const botList: MusicBotSummary[] = Array.isArray(bots) ? bots : [];
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const bot = botList.find((b) => b.id === selectedId) ?? botList[0] ?? null;
  const botId = bot?.id ?? null;
  useEffect(() => { if (selectedId === null && botList[0]) setSelectedId(botList[0].id); }, [selectedId, botList]);

  const { data: state } = useMusicBotState(botId);
  const { data: activity } = useQuery({
    queryKey: ['music-bot-activity', botId],
    queryFn: () => musicBotsApi.activity(botId!),
    enabled: !!botId,
    refetchInterval: 3000,
  });
  const { data: detail } = useQuery({
    queryKey: ['music-bot', botId],
    queryFn: () => musicBotsApi.get(botId!),
    enabled: !!botId,
  });
  const { data: stations } = useQuery({
    queryKey: ['radio-stations', bot?.serverConfigId],
    queryFn: () => radioStationsApi.list(bot!.serverConfigId),
    enabled: !!bot?.serverConfigId,
  });
  const startBot = useStartMusicBot();
  const stopBot = useStopMusicBot();

  const [volume, setVolume] = useState<number | null>(null);
  useEffect(() => { setVolume(null); }, [botId]);
  const shownVolume = volume ?? (typeof state?.volume === 'number' ? state.volume : bot?.volume ?? 50);

  const [query, setQuery] = useState('');
  const [searching, setSearching] = useState(false);
  const [results, setResults] = useState<BotSearchResult[]>([]);
  const [searchNote, setSearchNote] = useState('');

  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ['music-bot-state', botId] });
    void queryClient.invalidateQueries({ queryKey: ['music-bot-activity', botId] });
  };
  const run = async (action: () => Promise<unknown>, done?: string, failed = 'That did not work. Try again.') => {
    try { await action(); if (done) toast.success(tr(done)); }
    catch (err) { toast.error(errorText(err, failed)); }
    finally { refresh(); }
  };

  const automation: BotAutomationSettings = { ...DEFAULT_BOT_AUTOMATION_SETTINGS, ...(detail?.automationSettings ?? {}) };
  const queueEnd: QueueEnd = state?.repeat === 'queue' ? 'repeat'
    : automation.enabled && automation.source?.kind === 'radio' ? 'radio' : 'quiet';
  const radioId = automation.source?.kind === 'radio' ? automation.source.id : (Array.isArray(stations) ? stations[0]?.id : undefined);

  const setQueueEnd = (next: QueueEnd) => {
    if (!botId || next === queueEnd) return;
    void run(async () => {
      if (next === 'radio' && !radioId) throw new Error('no radio');
      if (state?.repeat === 'queue' && next !== 'repeat') await musicBotsApi.repeat(botId, 'off');
      if (next === 'repeat') await musicBotsApi.repeat(botId, 'queue');
      await musicBotsApi.automation(botId, next === 'radio'
        ? { ...automation, enabled: true, source: { kind: 'radio', id: radioId! } }
        : { ...automation, enabled: false });
      await queryClient.invalidateQueries({ queryKey: ['music-bot', botId] });
    }, 'Saved', next === 'radio' && !radioId ? 'Add a radio station first.' : 'Could not save. Try again.');
  };

  const doSearch = async () => {
    const text = query.trim();
    if (!botId || text.length < 2) return;
    if (isLink(text)) { setResults([]); setSearchNote(''); return; }
    setSearching(true); setSearchNote('');
    try {
      const found = await musicBotsApi.search(botId, text);
      setResults(found.results);
      if (!found.results.length) setSearchNote(found.cookies ? tr('No results.') : tr('YouTube cookies are missing. Upload them in Settings → YouTube.'));
    } catch (err) {
      setResults([]); setSearchNote(errorText(err, 'Search failed. Try again.'));
    } finally { setSearching(false); }
  };

  const queue = Array.isArray(state?.queue) ? state.queue : [];
  const upcoming = useMemo(
    () => queue.map((item: any, index: number) => ({ item, index })).filter(({ index }: { index: number }) => index > (state?.currentIndex ?? -1)),
    [queue, state?.currentIndex],
  );
  const nowPlaying = state?.nowPlaying ?? null;
  const status = state?.status ?? bot?.status ?? 'stopped';
  const online = status !== 'stopped' && status !== 'error';
  // Radio started by Auto-DJ has no requester; only credit someone for the same kind of request.
  const playingRadio = nowPlaying?.source === 'radio';
  const lastRequest = (activity ?? []).find((e) => (playingRadio ? /^!radio\b/ : /^!(play|add|ara sonucu)/).test(e.action));

  return (
    <div className="space-y-4">
      <PageHeader
        title={tr('Bot control')}
        description={tr('Play music, control the bots and see who did what.')}
        actions={<Button variant="outline" asChild><Link to={botId ? `/bot-hub/${botId}` : '/bot-hub'}><Settings2 className="mr-1 h-4 w-4" aria-hidden="true" />{tr('Advanced bot settings')}</Link></Button>}
      />
      <div className="grid gap-4 lg:grid-cols-[240px_minmax(0,1fr)]">
        <div className="space-y-2" role="list" aria-label={tr('Bots')}>
          {botList.map((b) => (
            <button
              key={b.id} type="button" role="listitem" onClick={() => setSelectedId(b.id)}
              aria-current={b.id === botId}
              className={`w-full rounded-md border p-3 text-left transition-colors ${b.id === botId ? 'border-primary bg-primary/10' : 'hover:bg-muted'}`}
            >
              <div className="font-medium">{b.name}</div>
              <div className="text-xs text-muted-foreground">{statusLabel(b.status)}{where(b) ? ` · ${where(b)}` : ''}</div>
            </button>
          ))}
          <Button variant="outline" className="w-full" onClick={() => setCreateOpen(true)}><Plus className="mr-1 h-4 w-4" aria-hidden="true" />{tr('Add bot')}</Button>
        </div>

        {!bot ? (
          <div className="rounded-md border p-6 text-sm text-muted-foreground">{tr('No bots yet. Add one to start playing music.')}</div>
        ) : (
          <div className="space-y-4 min-w-0">
            <section className="rounded-md border p-4" aria-label={tr('Now playing')}>
              <div className="flex items-start gap-3">
                <div className="flex h-14 w-14 shrink-0 items-center justify-center rounded-md bg-muted"><Music2 className="h-6 w-6 text-muted-foreground" aria-hidden="true" /></div>
                <div className="min-w-0 flex-1">
                  <div className="truncate font-medium">{nowPlaying ? `${nowPlaying.artist ? `${nowPlaying.artist} - ` : ''}${nowPlaying.title}` : tr('Nothing is playing')}</div>
                  <div className="text-xs text-muted-foreground">
                    {statusLabel(status)}
                    {nowPlaying && lastRequest ? ` · ${tr('Asked by')}: ${lastRequest.who}` : ''}
                  </div>
                  {nowPlaying && (
                    <>
                      <div className="mt-2 h-1 rounded bg-muted">
                        <div className="h-1 rounded bg-primary" style={{ width: `${state?.duration ? Math.min(100, Math.round((state.position / state.duration) * 100)) : 0}%` }} />
                      </div>
                      <div className="mt-1 text-xs text-muted-foreground">{clock(state?.position)} / {clock(state?.duration)}</div>
                    </>
                  )}
                </div>
                <div className="flex shrink-0 items-center gap-2">
                  {online ? (
                    <Button size="sm" variant="outline" onClick={() => stopBot.mutate(bot.id, { onError: (err) => toast.error(errorText(err, 'Failed to stop bot')) })}>
                      <PowerOff className="mr-1 h-4 w-4" aria-hidden="true" />{tr('Stop bot')}
                    </Button>
                  ) : (
                    <Button size="sm" onClick={() => startBot.mutate(bot.id, { onError: (err) => toast.error(errorText(err, 'Could not start the bot.')) })}>
                      <Power className="mr-1 h-4 w-4" aria-hidden="true" />{tr('Start bot')}
                    </Button>
                  )}
                  {/* Edit, Auto-DJ, extensions, widget link and delete, as in Bot Hub. */}
                  <BotSettingsMenu botId={bot.id} botName={bot.name} status={status} onDeleted={() => setSelectedId(null)} />
                </div>
              </div>

              <div className="mt-4 flex flex-wrap items-center gap-2">
                <Button variant="outline" size="icon" aria-label={tr('Previous')} disabled={!online} onClick={() => run(() => musicBotsApi.previous(bot.id))}><SkipBack className="h-4 w-4" /></Button>
                {status === 'paused'
                  ? <Button variant="outline" size="icon" aria-label={tr('Resume')} onClick={() => run(() => musicBotsApi.resume(bot.id))}><Play className="h-4 w-4" /></Button>
                  : <Button variant="outline" size="icon" aria-label={tr('Pause')} disabled={status !== 'playing'} onClick={() => run(() => musicBotsApi.pause(bot.id))}><Pause className="h-4 w-4" /></Button>}
                <Button variant="outline" size="icon" aria-label={tr('Next')} disabled={!online} onClick={() => run(() => musicBotsApi.skip(bot.id))}><SkipForward className="h-4 w-4" /></Button>
                <Button variant="outline" size="icon" aria-label={tr('Stop')} disabled={!online} onClick={() => run(() => musicBotsApi.stopPlayback(bot.id))}><Square className="h-4 w-4" /></Button>
                <div className="ml-2 flex min-w-[180px] flex-1 items-center gap-2">
                  {shownVolume === 0 ? <VolumeX className="h-4 w-4 text-muted-foreground" aria-hidden="true" /> : <Volume2 className="h-4 w-4 text-muted-foreground" aria-hidden="true" />}
                  <Slider
                    aria-label={tr('Volume')} value={[shownVolume]} max={100} step={1}
                    onValueChange={([v]) => setVolume(v)}
                    onValueCommit={([v]) => run(() => musicBotsApi.volume(bot.id, v)).then(() => setVolume(null))}
                  />
                  <span className="w-10 text-right text-sm tabular-nums">{shownVolume}%</span>
                </div>
              </div>

              <div className="mt-4 flex flex-wrap items-center gap-2 text-sm" role="radiogroup" aria-label={tr('When the queue ends')}>
                <span className="text-muted-foreground">{tr('When the queue ends')}:</span>
                {([['quiet', 'Stay quiet', Square], ['radio', 'Play radio', Radio], ['repeat', 'Repeat the queue', Repeat]] as const).map(([value, label, Icon]) => (
                  <Button
                    key={value} size="sm" variant="outline" role="radio" aria-checked={queueEnd === value}
                    className={queueEnd === value ? 'border-primary bg-primary/15 text-primary hover:bg-primary/20 hover:text-primary' : ''}
                    onClick={() => setQueueEnd(value)}
                  ><Icon className="mr-1 h-3.5 w-3.5" aria-hidden="true" />{tr(label)}</Button>
                ))}
              </div>
            </section>

            <section className="rounded-md border p-4" aria-label={tr('Search')}>
              <form className="flex gap-2" onSubmit={(e) => { e.preventDefault(); void doSearch(); }}>
                <Input
                  value={query} onChange={(e) => setQuery(e.target.value)} aria-label={tr('Song name or link')}
                  placeholder={tr('Song name, YouTube or Spotify link')}
                />
                {isLink(query) ? (
                  <>
                    <Button type="button" onClick={() => run(() => musicBotsApi.playUrl(bot.id, query.trim()), 'Playing')}>{tr('Play')}</Button>
                    <Button type="button" variant="outline" onClick={() => run(() => musicBotsApi.playUrl(bot.id, query.trim(), { enqueue: true }), 'Added to queue')}>{tr('Add to queue')}</Button>
                  </>
                ) : (
                  <Button type="submit" disabled={searching || query.trim().length < 2}><Search className="mr-1 h-4 w-4" aria-hidden="true" />{searching ? tr('Searching…') : tr('Search')}</Button>
                )}
              </form>
              {searchNote && <p className="mt-2 text-sm text-muted-foreground">{searchNote}</p>}
              {results.length > 0 && (
                <ul className="mt-3 divide-y" aria-label={tr('Search results')}>
                  {results.map((r) => (
                    <li key={r.url} className="flex items-center justify-between gap-2 py-2 text-sm">
                      <span className="min-w-0 truncate">
                        {r.artist} - {r.title} <span className="text-muted-foreground">· {r.source}{r.duration ? ` · ${clock(r.duration)}` : ''}</span>
                      </span>
                      <span className="flex shrink-0 gap-1">
                        <Button size="sm" onClick={() => run(() => musicBotsApi.playUrl(bot.id, r.url), 'Playing')}>{tr('Play')}</Button>
                        <Button size="sm" variant="outline" onClick={() => run(() => musicBotsApi.playUrl(bot.id, r.url, { enqueue: true }), 'Added to queue')}>{tr('Add to queue')}</Button>
                      </span>
                    </li>
                  ))}
                </ul>
              )}
            </section>

            <div className="grid gap-4 xl:grid-cols-2">
              <section className="rounded-md border p-4" aria-label={tr('Up next')}>
                <h2 className="mb-2 text-sm font-medium">{tr('Up next')} ({upcoming.length})</h2>
                {upcoming.length === 0 ? <p className="text-sm text-muted-foreground">{tr('The queue is empty.')}</p> : (
                  <ul className="space-y-1 text-sm">
                    {upcoming.map(({ item, index }: { item: any; index: number }, position: number) => (
                      <li key={item.id} className="flex items-center justify-between gap-2">
                        <span className="min-w-0 truncate">{position + 1}. {item.artist ? `${item.artist} - ` : ''}{item.title}</span>
                        <Button size="icon" variant="ghost" aria-label={tr('Remove')} onClick={() => run(() => musicBotsApi.removeFromQueue(bot.id, index, item.id))}><Trash2 className="h-4 w-4" /></Button>
                      </li>
                    ))}
                  </ul>
                )}
              </section>

              <section className="rounded-md border p-4" aria-label={tr('Who did what')}>
                <h2 className="mb-2 text-sm font-medium">{tr('Who did what')}</h2>
                {!activity?.length ? <p className="text-sm text-muted-foreground">{tr('Nothing recorded yet.')}</p> : (
                  <ul className="space-y-1 text-sm">
                    {activity.slice(0, 15).map((entry, i) => (
                      <li key={`${entry.at}-${i}`} className="flex items-baseline gap-2">
                        <span className="w-11 shrink-0 tabular-nums text-muted-foreground">{timeOfDay(entry.at)}</span>
                        <span className="font-medium">{entry.who}</span>
                        <span className="min-w-0 truncate">{entry.action}</span>
                        <Badge variant="outline" className="ml-auto shrink-0">{entry.via === 'panel' ? tr('Panel') : tr('TeamSpeak')}</Badge>
                      </li>
                    ))}
                  </ul>
                )}
              </section>
            </div>
          </div>
        )}
      </div>
      <BotFormDialog open={createOpen} bot={null} defaultServerId={bot?.serverConfigId ?? null} onClose={() => setCreateOpen(false)} />
    </div>
  );
}
