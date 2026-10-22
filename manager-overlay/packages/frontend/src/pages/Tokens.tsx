import { translate as tr, useTranslation as useUiTranslation } from '@/lib/i18n';
import { useMemo, useState, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { tokensApi } from '@/api/bans.api';
import { useServerStore } from '@/stores/server.store';
import { DataTable } from '@/components/shared/DataTable';
import { Button } from '@/components/ui/button';
import { PageLoader } from '@/components/shared/LoadingSpinner';
import { NoServerSelectedState } from '@/components/shared/NoServerSelectedState';
import { KeyRound, Trash2, Copy } from 'lucide-react';
import { type ColumnDef } from '@tanstack/react-table';
import { toast } from 'sonner';
import { copyText } from '@/lib/clipboard';
import { useTranslation } from '@/lib/i18n';
import { useServerGroups, useChannelGroups } from '@/hooks/use-groups';
import { useChannels } from '@/hooks/use-channels';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { apiErrorMessage } from '@/lib/api-error';

export default function Tokens() {
  useUiTranslation();
  const { t, language } = useTranslation();
  const { selectedConfigId: c, selectedSid: s } = useServerStore();
  const [creating, setCreating] = useState(false);
  const [keyType, setKeyType] = useState('0');
  const [groupId, setGroupId] = useState('');
  const [channelId, setChannelId] = useState('');
  const [description, setDescription] = useState('');
  const [createdKey, setCreatedKey] = useState('');
  const sg = useServerGroups(); const cg = useChannelGroups(); const channels = useChannels();
  useEffect(() => { setCreating(false); setCreatedKey(''); }, [c, s]);
  const createKey = useMutation({
    mutationFn: () => tokensApi.add(c!, s!, { tokentype: Number(keyType), tokenid1: Number(groupId),
      tokenid2: keyType === '1' ? Number(channelId) : 0, tokendescription: description.trim() }),
    onSuccess: (result) => {
      const token = (Array.isArray(result) ? result[0] : result)?.token;
      if (typeof token !== 'string' || !token) { toast.error(t('Could not create key')); return; }
      setCreatedKey(token); toast.success(t('Key created')); void qc.invalidateQueries({ queryKey: ['tokens', c, s] });
    },
  });
  const groupRows = (keyType === '0' ? sg.data : cg.data) ?? [];
  const { data, isLoading } = useQuery({ queryKey: ['tokens', c, s], queryFn: () => tokensApi.list(c!, s!), enabled: !!c && !!s });
  const history = useQuery({ queryKey: ['key-use-history', c, s], queryFn: () => tokensApi.history(c!, s!), enabled: !!c && !!s, refetchInterval: 5000 });
  const qc = useQueryClient();
  const capture = useMutation({ mutationFn: (target: { c: number; s: number; enabled: boolean }) => tokensApi.capture(target.c, target.s, target.enabled),
    onSuccess: (_data, target) => { void qc.invalidateQueries({ queryKey: ['key-use-history', target.c, target.s] }); },
    onError: error => toast.error(apiErrorMessage(error, t('Could not change key tracking'))) });
  const deleteToken = useMutation({ mutationFn: (token: string) => tokensApi.delete(c!, s!, token), onSuccess: () => qc.invalidateQueries({ queryKey: ['tokens'] }) });

  const tokens = useMemo(() => (Array.isArray(data) ? data : []), [data]);

  const columns: ColumnDef<any>[] = useMemo(() => [
    { accessorKey: 'token', header: t('Token'), cell: ({ getValue }) => (
      <div className="flex items-center gap-1">
        <span className="font-mono-data text-xs truncate max-w-[200px]">{getValue() as string}</span>
        <Button
          variant="ghost"
          size="icon"
          aria-label={t('Copy privilege key')}
          onClick={() => { void copyText(getValue() as string).then(() => toast.success(t('Copied')), () => toast.error(t('Could not copy. Select the text and copy it by hand.'))); }}
        >
          <Copy className="h-3 w-3 text-muted-foreground" />
        </Button>
      </div>
    )},
    { accessorKey: 'token_type', header: t('Type'), cell: ({ getValue }) => <span className="text-xs">{Number(getValue()) === 0 ? tr("Server Group") : tr("Channel Group")}</span> },
    { accessorKey: 'token_id1', header: t('Group ID'), cell: ({ getValue }) => <span className="font-mono-data text-xs">{getValue() as number}</span> },
    { accessorKey: 'token_fingerprint', header: t('Key fingerprint'), cell: ({ getValue }) => String(getValue() || '—').slice(0, 12) },
    { accessorKey: 'token_description', header: t('Description'), cell: ({ getValue }) => <span className="text-xs">{(getValue() as string) || '-'}</span> },
    { id: 'actions', header: () => <span className="sr-only"> {tr("Actions")}</span>, cell: ({ row }) => (
      <Button variant="ghost" size="icon" className="text-destructive hover:text-destructive" aria-label={tr("Delete privilege key")} onClick={() => deleteToken.mutate(row.original.token, { onSuccess: () => toast.success(tr("Token deleted")) })}>
        <Trash2 className="h-3.5 w-3.5" />
      </Button>
    )},
  ], [deleteToken.mutate, language]);

  if (!c || !s) return <NoServerSelectedState pageTitle={tr("Privilege Keys")} icon={KeyRound} />;
  if (isLoading) return <PageLoader />;

  return (
    <div className="space-y-5">
      <p className="rounded-md border bg-card p-3 text-sm text-muted-foreground">{t('Keys grant a group once and then disappear. Enable usage tracking below to record future redemptions. Missing keys alone do not prove who used them.')}</p>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <h1 className="text-xl font-semibold"> {tr("Privilege Keys")}</h1>
        <Button onClick={() => { setKeyType('0'); setGroupId(''); setChannelId(''); setDescription(''); setCreatedKey(''); createKey.reset(); setCreating(true); }}>{t('Create privilege key')}</Button>
      </div>
      <Dialog open={creating} onOpenChange={open => { if (!createKey.isPending) setCreating(open); }}>
        <DialogContent><DialogHeader><DialogTitle>{t('Create privilege key')}</DialogTitle></DialogHeader>
          {createdKey ? <div className="space-y-3">
            <Input aria-label={t('Token')} readOnly value={createdKey} onFocus={event => event.target.select()} />
            <Button onClick={() => { void copyText(createdKey).then(() => toast.success(t('Copied')), () => toast.error(t('Could not copy. Select the text and copy it by hand.'))); }}>{t('Copy')}</Button>
          </div> : <form className="space-y-3" onSubmit={event => { event.preventDefault(); if (groupId && (keyType === '0' || channelId)) createKey.mutate(); }}>
            <Label htmlFor="key-type">{t('Type')}</Label>
            <select id="key-type" className="w-full rounded-md border bg-background p-2" value={keyType} onChange={event => { setKeyType(event.target.value); setGroupId(''); }}>
              <option value="0">{t('Server Group')}</option><option value="1">{t('Channel Group')}</option>
            </select>
            <Label htmlFor="key-group">{t('Group')}</Label>
            <select id="key-group" className="w-full rounded-md border bg-background p-2" value={groupId} onChange={event => setGroupId(event.target.value)} required>
              <option value="">{t('Choose group')}</option>{Array.isArray(groupRows) && groupRows.map((group: any) => <option key={group.sgid ?? group.cgid} value={group.sgid ?? group.cgid}>{group.name}</option>)}
            </select>
            {keyType === '1' && <><Label htmlFor="key-channel">{t('Target channel')}</Label>
              <select id="key-channel" className="w-full rounded-md border bg-background p-2" value={channelId} onChange={event => setChannelId(event.target.value)} required>
                <option value="">{t('Select channel')}</option>{Array.isArray(channels.data) && channels.data.map((channel: any) => <option key={channel.cid} value={channel.cid}>{channel.channel_name}</option>)}
              </select></>}
            <Label htmlFor="key-description">{t('Description')}</Label><Input id="key-description" value={description} maxLength={255} onChange={event => setDescription(event.target.value)} />
            {createKey.isError && <p role="alert" className="text-sm text-destructive">{apiErrorMessage(createKey.error, t('Could not create key'))}</p>}
            <div className="flex justify-end gap-2"><Button type="button" variant="outline" onClick={() => setCreating(false)} disabled={createKey.isPending}>{t('Cancel')}</Button>
              <Button type="submit" disabled={createKey.isPending || !groupId || (keyType === '1' && !channelId)}>{t('Create')}</Button></div>
          </form>}
        </DialogContent>
      </Dialog>
      <p className="text-xs text-muted-foreground">{t('The table shows currently available keys, not a usage history. Never share a Server Admin key publicly.')}</p>
      <DataTable
        columns={columns}
        data={tokens}
        searchEnabled
        searchLabel={tr("Search privilege keys")}
        searchPlaceholder={tr("Search tokens...")}
        tableLabel={tr("Privilege keys table")}
        density="compact"
        stickyHeader
        emptyText={tr("No privilege keys found")}
        filteredEmptyText={tr("No privilege keys match your search")}
      />
      <section className="space-y-3 rounded-md border bg-card p-4" aria-label={t('Key usage history')}>
        <div className="flex flex-wrap items-center justify-between gap-2"><h2 className="font-semibold">{t('Key usage history')}</h2>
          <Button disabled={capture.isPending || history.isLoading || history.isError} onClick={() => capture.mutate({ c, s, enabled: !history.data?.enabled })}>{t(history.data?.enabled ? 'Stop key tracking' : 'Start key tracking')}</Button></div>
        <p className="text-sm text-muted-foreground">{t('Records observed key redemptions from now on. No raw keys are stored. Retention: 30 days / 200 events per server. Earlier usage cannot be reconstructed.')}</p>
        {history.isError ? <p role="alert">{apiErrorMessage(history.error, t('Could not load key usage history'))}</p> : <>
          <p role="status" className="text-sm">{t('Status')}: {t(history.data?.status === 'capturing' ? 'Capturing' : history.data?.status === 'connecting' ? 'Waiting for Query notifications' : history.data?.status === 'gap' ? 'Capture has gaps — check Query connection' : 'Disabled')}</p>
          <DataTable columns={[
            { accessorKey: 'observedAt', header: t('Date'), cell: ({ getValue }) => new Date(String(getValue())).toLocaleString(language === 'tr' ? 'tr-TR' : 'en-GB') },
            { accessorKey: 'fingerprint', header: t('Key fingerprint'), cell: ({ getValue }) => String(getValue()).slice(0, 12) },
            { accessorKey: 'nickname', header: t('User'), cell: ({ row }: { row: { original: { nickname?: string; clientId: number } } }) => row.original.nickname || `#${row.original.clientId}` },
            { accessorKey: 'uniqueId', header: 'UID' }, { accessorKey: 'databaseId', header: t('Database ID') },
          ]} data={Array.isArray(history.data?.items) ? history.data.items : []} emptyText={t('No observed key redemptions yet')} />
        </>}
      </section>
    </div>
  );
}
