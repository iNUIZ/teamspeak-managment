import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';
import { useRef, useState, type KeyboardEvent } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import { useBots, useCreateBot, useToggleBot } from '@/hooks/use-bots';
import { useServerStore } from '@/stores/server.store';
import { botsApi } from '@/api/bots.api';
import { PageLoader } from '@/components/shared/LoadingSpinner';
import { EmptyState } from '@/components/shared/EmptyState';
import { CONNECTION_SETUP_PATH, ConnectionRequiredNotice, useConnectionAvailability } from '@/components/shared/NoServerSelectedState';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { Badge } from '@/components/ui/badge';
import { Bot, Plus, Pencil, Trash2, Play, Clock, AlertTriangle, LayoutTemplate } from 'lucide-react';
import { ConfirmDialog } from '@/components/shared/ConfirmDialog';
import { TemplateGallery } from '@/components/bots/TemplateGallery';
import { toast } from 'sonner';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { timeAgo } from '@/lib/utils';
import { CommandsTab } from '@/pages/media-bots/CommandsTab';
import { MediaCommandPermissions } from '@/components/bots/MediaCommandPermissions';
import { ExtensionsTab } from '@/pages/extensions/ExtensionsTab';
import { useTranslation } from '@/lib/i18n';

/** Render Bot Flows and its server-scoped Chat commands tab. */
export default function BotList() {
  useUiLanguageStore(state => state.language);
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();
  const qc = useQueryClient();
  const { selectedConfigId, selectedSid } = useServerStore();
  const { data, isLoading } = useBots();
  const createBot = useCreateBot();
  const toggleBot = useToggleBot();
  const deleteBot = useMutation({ mutationFn: (id: number) => botsApi.delete(id), onSuccess: () => qc.invalidateQueries({ queryKey: ['bots'] }) });

  const [showCreate, setShowCreate] = useState(false);
  const [showTemplates, setShowTemplates] = useState(false);
  const [deleteId, setDeleteId] = useState<number | null>(null);
  const [newName, setNewName] = useState('');
  const [newDesc, setNewDesc] = useState('');
  const flowsTabRef = useRef<HTMLButtonElement>(null);
  const chatCommandsTabRef = useRef<HTMLButtonElement>(null);
  const extensionsTabRef = useRef<HTMLButtonElement>(null);

  const bots = Array.isArray(data) ? data : [];
  const { isPending: connectionsPending, hasNoConnections } = useConnectionAvailability();
  const createBlocked = connectionsPending || hasNoConnections;

  if (isLoading) return <PageLoader />;

  const handleCreate = () => {
    if (!selectedConfigId) {
      toast.error(uiText("Please select a server first"));
      return;
    }
    createBot.mutate({ name: newName, description: newDesc, serverConfigId: selectedConfigId, virtualServerId: selectedSid || 1, flowData: { nodes: [], edges: [] } }, {
      onSuccess: (bot: any) => { toast.success(uiText("Bot created")); setShowCreate(false); setNewName(''); setNewDesc(''); navigate(`/bots/${bot.id}`); },
      onError: () => toast.error(uiText("Failed to create bot")),
    });
  };

  const activeTab = searchParams.get('tab') === 'commands' ? 'commands' : searchParams.get('tab') === 'extensions' ? 'extensions' : 'flows';
  const commandsTab = activeTab === 'commands';
  const extensionsTab = activeTab === 'extensions';
  const handleTabKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return;
    event.preventDefault();
    const order = ['flows', 'commands', 'extensions'] as const;
    const current = order.indexOf(activeTab);
    const next = order[(current + (event.key === 'ArrowRight' ? 1 : order.length - 1)) % order.length];
    setSearchParams(next === 'flows' ? {} : { tab: next });
    ({ flows: flowsTabRef, commands: chatCommandsTabRef, extensions: extensionsTabRef }[next]).current?.focus();
  };
  const tabLinks = (
    <div className="flex gap-1 border-b" role="tablist" aria-label={uiText("Bot Flows sections")}>
      <button
        type="button"
        role="tab"
        id="bot-flows-tab"
        ref={flowsTabRef}
        aria-controls="bot-flows-panel"
        aria-selected={activeTab === 'flows'}
        tabIndex={activeTab === 'flows' ? 0 : -1}
        onKeyDown={handleTabKeyDown}
        className={`border-b-2 px-3 py-2 text-sm ${activeTab === 'flows' ? 'border-primary font-medium' : 'border-transparent text-muted-foreground'}`}
        onClick={() => setSearchParams({})}
      >{t('Flows')}</button>
      <button
        type="button"
        role="tab"
        id="bot-chat-commands-tab"
        ref={chatCommandsTabRef}
        aria-controls="bot-chat-commands-panel"
        aria-selected={commandsTab}
        tabIndex={commandsTab ? 0 : -1}
        onKeyDown={handleTabKeyDown}
        className={`border-b-2 px-3 py-2 text-sm ${commandsTab ? 'border-primary font-medium' : 'border-transparent text-muted-foreground'}`}
        onClick={() => setSearchParams({ tab: 'commands' })}
      >{t('Chat commands')}</button>
      <button
        type="button"
        role="tab"
        id="bot-extensions-tab"
        ref={extensionsTabRef}
        aria-controls="bot-extensions-panel"
        aria-selected={extensionsTab}
        tabIndex={extensionsTab ? 0 : -1}
        onKeyDown={handleTabKeyDown}
        className={`border-b-2 px-3 py-2 text-sm ${extensionsTab ? 'border-primary font-medium' : 'border-transparent text-muted-foreground'}`}
        onClick={() => setSearchParams({ tab: 'extensions' })}
      >{t('Extensions')}</button>
    </div>
  );

  if (commandsTab) {
    return (
      <div className="space-y-5">
        <h1 className="text-xl font-semibold">{uiText("Bot Flows")}</h1>
        {tabLinks}
        <div role="tabpanel" id="bot-chat-commands-panel" aria-labelledby="bot-chat-commands-tab" tabIndex={0}>
          <MediaCommandPermissions />
          <CommandsTab showClashWarnings />
        </div>
      </div>
    );
  }

  if (extensionsTab) {
    return (
      <div className="space-y-5">
        <h1 className="text-xl font-semibold">{uiText("Bot Flows")}</h1>
        {tabLinks}
        <div role="tabpanel" id="bot-extensions-panel" aria-labelledby="bot-extensions-tab" tabIndex={0}>
          <ExtensionsTab />
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-5">
      <div className="flex flex-col items-start gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h1 className="text-xl font-semibold">{uiText("Bot Flows")}</h1>
        <div className="flex w-full flex-wrap items-center gap-2 sm:w-auto">
          <Button variant="outline" size="sm" disabled={createBlocked} onClick={() => setShowTemplates(true)}><LayoutTemplate className="h-4 w-4 mr-1" /> {uiText(" From Template")}</Button>
          <Button size="sm" disabled={createBlocked} onClick={() => setShowCreate(true)}><Plus className="h-4 w-4 mr-1" /> {uiText(" New Bot")}</Button>
        </div>
      </div>
      {tabLinks}
      <div role="tabpanel" id="bot-flows-panel" aria-labelledby="bot-flows-tab" tabIndex={0}>
      {bots.length > 0 && hasNoConnections && (
        <ConnectionRequiredNotice>{uiText("New bot flows need a TeamSpeak server connection.")}</ConnectionRequiredNotice>
      )}

      {bots.length === 0 && hasNoConnections ? (
        <EmptyState icon={Bot} title={uiText("Connect a TeamSpeak server first")} description={uiText("Bot flows run against a server connection. Add one in Settings → Connections, then create your first flow.")}>
          <Button size="sm" asChild>
            <Link to={CONNECTION_SETUP_PATH}>{uiText("Open connection setup")}</Link>
          </Button>
        </EmptyState>
      ) : bots.length === 0 ? (
        <EmptyState icon={Bot} title={uiText("No bot flows yet")} description={uiText("Create your first automation flow to get started.")} />
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 xl:grid-cols-3 gap-4">
          {bots.map((bot: any) => (
            <Card key={bot.id} className="group hover:border-primary/30 transition-colors">
              <CardHeader className="pb-2">
                <div className="flex items-center justify-between">
                  <CardTitle className="text-sm font-medium truncate">{bot.name}</CardTitle>
                  <div className="flex items-center gap-2">
                    <Switch
                      checked={bot.enabled}
                      onCheckedChange={(enabled) => toggleBot.mutate({ id: bot.id, enabled }, {
                        onSuccess: () => toast.success(enabled ? uiText("Bot enabled") : uiText("Bot disabled")),
                      })}
                    />
                  </div>
                </div>
              </CardHeader>
              <CardContent className="space-y-3">
                <p className="text-xs text-muted-foreground line-clamp-2">{bot.description || uiText("No description")}</p>

                <div className="flex items-center gap-2 flex-wrap">
                  <Badge variant={bot.enabled ? 'default' : 'secondary'} className="text-[10px]">
                    {bot.enabled ? uiText("Active") : uiText("Inactive")}
                  </Badge>
                  {bot.serverConfigId && (
                    <Badge variant="outline" className="text-[10px]">{uiText("Server #")}{bot.serverConfigId}</Badge>
                  )}
                </div>

                {bot.updatedAt && (
                  <p className="text-[10px] text-muted-foreground flex items-center gap-1">
                    <Clock className="h-3 w-3" /> {uiText(" Updated ")}{timeAgo(new Date(bot.updatedAt).getTime() / 1000)}
                  </p>
                )}

                <div className="touch-action-reveal flex items-center gap-1 pt-1 transition-opacity">
                  <Button variant="outline" size="sm" className="h-7 text-xs flex-1" onClick={() => navigate(`/bots/${bot.id}`)}>
                    <Pencil className="h-3 w-3 mr-1" /> {uiText(" Edit Flow")}</Button>
                  <Button variant="ghost" size="icon" className="h-7 w-7 text-destructive hover:text-destructive" onClick={() => setDeleteId(bot.id)} aria-label={uiText("Delete {0}", bot.name)} title={uiText("Delete {0}", bot.name)}>
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}

      {/* Create Dialog */}
      <Dialog open={showCreate} onOpenChange={setShowCreate}>
        <DialogContent>
          <DialogHeader><DialogTitle>{uiText("New Bot Flow")}</DialogTitle></DialogHeader>
          <div className="space-y-3">
            <div><Label className="text-xs">{uiText("Name")}</Label><Input value={newName} onChange={(e) => setNewName(e.target.value)} placeholder={uiText("My Automation")} /></div>
            <div><Label className="text-xs">{uiText("Description")}</Label><Input value={newDesc} onChange={(e) => setNewDesc(e.target.value)} placeholder={uiText("What does this bot do?")} /></div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowCreate(false)}>{uiText("Cancel")}</Button>
            <Button onClick={handleCreate} disabled={!newName || !selectedConfigId || createBot.isPending}>{uiText("Create")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Template Gallery */}
      <TemplateGallery
        open={showTemplates}
        onOpenChange={setShowTemplates}
        onSelect={(name, description, flowData) => {
          if (!selectedConfigId) {
            toast.error(uiText("Please select a server first"));
            return;
          }
          createBot.mutate({ name, description, serverConfigId: selectedConfigId, virtualServerId: selectedSid || 1, flowData }, {
            onSuccess: (bot: any) => { toast.success(uiText("Bot '{0}' created from template", name)); navigate(`/bots/${bot.id}`); },
            onError: () => toast.error(uiText("Failed to create bot from template")),
          });
        }}
      />

      {/* Delete Confirm */}
      <ConfirmDialog
        open={deleteId !== null}
        onOpenChange={() => setDeleteId(null)}
        title={uiText("Delete Bot?")}
        description={uiText("This will permanently delete this bot flow and all its execution history.")}
        onConfirm={() => {
          if (deleteId) deleteBot.mutate(deleteId, { onSuccess: () => { toast.success(uiText("Bot deleted")); setDeleteId(null); } });
        }}
        destructive
      />
      </div>
    </div>
  );
}
