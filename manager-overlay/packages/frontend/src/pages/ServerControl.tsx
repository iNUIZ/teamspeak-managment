import { translate as tr, useTranslation as useUiTranslation } from '@/lib/i18n';
/**
 * One simple screen to run the selected TeamSpeak server: every channel with the people in
 * it, and the everyday actions on a person (move, poke, message, kick, ban). The detailed
 * Channels, Clients and Permissions pages stay for everything else.
 */

import { useMemo, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import axios from 'axios';
import { Ban, Hand, MessageSquare, MoveRight, Server, UserX, Users } from 'lucide-react';
import { channelsApi } from '@/api/channels.api';
import { clientsApi } from '@/api/clients.api';
import { useServerStore } from '@/stores/server.store';
import { PageHeader } from '@/components/shared/PageHeader';
import { NoServerSelectedState } from '@/components/shared/NoServerSelectedState';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Dialog, DialogContent, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

interface Channel { cid: number; pid: number; name: string; depth: number }
interface Person { clid: number; cid: number; nickname: string; away: boolean; muted: boolean }
type Action = 'move' | 'poke' | 'message' | 'kick' | 'ban';

const ACTIONS: Array<{ id: Action; label: string; icon: typeof MoveRight; needsText: boolean; danger?: boolean }> = [
  { id: 'move', label: 'Move', icon: MoveRight, needsText: false },
  { id: 'poke', label: 'Poke', icon: Hand, needsText: true },
  { id: 'message', label: 'Message', icon: MessageSquare, needsText: true },
  { id: 'kick', label: 'Kick', icon: UserX, needsText: false, danger: true },
  { id: 'ban', label: 'Ban', icon: Ban, needsText: false, danger: true },
];

/** Channels in display order with their depth, children right after their parent. */
function flattenChannels(raw: any[]): Channel[] {
  const channels = raw.map((c) => ({ cid: Number(c.cid), pid: Number(c.pid) || 0, name: String(c.channel_name ?? '') }));
  const byParent = new Map<number, typeof channels>();
  for (const c of channels) byParent.set(c.pid, [...(byParent.get(c.pid) ?? []), c]);
  const out: Channel[] = [];
  const known = new Set(channels.map((c) => c.cid));
  const walk = (pid: number, depth: number) => {
    for (const c of byParent.get(pid) ?? []) { out.push({ ...c, depth }); walk(c.cid, depth + 1); }
  };
  walk(0, 0);
  // A channel whose parent is missing from the list still shows, at the top level.
  for (const c of channels) if (c.pid !== 0 && !known.has(c.pid) && !out.some((o) => o.cid === c.cid)) out.push({ ...c, depth: 0 });
  return out;
}

function errorText(err: unknown, fallback: string): string {
  return axios.isAxiosError(err) && typeof err.response?.data?.error === 'string' ? tr(err.response.data.error) : tr(fallback);
}

export default function ServerControl() {
  useUiTranslation();
  const queryClient = useQueryClient();
  const { selectedConfigId: configId, selectedSid: sid } = useServerStore();
  const enabled = !!configId && !!sid;
  const channelsQuery = useQuery({ queryKey: ['channels', configId, sid], queryFn: () => channelsApi.list(configId!, sid!), enabled, refetchInterval: 10000 });
  const clientsQuery = useQuery({ queryKey: ['clients', configId, sid], queryFn: () => clientsApi.list(configId!, sid!), enabled, refetchInterval: 5000 });

  const channels = useMemo(() => flattenChannels(Array.isArray(channelsQuery.data) ? channelsQuery.data : []), [channelsQuery.data]);
  const people: Person[] = useMemo(() => (Array.isArray(clientsQuery.data) ? clientsQuery.data : [])
    .filter((c: any) => String(c.client_type) !== '1')
    .map((c: any) => ({
      clid: Number(c.clid), cid: Number(c.cid), nickname: String(c.client_nickname ?? ''),
      away: String(c.client_away) === '1', muted: String(c.client_input_muted) === '1' || String(c.client_output_muted) === '1',
    })), [clientsQuery.data]);

  const [target, setTarget] = useState<Person | null>(null);
  const [action, setAction] = useState<Action>('move');
  const [text, setText] = useState('');
  const [destination, setDestination] = useState('');
  const [busy, setBusy] = useState(false);

  if (!enabled) return <NoServerSelectedState pageTitle={tr('Server control')} icon={Server} />;

  const open = (person: Person) => { setTarget(person); setAction('move'); setText(''); setDestination(''); };
  const current = ACTIONS.find((a) => a.id === action)!;
  const textMissing = current.needsText && !text.trim();

  const submit = async () => {
    if (!target || textMissing || (action === 'move' && !destination)) return;
    setBusy(true);
    try {
      if (action === 'move') await clientsApi.move(configId!, sid!, target.clid, Number(destination));
      if (action === 'poke') await clientsApi.poke(configId!, sid!, target.clid, text.trim());
      if (action === 'message') await clientsApi.message(configId!, sid!, target.clid, text.trim());
      // 5 = kick from the server (4 would only move them to the default channel).
      if (action === 'kick') await clientsApi.kick(configId!, sid!, target.clid, 5, text.trim() || undefined);
      if (action === 'ban') await clientsApi.ban(configId!, sid!, target.clid, 0, text.trim() || undefined);
      toast.success(tr('Done'));
      setTarget(null);
    } catch (err) {
      toast.error(errorText(err, 'That did not work. Try again.'));
    } finally {
      setBusy(false);
      void queryClient.invalidateQueries({ queryKey: ['clients', configId, sid] });
    }
  };

  const loading = channelsQuery.isLoading || clientsQuery.isLoading;
  return (
    <div className="space-y-4">
      <PageHeader
        title={tr('Server control')}
        description={tr('Everyone on the server, by channel. Click a name to move, poke, message, kick or ban.')}
        actions={<span className="flex items-center gap-1 text-sm text-muted-foreground"><Users className="h-4 w-4" aria-hidden="true" />{people.length} {tr('online')}</span>}
      />
      {loading ? <p className="text-sm text-muted-foreground">{tr('Loading…')}</p> : (
        <section className="rounded-md border p-2" aria-label={tr('Channels and people')}>
          {channels.map((channel) => {
            const inside = people.filter((p) => p.cid === channel.cid);
            return (
              <div key={channel.cid} style={{ paddingLeft: `${channel.depth * 16}px` }}>
                <div className="flex items-center gap-2 px-2 py-1 text-sm font-medium">
                  <span className="truncate">{channel.name || `#${channel.cid}`}</span>
                  {inside.length > 0 && <span className="text-xs text-muted-foreground">({inside.length})</span>}
                </div>
                {inside.map((person) => (
                  <button
                    key={person.clid} type="button" onClick={() => open(person)}
                    className="ml-4 flex w-[calc(100%-1rem)] items-center gap-2 rounded px-2 py-1 text-left text-sm hover:bg-muted"
                  >
                    <span className="truncate">{person.nickname}</span>
                    {person.away && <span className="text-xs text-muted-foreground">{tr('away')}</span>}
                    {person.muted && <span className="text-xs text-muted-foreground">{tr('muted')}</span>}
                  </button>
                ))}
              </div>
            );
          })}
          {channels.length === 0 && <p className="p-2 text-sm text-muted-foreground">{tr('No channels found.')}</p>}
        </section>
      )}

      <Dialog open={!!target} onOpenChange={(next) => { if (!next && !busy) setTarget(null); }}>
        <DialogContent>
          <DialogHeader><DialogTitle>{target?.nickname}</DialogTitle></DialogHeader>
          <div className="flex flex-wrap gap-2" role="radiogroup" aria-label={tr('Action')}>
            {ACTIONS.map(({ id, label, icon: Icon, danger }) => (
              <Button
                key={id} size="sm" variant="outline" role="radio" aria-checked={action === id}
                className={action === id ? (danger ? 'border-destructive bg-destructive/15 text-destructive' : 'border-primary bg-primary/15 text-primary') : ''}
                onClick={() => setAction(id)}
              ><Icon className="mr-1 h-3.5 w-3.5" aria-hidden="true" />{tr(label)}</Button>
            ))}
          </div>
          {action === 'move' ? (
            <Select value={destination} onValueChange={setDestination}>
              <SelectTrigger aria-label={tr('Channel')}><SelectValue placeholder={tr('Choose a channel')} /></SelectTrigger>
              <SelectContent>
                {channels.filter((c) => c.cid !== target?.cid).map((c) => (
                  <SelectItem key={c.cid} value={String(c.cid)}>{`${'— '.repeat(c.depth)}${c.name || `#${c.cid}`}`}</SelectItem>
                ))}
              </SelectContent>
            </Select>
          ) : (
            <Input
              value={text} onChange={(e) => setText(e.target.value)} maxLength={action === 'poke' ? 100 : 1024}
              aria-label={current.needsText ? tr('Text') : tr('Reason (optional)')}
              placeholder={current.needsText ? tr('Text') : tr('Reason (optional)')}
            />
          )}
          {action === 'ban' && <p className="text-xs text-destructive">{tr('A ban without an end date stays until you remove it under Bans.')}</p>}
          <DialogFooter>
            <Button variant="outline" onClick={() => setTarget(null)} disabled={busy}>{tr('Cancel')}</Button>
            <Button
              variant={current.danger ? 'destructive' : 'default'} onClick={() => void submit()}
              disabled={busy || textMissing || (action === 'move' && !destination)}
            >{tr(current.label)}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
