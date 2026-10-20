import { useState } from 'react';
import { serversApi } from '@/api/servers.api';
import { useTranslation } from '@/lib/i18n';
import { apiErrorMessage } from '@/lib/api-error';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';

export function SnapshotBackup({ configId, sid, name }: { configId: number; sid: number; name: string }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false); const [password, setPassword] = useState('');
  const [confirm, setConfirm] = useState(''); const [busy, setBusy] = useState(false); const [error, setError] = useState('');
  return <><Button size="sm" variant="outline" onClick={() => { setOpen(true); setPassword(''); setConfirm(''); setError(''); }}>{t('Download configuration backup')}</Button>
    <Dialog open={open} onOpenChange={value => { if (!busy) { setOpen(value); setPassword(''); setConfirm(''); } }}>
      <DialogContent><DialogHeader><DialogTitle>{t('Server configuration backup')} · {name}</DialogTitle></DialogHeader>
        <p className="text-sm text-muted-foreground">{t('Includes server settings, channels, groups and permissions. Does not include server database files, icons, avatars or uploaded files.')}</p>
        <p className="text-xs text-muted-foreground">{t('Use the installer backup for a complete encrypted database and media backup. Use HTTPS to protect the password in transit.')}</p>
        <form className="space-y-3" onSubmit={async event => {
          event.preventDefault(); if (busy || password.length < 16 || password !== confirm) return;
          setBusy(true); setError('');
          try {
            const data = await serversApi.createSnapshot(configId, sid, password);
            const url = URL.createObjectURL(new Blob([JSON.stringify(data)], { type: 'application/json' }));
            const link = document.createElement('a'); link.href = url; link.download = `teamspeak-server-${sid}-${Date.now()}.encrypted.json`;
            document.body.append(link); link.click(); link.remove(); setTimeout(() => URL.revokeObjectURL(url), 1000);
            setOpen(false); setPassword(''); setConfirm('');
          } catch (failure) { setError(apiErrorMessage(failure, t('Backup failed'))); } finally { setBusy(false); }
        }}>
          <Label htmlFor={`backup-pass-${sid}`}>{t('Backup password (at least 16 characters)')}</Label>
          <Input id={`backup-pass-${sid}`} type="password" autoComplete="new-password" value={password} minLength={16} maxLength={128} onChange={event => setPassword(event.target.value)} />
          <Label htmlFor={`backup-confirm-${sid}`}>{t('Confirm password')}</Label>
          <Input id={`backup-confirm-${sid}`} type="password" autoComplete="new-password" value={confirm} onChange={event => setConfirm(event.target.value)} />
          {error && <p role="alert" className="text-sm text-destructive">{error}</p>}
          <Button disabled={busy || password.length < 16 || password !== confirm}>{t(busy ? 'Preparing backup…' : 'Download configuration backup')}</Button>
        </form>
      </DialogContent>
    </Dialog>
  </>;
}
