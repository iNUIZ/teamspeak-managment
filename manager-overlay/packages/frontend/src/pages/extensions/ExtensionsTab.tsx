import { translateUiError as uiError } from '@/lib/i18n';
import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';
import { useState, type FormEvent } from 'react';
import { useNavigate } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Puzzle, MessageSquarePlus, Settings2, Trash2, Plus } from 'lucide-react';
import { toast } from 'sonner';
import { chatCommandsApi, extensionsApi } from '@/api/music.api';
import { botsApi } from '@/api/bots.api';
import { useServerStore } from '@/stores/server.store';
import { BOT_TEMPLATES, TEMPLATE_CATEGORIES, type BotTemplate } from '@/data/bot-templates';
import { TemplateGallery } from '@/components/bots/TemplateGallery';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import { Badge } from '@/components/ui/badge';
import { useTranslation } from '@/lib/i18n';

type Flow = { id: number; name: string; enabled: boolean; serverConfigId: number };
type Command = { id: number; name: string; response: string; enabled: boolean };

/** A flow counts as this extension when it was installed from it (same name, either language). */
function flowFor(template: BotTemplate, flows: Flow[]): Flow | undefined {
  const names = new Set([template.name, uiText(template.name)]);
  return flows.find((flow) => names.has(flow.name));
}

/**
 * Extensions = ready features you switch on (each opens a short settings form),
 * plus your own `!command → reply` lines. No JSON.
 */
export function ExtensionsTab({ fixedConfigId }: { fixedConfigId?: number } = {}) {
  useUiLanguageStore(state => state.language);
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { selectedConfigId: headerConfigId, selectedSid } = useServerStore();
  const configId = fixedConfigId ?? headerConfigId;
  const qc = useQueryClient();
  const [installing, setInstalling] = useState<BotTemplate | null>(null);
  const [name, setName] = useState('');
  const [response, setResponse] = useState('');

  const flows = useQuery({ queryKey: ['bots'], queryFn: botsApi.list, enabled: Boolean(configId) });
  const commandsKey = ['chat-commands', configId];
  const commands = useQuery({ queryKey: commandsKey, queryFn: () => chatCommandsApi.list(configId!), enabled: Boolean(configId) });
  const packsKey = ['extensions', configId];
  const packs = useQuery({ queryKey: packsKey, queryFn: () => extensionsApi.list(configId!), enabled: Boolean(configId) });

  const install = useMutation({
    mutationFn: async ({ name, description, flowData }: { name: string; description: string; flowData: unknown }) => {
      const flow = await botsApi.create({ name, description, serverConfigId: configId, virtualServerId: selectedSid || 1, flowData });
      await botsApi.enable(flow.id);
      return flow;
    },
    onSuccess: () => { toast.success(t('Extension turned on')); qc.invalidateQueries({ queryKey: ['bots'] }); },
    onError: (error: any) => toast.error(uiError(String(error?.response?.data?.error || t('Extension could not be installed')), t('Extension could not be installed'))),
  });
  const toggleFlow = useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) => (enabled ? botsApi.enable(id) : botsApi.disable(id)),
    onSuccess: () => qc.invalidateQueries({ queryKey: ['bots'] }),
    onError: () => toast.error(t('Extension settings could not be saved')),
  });
  const addCommand = useMutation({
    mutationFn: () => chatCommandsApi.create(configId!, { name: name.trim().replace(/^!/, ''), response: response.trim(), enabled: true }),
    onSuccess: () => { toast.success(t('Command added')); setName(''); setResponse(''); qc.invalidateQueries({ queryKey: commandsKey }); },
    onError: (error: any) => toast.error(uiError(String(error?.response?.data?.error || t('Command could not be added')), t('Command could not be added'))),
  });
  const toggleCommand = useMutation({
    mutationFn: ({ id, enabled }: { id: number; enabled: boolean }) => chatCommandsApi.update(configId!, id, { enabled }),
    onSuccess: () => qc.invalidateQueries({ queryKey: commandsKey }),
    onError: () => toast.error(t('Extension settings could not be saved')),
  });
  const deleteCommand = useMutation({
    mutationFn: (id: number) => chatCommandsApi.delete(configId!, id),
    onSuccess: () => qc.invalidateQueries({ queryKey: commandsKey }),
    onError: () => toast.error(t('Command could not be deleted')),
  });
  const togglePack = useMutation({
    mutationFn: ({ id, enabled }: { id: string; enabled: boolean }) => extensionsApi.setEnabled(configId!, id, enabled),
    onSuccess: () => qc.invalidateQueries({ queryKey: packsKey }),
    onError: () => toast.error(t('Extension settings could not be saved')),
  });

  if (!configId) return <p className="text-sm text-muted-foreground">{t('Select a server connection to manage extensions.')}</p>;

  const serverFlows: Flow[] = (Array.isArray(flows.data) ? flows.data : []).filter((flow: Flow) => flow.serverConfigId === configId);
  const commandList: Command[] = Array.isArray(commands.data) ? commands.data : [];
  const packList = Array.isArray(packs.data?.installed) ? packs.data.installed : [];
  const canAdd = name.trim().replace(/^!/, '').length > 0 && response.trim().length > 0 && !addCommand.isPending;
  const submit = (event: FormEvent) => { event.preventDefault(); if (canAdd) addCommand.mutate(); };

  return (
    <div className="space-y-5">
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-base"><Puzzle className="h-4 w-4" /> {t('Ready extensions')}</CardTitle>
          <p className="text-sm text-muted-foreground">{t('Turn on what you need. Each one asks only for the few settings it uses.')}</p>
        </CardHeader>
        <CardContent className="space-y-5">
          {TEMPLATE_CATEGORIES.map((category) => {
            const templates = BOT_TEMPLATES.filter((template) => template.category === category.id);
            if (templates.length === 0) return null;
            return (
              <section key={category.id} aria-label={t(category.label)}>
                <h3 className="mb-2 text-xs font-semibold uppercase tracking-wider text-muted-foreground">{t(category.label)}</h3>
                <div className="grid gap-2 md:grid-cols-2 xl:grid-cols-3">
                  {templates.map((template) => {
                    const Icon = template.icon;
                    const flow = flowFor(template, serverFlows);
                    return (
                      <div key={template.id} role="group" aria-label={t(template.name)} className="flex items-start gap-3 rounded-lg border p-3">
                        <div className="mt-0.5 rounded-md bg-primary/10 p-1.5"><Icon className="h-4 w-4 text-primary" /></div>
                        <div className="min-w-0 flex-1">
                          <div className="flex items-center gap-2">
                            <span className="text-sm font-medium">{t(template.name)}</span>
                            {flow && <Badge variant={flow.enabled ? 'default' : 'secondary'} className="text-[10px]">{flow.enabled ? t('On') : t('Off')}</Badge>}
                          </div>
                          <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">{t(template.description)}</p>
                          <div className="mt-2 flex items-center gap-2">
                            {flow ? (
                              <>
                                <Switch
                                  aria-label={t(template.name)}
                                  checked={flow.enabled}
                                  disabled={toggleFlow.isPending}
                                  onCheckedChange={(enabled) => toggleFlow.mutate({ id: flow.id, enabled })}
                                />
                                <Button variant="ghost" size="sm" className="h-7 text-xs" onClick={() => navigate(`/bots/${flow.id}`)}>
                                  <Settings2 className="mr-1 h-3 w-3" /> {t('Settings')}
                                </Button>
                              </>
                            ) : (
                              <Button size="sm" className="h-7 text-xs" disabled={install.isPending} onClick={() => setInstalling(template)}>
                                <Plus className="mr-1 h-3 w-3" /> {t('Turn on')}
                              </Button>
                            )}
                          </div>
                        </div>
                      </div>
                    );
                  })}
                </div>
              </section>
            );
          })}
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="flex items-center gap-2 text-base"><MessageSquarePlus className="h-4 w-4" /> {t('Your own commands')}</CardTitle>
          <p className="text-sm text-muted-foreground">{t('When someone types the command in chat, the bot replies with your text.')}</p>
        </CardHeader>
        <CardContent className="space-y-4">
          <form onSubmit={submit} className="grid gap-3 sm:grid-cols-[minmax(0,12rem)_minmax(0,1fr)_auto] sm:items-end">
            <div>
              <Label htmlFor="extension-command-name" className="text-xs">{t('Command')}</Label>
              <div className="relative mt-1">
                <span className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-sm text-muted-foreground">!</span>
                <Input id="extension-command-name" className="pl-6" placeholder="site" maxLength={32} value={name} onChange={(e) => setName(e.target.value)} />
              </div>
            </div>
            <div>
              <Label htmlFor="extension-command-response" className="text-xs">{t('Bot reply')}</Label>
              <Input id="extension-command-response" className="mt-1" placeholder="https://example.com" maxLength={900} value={response} onChange={(e) => setResponse(e.target.value)} />
            </div>
            <Button type="submit" disabled={!canAdd}><Plus className="mr-1 h-4 w-4" /> {t('Add')}</Button>
          </form>

          {commandList.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t('No commands yet.')}</p>
          ) : (
            <ul className="divide-y rounded-lg border">
              {commandList.map((command) => (
                <li key={command.id} className="flex items-center gap-3 p-3">
                  <code className="shrink-0 text-sm">!{command.name}</code>
                  <span className="min-w-0 flex-1 truncate text-sm text-muted-foreground">{command.response}</span>
                  <Switch aria-label={`!${command.name}`} checked={command.enabled} disabled={toggleCommand.isPending} onCheckedChange={(enabled) => toggleCommand.mutate({ id: command.id, enabled })} />
                  <Button variant="ghost" size="icon" className="h-7 w-7 text-destructive hover:text-destructive" aria-label={uiText('Delete {0}', `!${command.name}`)} title={uiText('Delete {0}', `!${command.name}`)} disabled={deleteCommand.isPending} onClick={() => deleteCommand.mutate(command.id)}>
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>

      {packList.length > 0 && (
        <Card>
          <CardHeader className="pb-3"><CardTitle className="text-base">{t('Installed packages')}</CardTitle></CardHeader>
          <CardContent className="space-y-2">
            {packList.map((pack: any) => (
              <div key={pack.id} className="flex items-center justify-between gap-4 rounded-lg border p-3">
                <div><div className="text-sm font-medium">{pack.name}</div><div className="text-xs text-muted-foreground">{pack.description || uiText('{0} command(s)', pack.commands?.length || 0)}</div></div>
                <Switch aria-label={pack.name} disabled={togglePack.isPending} checked={Boolean(pack.enabled)} onCheckedChange={(enabled) => togglePack.mutate({ id: pack.id, enabled })} />
              </div>
            ))}
          </CardContent>
        </Card>
      )}

      <TemplateGallery
        open={installing !== null}
        onOpenChange={(open) => { if (!open) setInstalling(null); }}
        template={installing}
        createLabel={t('Turn on')}
        onSelect={(flowName, description, flowData) => install.mutate({ name: flowName, description, flowData })}
      />
    </div>
  );
}
