import { useMemo, useState, useEffect } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { messagesApi } from '@/api/bans.api';
import { clientsApi } from '@/api/clients.api';
import { useServerStore } from '@/stores/server.store';
import { DataTable } from '@/components/shared/DataTable';
import { PageLoader } from '@/components/shared/LoadingSpinner';
import { NoServerSelectedState } from '@/components/shared/NoServerSelectedState';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Mail, Plus, Trash2, Eye } from 'lucide-react';
import { type ColumnDef } from '@tanstack/react-table';
import { timeAgo } from '@/lib/utils';
import { toast } from 'sonner';
import { useTranslation } from '@/lib/i18n';
import { apiErrorMessage } from '@/lib/api-error';

export default function Messages() {
  const { t, language } = useTranslation();
  const { selectedConfigId: c, selectedSid: s } = useServerStore();
  const qc = useQueryClient();
  const [compose, setCompose] = useState(false);
  const [view, setView] = useState<any>(null);
  const [mode, setMode] = useState<'chat' | 'offline'>('chat');
  const [recipient, setRecipient] = useState('');
  const [uid, setUid] = useState('');
  const [subject, setSubject] = useState('');
  const [body, setBody] = useState('');
  useEffect(() => { setCompose(false); setView(null); setRecipient(''); setUid(''); setBody(''); setSubject(''); }, [c, s]);
  const inbox = useQuery({ queryKey: ['messages', c, s], queryFn: () => messagesApi.list(c!, s!), enabled: !!c && !!s });
  const clients = useQuery({ queryKey: ['message-recipients', c, s], queryFn: () => clientsApi.list(c!, s!), enabled: !!c && !!s && compose });
  const sender = useQuery({ queryKey: ['message-sender', c, s], queryFn: () => messagesApi.sender(c!, s!), enabled: !!c && !!s && compose });
  const detail = useQuery({ queryKey: ['message-detail', c, s, view?.msgid], queryFn: () => messagesApi.get(c!, s!, view.msgid), enabled: !!c && !!s && !!view });
  const send = useMutation({
    mutationFn: (target: { configId: number; sid: number; payload: any }) => messagesApi.send(target.configId, target.sid, target.payload),
    onSuccess: (_data, target) => {
      toast.success(t('TeamSpeak accepted the message. This is not a read receipt.'));
      if (useServerStore.getState().selectedConfigId === target.configId && useServerStore.getState().selectedSid === target.sid) { setCompose(false); setBody(''); setSubject(''); }
    },
  });
  const remove = useMutation({
    mutationFn: (target: { c: number; s: number; id: number }) => messagesApi.delete(target.c, target.s, target.id),
    onSuccess: (_result, target) => {
      qc.setQueryData(['messages', target.c, target.s], (rows: any) => Array.isArray(rows) ? rows.filter(row => String(row.msgid) !== String(target.id)) : rows);
      toast.success(t('Message deleted'));
    },
    onError: error => toast.error(apiErrorMessage(error, t('Could not delete message'))),
  });
  const online = (Array.isArray(clients.data) ? clients.data : []).filter((client: any) => Number(client.client_type || 0) === 0);
  const rows = Array.isArray(inbox.data) ? inbox.data : [];
  const columns: ColumnDef<any>[] = useMemo(() => [
    { accessorKey: 'cluid', header: t('Sender UID'), cell: ({ row }) => row.original.senderName || row.original.cluid || '—' },
    { accessorKey: 'subject', header: t('Subject') },
    { accessorKey: 'timestamp', header: t('Date'), cell: ({ getValue }) => timeAgo(Number(getValue())) },
    { accessorKey: 'flag_read', header: t('Status'), cell: ({ getValue }) => t(Number(getValue()) === 1 ? 'Read' : 'Unread') },
    { id: 'actions', header: t('Actions'), cell: ({ row }) => <div className="flex gap-1">
      <Button variant="ghost" size="icon" aria-label={t('View message')} onClick={() => setView(row.original)}><Eye className="h-4 w-4" /></Button>
      <Button variant="ghost" size="icon" aria-label={t('Delete message')} disabled={remove.isPending} onClick={() => c && s && remove.mutate({ c, s, id: row.original.msgid })}><Trash2 className="h-4 w-4" /></Button>
    </div> },
  ], [language, c, s, remove.isPending]);
  if (!c || !s) return <NoServerSelectedState pageTitle={t('Messages')} icon={Mail} />;
  const content = (Array.isArray(detail.data) ? detail.data[0] : detail.data)?.message;
  return <div className="space-y-4">
    <div className="flex items-center justify-between gap-2"><h1 className="text-xl font-semibold">{t('Messages')}</h1><Button onClick={() => { send.reset(); setCompose(true); }}><Plus className="mr-2 h-4 w-4" />{t('Compose')}</Button></div>
    <p className="text-sm text-muted-foreground">{t('Online chat appears in the recipient’s TeamSpeak private chat. Offline messages go to their TeamSpeak mailbox, not the Manager inbox. The sender is the configured Query account.')}</p>
    <h2 className="font-medium">{t('Query account inbox')}</h2>
    <p className="text-xs text-muted-foreground">{t('This list contains messages received by the connected Query account, not all users’ inboxes or sent messages. Client support for offline mail can vary.')}</p>
    {inbox.isLoading ? <PageLoader /> : inbox.isError ? <p role="alert">{apiErrorMessage(inbox.error, t('Could not load inbox'))}</p> : <DataTable columns={columns} data={rows} searchEnabled searchPlaceholder={t('Search messages...')} emptyText={t('No offline messages found')} />}
    <Dialog open={compose} onOpenChange={open => { if (!send.isPending) setCompose(open); }}><DialogContent><DialogHeader><DialogTitle>{t('Compose Message')}</DialogTitle></DialogHeader>
      <p className="text-xs text-muted-foreground">{t('Sender')}: {sender.data?.nickname || sender.data?.loginName || t('Configured Query account')}</p>
      <form className="space-y-3" onSubmit={event => { event.preventDefault(); send.mutate({ configId: c, sid: s, payload: mode === 'chat' ? { mode, clid: Number(recipient), message: body, ...(uid && { expectedUid: uid }) } : { mode, cluid: uid.trim(), subject, message: body } }); }}>
        <Label htmlFor="delivery-mode">{t('Delivery method')}</Label><select id="delivery-mode" className="w-full rounded-md border bg-background p-2" value={mode} onChange={event => { setMode(event.target.value as 'chat' | 'offline'); setRecipient(''); setUid(''); send.reset(); }}>
          <option value="chat">{t('Private chat (online)')}</option><option value="offline">{t('Offline mailbox (UID)')}</option>
        </select>
        <Label htmlFor="message-recipient">{t('Recipient')}</Label><select id="message-recipient" className="w-full rounded-md border bg-background p-2" value={recipient} onChange={event => { setRecipient(event.target.value); const client = online.find((row: any) => String(row.clid) === event.target.value); setUid(client?.client_unique_identifier || ''); }} required={mode === 'chat'}>
          <option value="">{t(clients.isLoading ? 'Loading...' : 'Select online user')}</option>{online.map((client: any) => <option key={client.clid} value={client.clid}>{client.client_nickname} (#{client.clid})</option>)}
        </select>
        {clients.isError && <p role="alert" className="text-xs text-destructive">{apiErrorMessage(clients.error, t('Could not load recipients'))}</p>}
        {mode === 'offline' && <><Label htmlFor="message-uid">{t('Recipient Client UID')}</Label><Input id="message-uid" value={uid} onChange={event => setUid(event.target.value)} required maxLength={128} /><Label htmlFor="message-subject">{t('Subject')}</Label><Input id="message-subject" value={subject} onChange={event => setSubject(event.target.value)} required maxLength={255} /></>}
        <Label htmlFor="message-body">{t('Message')}</Label><Textarea id="message-body" value={body} onChange={event => setBody(event.target.value)} rows={5} required maxLength={8192} />
        {send.isError && <p role="alert" className="text-sm text-destructive">{apiErrorMessage(send.error, t('Failed to send message'))}</p>}
        <DialogFooter><Button type="button" variant="outline" disabled={send.isPending} onClick={() => setCompose(false)}>{t('Cancel')}</Button><Button type="submit" disabled={send.isPending || !body.trim() || (mode === 'chat' ? !recipient : !uid.trim() || !subject.trim())}>{t(send.isPending ? 'Sending…' : 'Send')}</Button></DialogFooter>
      </form></DialogContent></Dialog>
    <Dialog open={!!view} onOpenChange={() => setView(null)}><DialogContent><DialogHeader><DialogTitle>{view?.subject || t('Message')}</DialogTitle></DialogHeader>
      {detail.isLoading ? <PageLoader /> : detail.isError ? <p role="alert">{apiErrorMessage(detail.error, t('Could not load message'))}</p> : <p className="whitespace-pre-wrap break-words">{content || t('No content')}</p>}
      <DialogFooter><Button onClick={() => setView(null)}>{t('Close')}</Button></DialogFooter>
    </DialogContent></Dialog>
  </div>;
}
