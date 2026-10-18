import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Puzzle, PackageCheck, Upload, ShieldCheck } from 'lucide-react';
import { toast } from 'sonner';
import { extensionsApi, type ExtensionManifest } from '@/api/music.api';
import { useServerStore } from '@/stores/server.store';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Badge } from '@/components/ui/badge';
import { Label } from '@/components/ui/label';
import { useTranslation } from '@/lib/i18n';

const EXAMPLE = JSON.stringify({
  id: 'welcome-pack',
  name: 'Welcome Pack',
  version: '1.0.0',
  author: 'Your name',
  description: 'A safe third-party command extension.',
  commands: [
    { name: 'website', description: 'Show the community website', response: 'https://example.com' },
  ],
}, null, 2);

export function ExtensionsTab() {
  const { t } = useTranslation();
  const { selectedConfigId } = useServerStore();
  const qc = useQueryClient();
  const [source, setSource] = useState(EXAMPLE);
  const queryKey = ['extensions', selectedConfigId];
  const packs = useQuery({
    queryKey,
    queryFn: () => extensionsApi.list(selectedConfigId!),
    enabled: Boolean(selectedConfigId),
  });
  const install = useMutation({
    mutationFn: (manifest: ExtensionManifest) => extensionsApi.import(selectedConfigId!, manifest),
    onSuccess: () => { toast.success(t('Extension installed')); qc.invalidateQueries({ queryKey }); },
    onError: (error: any) => toast.error(error?.response?.data?.error || t('Extension could not be installed')),
  });
  const toggle = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) => extensionsApi.setEnabled(selectedConfigId!, id, enabled),
    onSuccess: () => qc.invalidateQueries({ queryKey }),
  });

  const importManifest = () => {
    try { install.mutate(JSON.parse(source)); }
    catch { toast.error(t('Manifest must be valid JSON')); }
  };

  if (!selectedConfigId) return <p className="text-sm text-muted-foreground">{t('Select a server connection to manage extensions.')}</p>;
  const bundled = Array.isArray(packs.data?.bundled) ? packs.data.bundled : [];
  const installed = Array.isArray(packs.data?.installed) ? packs.data.installed : [];

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2 text-base"><PackageCheck className="h-4 w-4" /> {t('Bundled extensions')}</CardTitle></CardHeader>
        <CardContent className="grid gap-3 md:grid-cols-2">
          {bundled.map((pack: any) => (
            <div key={pack.id} className="rounded-lg border p-4">
              <div className="flex items-start justify-between gap-3"><div><div className="font-medium">{pack.name}</div><div className="mt-1 text-xs text-muted-foreground">{pack.description}</div></div><Badge>Bundled</Badge></div>
            </div>
          ))}
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2 text-base"><Puzzle className="h-4 w-4" /> {t('Third-party extensions')}</CardTitle></CardHeader>
        <CardContent className="space-y-3">
          {installed.length === 0 && <p className="text-sm text-muted-foreground">{t('No third-party extension installed yet.')}</p>}
          {installed.map((pack: any) => (
            <div key={pack.id} className="flex items-center justify-between gap-4 rounded-lg border p-4">
              <div><div className="font-medium">{pack.name} <span className="text-xs text-muted-foreground">v{pack.version}</span></div><div className="text-xs text-muted-foreground">{pack.description || `${pack.commands?.length || 0} command(s)`}</div></div>
              <Switch checked={Boolean(pack.enabled)} onCheckedChange={(enabled) => toggle.mutate({ id: pack.id, enabled })} />
            </div>
          ))}
        </CardContent>
      </Card>

      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2 text-base"><Upload className="h-4 w-4" /> {t('Install from manifest')}</CardTitle></CardHeader>
        <CardContent className="space-y-3">
          <div className="flex gap-2 rounded-md border border-emerald-500/30 bg-emerald-500/5 p-3 text-xs text-muted-foreground"><ShieldCheck className="h-4 w-4 shrink-0 text-emerald-500" /> Manifests use a permission-limited command format. Uploaded packages cannot execute arbitrary Node.js code, read secrets, or access the host filesystem.</div>
          <Label htmlFor="extension-manifest">extension.json</Label>
          <textarea id="extension-manifest" className="min-h-64 w-full rounded-md border bg-background p-3 font-mono text-xs" value={source} onChange={(event) => setSource(event.target.value)} spellCheck={false} />
          <Button onClick={importManifest} disabled={install.isPending}><Upload className="mr-2 h-4 w-4" /> {t('Validate and install')}</Button>
        </CardContent>
      </Card>
    </div>
  );
}
