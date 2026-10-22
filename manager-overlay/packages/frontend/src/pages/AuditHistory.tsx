import { translate as tr, useTranslation as useUiTranslation } from '@/lib/i18n';
import { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import {
  ADMIN_AUDIT_ACTIONS,
  ADMIN_AUDIT_OUTCOMES,
  type AdminAuditAction,
  type AdminAuditEventDto,
  type AdminAuditOutcome,
  type AdminAuditResultCode,
} from '@ts6/common';
import { ClipboardList, Filter, RefreshCw } from 'lucide-react';
import { auditApi } from '@/api/audit.api';
import { usersApi } from '@/api/bots.api';
import { serversApi } from '@/api/servers.api';
import { PageLoader } from '@/components/shared/LoadingSpinner';
import { EmptyState } from '@/components/shared/EmptyState';
import { RefreshStatus, StaleDataNotice } from '@/components/shared/RefreshStatus';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { adminAuditHistoryRefetchInterval } from '@/lib/admin-audit-live';
import { apiErrorMessage } from '@/lib/api-error';
import { historyRefreshPresentation, nextScopedCursorStack } from '@/lib/history-status-consistency';
import { cn } from '@/lib/utils';

const ACTION_LABELS: Record<AdminAuditAction, string> = {
  'client.kick': 'Kick client',
  'client.ban': 'Ban client',
  'client.move': 'Move client',
  'client.poke': 'Poke client',
  'client.message': 'Message client',
  'client.permission_add': 'Add client permission',
  'client.permission_delete': 'Delete client permission',
  'ban.create': 'Add ban',
  'ban.delete': 'Delete ban',
  'ban.delete_all': 'Clear bans',
  'channel.create': 'Create channel',
  'channel.update': 'Edit channel',
  'channel.delete': 'Delete channel',
  'channel.move': 'Move channel',
  'channel.permission_add': 'Add channel permission',
  'channel.permission_delete': 'Delete channel permission',
  'server_group.create': 'Create server group',
  'server_group.update': 'Rename server group',
  'server_group.delete': 'Delete server group',
  'server_group.copy': 'Copy server group',
  'server_group.member_add': 'Add server group member',
  'server_group.member_remove': 'Remove server group member',
  'server_group.permission_add': 'Add server group permission',
  'server_group.permission_delete': 'Delete server group permission',
  'channel_group.create': 'Create channel group',
  'channel_group.update': 'Rename channel group',
  'channel_group.delete': 'Delete channel group',
  'channel_group.assign': 'Assign channel group',
  'channel_group.permission_add': 'Add channel group permission',
  'channel_group.permission_delete': 'Delete channel group permission',
  'privilege_key.create': 'Create privilege key',
  'privilege_key.delete': 'Delete privilege key',
  'virtual_server.edit': 'Edit virtual server',
  'virtual_server.start': 'Start virtual server',
  'virtual_server.stop': 'Stop virtual server',
  'connection.create': 'Create connection',
  'connection.update': 'Update connection',
  'connection.credentials_changed': 'Change connection credentials',
  'connection.delete': 'Delete connection',
  'flow.create': 'Create flow',
  'flow.update': 'Update flow',
  'flow.delete': 'Delete flow',
  'flow.enable': 'Enable flow',
  'flow.disable': 'Disable flow',
  'user.create': 'Create user',
  'user.update': 'Update user',
  'user.delete': 'Delete user',
  'settings.yt_cookies_changed': 'Update yt-dlp cookies',
  'settings.yt_cookies_removed': 'Remove yt-dlp cookies',
  'settings.youtube_auth_mode_changed': 'Change YouTube playback method',
  'settings.limits_update': 'Update app limits',
  'settings.video_streaming_update': 'Update video streaming defaults',
  'settings.iptv_network_update': 'Update allowed local IPTV hosts',
  'media.music.start': 'Start music',
  'media.music.stop': 'Stop music',
  'media.video.start': 'Start video stream',
  'media.video.stop': 'Stop video stream',
  'media.video.source_change': 'Change stream source',
  'media.video.queue_add': 'Queue a video',
  'media.video.queue_change': 'Change the video queue',
  'media.video.skip': 'Skip to a queued video',
  'media.session.switch': 'Switch media (replace what was playing)',
  'listener.remote.queue_add': 'Listener remote: add library song',
  'listener.remote.issue': 'Issue listener remote link',
  'listener.remote.request': 'Listener remote: request link',
  'listener.remote.revoke': 'Revoke listener remote access',
  'listener.remote.auth_failed': 'Listener remote: refused access (total for 5 minutes)',
  'listener.remote.rate_limited': 'Listener remote: rate limited (total for 5 minutes)',
  'listener.remote.denied': 'Listener remote: permission denied',
  'listener.remote.capacity': 'Listener remote: queue full',
  'music_bot.avatar_update': 'Update bot avatar',
  'music_bot.automation_update': 'Update bot automation',
  'channel_banner.upload': 'Upload channel banner',
  'channel_banner.delete': 'Delete channel banner',
  'server_icon.upload': 'Upload server icon',
  'channel_file.mkdir': 'Create channel folder',
  'channel_file.upload': 'Upload channel file',
  'channel_file.download': 'Download channel file',
  'channel_file.delete': 'Delete channel file',
  'server_icon.delete': 'Delete server icon',
  'settings.public_url_update': 'Update public URL',
};

const RESULT_CODE_LABELS: Record<AdminAuditResultCode, string> = {
  ok: 'OK',
  ts_error: 'TeamSpeak error',
  timeout: 'Timeout',
  network_error: 'Network error',
  engine_reload_failed: 'Engine reload failed',
  pool_refresh_failed: 'Pool refresh failed',
  not_found: 'Not found',
  validation_failed: 'Validation failed',
  storage_failed: 'Storage failed',
  invalid_access: 'Invalid or expired access',
  permission_denied: 'Permission denied',
  rate_limited: 'Rate limited',
  queue_full: 'Queue full',
  invalid_request: 'Invalid request',
  cancelled: 'Cancelled',
  unavailable: 'Unavailable',
  unknown: 'Unknown',
};

const TARGET_TYPE_LABELS: Record<string, string> = {
  client: 'Client',
  ban: 'Ban',
  channel: 'Channel',
  server_group: 'Server group',
  channel_group: 'Channel group',
  privilege_key: 'Privilege key',
  virtual_server: 'Virtual server',
  connection: 'Connection',
  listener_remote_refusals: 'Refusals',
  flow: 'Flow',
  user: 'User',
  settings: 'Settings',
  music_bot: 'Music bot',
  channel_banner: 'Channel banner',
  server_icon: 'Server icon',
};

const AUDIT_GRID =
  'grid min-w-[52rem] grid-cols-[9.5rem_minmax(8rem,1.1fr)_minmax(6rem,0.9fr)_minmax(7rem,1fr)_minmax(6.5rem,0.85fr)_minmax(5.5rem,0.75fr)] gap-x-3';

function outcomeClass(outcome: AdminAuditOutcome): string {
  switch (outcome) {
    case 'success':
      return 'text-emerald-600 dark:text-emerald-400';
    case 'failure':
      return 'text-destructive';
    case 'partial':
      return 'text-amber-600 dark:text-amber-400';
    case 'unknown':
    case 'pending':
      return 'text-muted-foreground';
    default:
      return 'text-muted-foreground';
  }
}

function formatWhen(iso: string): string {
  try {
    return new Intl.DateTimeFormat(undefined, {
      dateStyle: 'medium',
      timeStyle: 'medium',
    }).format(new Date(iso));
  } catch {
    return iso;
  }
}

function formatDurationMs(startedIso: string, completedIso: string | null): string | null {
  if (!completedIso) return null;
  const start = Date.parse(startedIso);
  const end = Date.parse(completedIso);
  if (!Number.isFinite(start) || !Number.isFinite(end) || end < start) return null;
  const ms = end - start;
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.round(ms / 1000)}s`;
}

function EventRow({ event, connections = [] }: { event: AdminAuditEventDto; connections?: { id: number; name: string }[] }) {
  useUiTranslation();
  const label = tr(ACTION_LABELS[event.action] ?? event.action);
  const actor = event.actorUsername ?? `user #${event.actorUserId}`;
  const targetType = event.targetType
    ? tr(TARGET_TYPE_LABELS[event.targetType] ?? event.targetType)
    : null;
  const target = targetType
    ? `${targetType}${event.targetId ? ` ${event.targetId}` : ''}`
    : '—';
  const contextBits = [
    event.connectionId != null ? `${connections.find(row => row.id === event.connectionId)?.name || tr('Connection')} (#${event.connectionId})` : null,
    event.virtualServerId != null ? `${tr('TeamSpeak server')} #${event.virtualServerId}` : null,
  ].filter(Boolean);
  const context = contextBits.join(' · ') || '—';
  const resultLabel = event.resultCode
    ? tr(RESULT_CODE_LABELS[event.resultCode] ?? event.resultCode)
    : null;
  const duration = formatDurationMs(event.createdAt, event.completedAt);
  const title = [
    `Operation ${event.operationId}`,
    resultLabel ? `Result: ${resultLabel}` : null,
    duration ? `Duration: ${duration}` : null,
    event.completedAt ? `Completed: ${formatWhen(event.completedAt)}` : null,
  ].filter(Boolean).join(' · ');

  return (
    <div
      role="row"
      className={cn(AUDIT_GRID, 'items-start border-b border-border/60 px-4 py-2.5 text-sm last:border-0')}
      title={title}
    >
      <div className="space-y-0.5">
        <p className="text-xs text-muted-foreground tabular-nums leading-snug">
          {formatWhen(event.createdAt)}
        </p>
        {duration && (
          <p className="text-[10px] text-muted-foreground/80">{duration}</p>
        )}
      </div>
      <div className="min-w-0">
        <p className="font-medium leading-snug truncate" title={label}>{label}</p>
        <p className="text-[10px] text-muted-foreground font-mono-data truncate" title={event.action}>
          {event.action}
        </p>
      </div>
      <p className="min-w-0 truncate text-xs leading-snug" title={actor}>{actor}</p>
      <p className="min-w-0 truncate text-xs text-muted-foreground font-mono-data leading-snug" title={target}>
        {target}
      </p>
      <p className="min-w-0 truncate text-xs text-muted-foreground leading-snug" title={context}>
        {context}
      </p>
      <div className="min-w-0 space-y-0.5">
        <p className={cn('text-xs font-medium uppercase tracking-wide', outcomeClass(event.outcome))}>
          {tr(event.outcome)}
        </p>
        {resultLabel && (
          <p className="text-[10px] text-muted-foreground truncate" title={resultLabel}>
            {resultLabel}
          </p>
        )}
      </div>
    </div>
  );
}

export default function AuditHistory() {
  const actorsQuery = useQuery({ queryKey: ['audit-actors'], queryFn: usersApi.list });
  const connectionsQuery = useQuery({ queryKey: ['audit-connections'], queryFn: serversApi.list });
  useUiTranslation();
  const [action, setAction] = useState<string>('');
  const [outcome, setOutcome] = useState<string>('');
  const [actorUserId, setActorUserId] = useState('');
  const [connectionId, setConnectionId] = useState('');
  const [virtualServerId, setVirtualServerId] = useState('');
  const filterScopeKey = [
    action,
    outcome,
    actorUserId,
    connectionId,
    virtualServerId,
  ].join('|');
  const [pageState, setPageState] = useState<{ scopeKey: string; cursorStack: (string | null)[] }>({
    scopeKey: filterScopeKey,
    cursorStack: [null],
  });

  const scopedPage = nextScopedCursorStack(pageState, filterScopeKey, [null]);
  if (scopedPage !== pageState) {
    setPageState(scopedPage);
  }

  const cursorStack = scopedPage.cursorStack;
  const cursor = cursorStack[cursorStack.length - 1] ?? undefined;
  const onNewestPage = cursorStack.length === 1 && cursorStack[0] == null;
  const historyRefetchInterval = adminAuditHistoryRefetchInterval({ onNewestPage });
  const historyLive = historyRefetchInterval !== false;

  const filters = useMemo(
    () => ({
      action: (action || undefined) as AdminAuditAction | undefined,
      outcome: (outcome || undefined) as AdminAuditOutcome | undefined,
      actorUserId: actorUserId ? Number(actorUserId) : undefined,
      connectionId: connectionId ? Number(connectionId) : undefined,
      virtualServerId: virtualServerId ? Number(virtualServerId) : undefined,
      cursor: cursor || undefined,
      limit: 50,
    }),
    [action, outcome, actorUserId, connectionId, virtualServerId, cursor],
  );

  const { data, isLoading, isFetching, error, refetch, isError, status } = useQuery({
    queryKey: ['admin-audit', filters],
    queryFn: () => auditApi.list(filters),
    refetchInterval: historyRefetchInterval,
  });

  const backgroundError = isError
    ? apiErrorMessage(
      error,
      data
        ? 'Audit refresh failed. The last successful page is still displayed.'
        : 'Failed to load audit history. Confirm you are signed in as an admin.',
    )
    : null;
  // Treat a failed poll as non-live even when React Query keeps prior rows.
  const refreshPresentation = historyRefreshPresentation({
    isFetching,
    hasError: !!backgroundError,
    livePolling: historyLive && status !== 'error',
    idleLiveLabel: 'Live audit history active',
    idleStaticLabel: onNewestPage ? 'Audit history up to date' : 'This audit page loaded',
    refreshingLabel: 'Refreshing audit…',
    degradedLabel: 'Audit updates interrupted',
  });

  const resetPages = () => setPageState({ scopeKey: filterScopeKey, cursorStack: [null] });
  const retryRefresh = () => {
    void refetch();
  };

  return (
    <div className="mx-auto flex h-full max-w-6xl flex-col space-y-4" data-testid="audit-history-page">
      <div className="flex shrink-0 flex-wrap items-start justify-between gap-3">
        <div className="space-y-1">
          <h1 className="flex items-center gap-2 text-xl font-semibold">
            <ClipboardList className="h-5 w-5 text-primary" /> {tr("Administrative Audit")}</h1>
          <p className="text-sm text-muted-foreground max-w-2xl">
            {tr('Manager-initiated admin actions only — separate from TeamSpeak client activity and Server Logs.')}{' '}
            {tr('Retention:')}{' '}{data?.retention?.maxAgeDays ?? 30}{' '}{tr('days or')}{' '}{data?.retention?.maxRows?.toLocaleString() ?? '100.000'}{' '}{tr('rows (whichever first). Records cannot be deleted from this UI.')}
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <RefreshStatus
            isRefreshing={isFetching}
            tone={refreshPresentation.tone}
            idleLabel={refreshPresentation.idleLabel}
            refreshingLabel={refreshPresentation.refreshingLabel}
            degradedLabel={refreshPresentation.degradedLabel}
          />
          {refreshPresentation.showLiveBadge && (
            <span className="text-xs font-medium text-emerald-600" title={tr("Newest page refreshes automatically")} data-testid="audit-live-badge"> {tr("Live")}</span>
          )}
          <Button
            size="sm"
            variant="outline"
            onClick={retryRefresh}
            disabled={isFetching}
            aria-busy={isFetching}
          >
            <RefreshCw className={cn('h-4 w-4 mr-1', isFetching && 'animate-spin')} /> {tr("Refresh")}</Button>
        </div>
      </div>

      {backgroundError && data && (
        <StaleDataNotice
          message={backgroundError}
          onRetry={retryRefresh}
          isRetrying={isFetching}
        />
      )}

      <div className="flex shrink-0 flex-wrap items-end gap-3 rounded-lg border border-border bg-card p-3">
        <label className="flex min-w-0 flex-col gap-1 text-xs text-muted-foreground">
          <span className="flex items-center gap-1"><Filter className="h-3 w-3" /> {tr("Action")}</span>
          <select
            className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
            value={action}
            onChange={(e) => { setAction(e.target.value); resetPages(); }}
          >
            <option value=""> {tr("All")}</option>
            {ADMIN_AUDIT_ACTIONS.map((a) => (
              <option key={a} value={a}>{tr(ACTION_LABELS[a] ?? a)}</option>
            ))}
          </select>
        </label>
        <label className="flex min-w-0 flex-col gap-1 text-xs text-muted-foreground">
          <span> {tr("Outcome")}</span>
          <select
            className="h-9 rounded-md border border-input bg-background px-2 text-sm text-foreground"
            value={outcome}
            onChange={(e) => { setOutcome(e.target.value); resetPages(); }}
          >
            <option value=""> {tr("All")}</option>
            {ADMIN_AUDIT_OUTCOMES.map((o) => (
              <option key={o} value={o}>{tr(o)}</option>
            ))}
          </select>
        </label>
        <label className="flex min-w-0 flex-col gap-1 text-xs text-muted-foreground">
          <span>{tr('Manager administrator')}</span>
          <select aria-label={tr('Manager administrator')} className="h-9 max-w-64 rounded-md border bg-background px-2" value={actorUserId} onChange={event => { setActorUserId(event.target.value); resetPages(); }}>
            <option value="">{tr('All administrators')}</option>
            {Array.isArray(actorsQuery.data) && actorsQuery.data.map((actor: any) => <option key={actor.id} value={actor.id}>{actor.displayName || actor.username} (@{actor.username}, #{actor.id})</option>)}
            {Array.from(new Map((data?.items || []).filter(row => row.actorUserId != null).map(row => [row.actorUserId, row])).values()).filter(row => !Array.isArray(actorsQuery.data) || !actorsQuery.data.some((actor: any) => actor.id === row.actorUserId)).map(row => <option key={row.actorUserId} value={row.actorUserId!}>{row.actorUsername || tr('Deleted user')} (#{row.actorUserId})</option>)}
          </select>
        </label>
        <label className="flex min-w-0 flex-col gap-1 text-xs text-muted-foreground">
          <span>{tr('Connection')}</span>
          <select aria-label={tr('Connection')} className="h-9 max-w-64 rounded-md border bg-background px-2" value={connectionId} onChange={event => { setConnectionId(event.target.value); resetPages(); }}>
            <option value="">{tr('All connections')}</option>{Array.isArray(connectionsQuery.data) && connectionsQuery.data.map((connection: any) => <option key={connection.id} value={connection.id}>{connection.name} (#{connection.id})</option>)}
          </select>
        </label>
        <label className="flex min-w-0 flex-col gap-1 text-xs text-muted-foreground">
          <span> {tr("Virtual server ID")}</span>
          <Input
            className="h-9 w-28"
            inputMode="numeric"
            value={virtualServerId}
            onChange={(e) => { setVirtualServerId(e.target.value.replace(/\D/g, '')); resetPages(); }}
            placeholder={tr("Any")}
          />
        </label>
      </div>

      <div className="relative flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-border bg-card shadow-sm">
        {isLoading ? (
          <PageLoader />
        ) : isError && !data ? (
          <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-sm text-destructive">
            <p>{backgroundError}</p>
            <Button size="sm" variant="outline" onClick={retryRefresh} disabled={isFetching}>
              {isFetching ? tr("Retrying…") : tr("Retry")}
            </Button>
          </div>
        ) : !data?.items.length ? (
          <div className="flex flex-1 items-center justify-center">
            <EmptyState
              icon={ClipboardList}
              title={tr("No audit events yet")}
              description={tr("Kick, ban, connection, flow, user, and settings mutations that are instrumented will appear here.")}
            />
          </div>
        ) : (
          <ScrollArea className="flex-1">
            <div className={cn('min-w-0', isFetching && 'opacity-70')}>
              <div
                className={cn(
                  AUDIT_GRID,
                  'sticky top-0 z-10 border-b border-border bg-card/95 px-4 py-2 text-[10px] font-medium uppercase tracking-wide text-muted-foreground backdrop-blur-sm',
                )}
                role="row"
              >
                <span> {tr("Date / time")}{' '}</span>
                <span> {tr("Action")}</span>
                <span> {tr("Actor")}{' '}</span>
                <span> {tr("Target")}{' '}</span>
                <span> {tr("Context")}{' '}</span>
                <span> {tr("Outcome")}</span>
              </div>
              {data.items.map((event) => (
                <EventRow key={event.id} event={event} connections={Array.isArray(connectionsQuery.data) ? connectionsQuery.data : []} />
              ))}
            </div>
          </ScrollArea>
        )}
      </div>

      <div className="flex shrink-0 items-center justify-between gap-2">
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={cursorStack.length <= 1 || isFetching}
          onClick={() => setPageState((prev) => ({
            scopeKey: filterScopeKey,
            cursorStack: prev.scopeKey === filterScopeKey ? prev.cursorStack.slice(0, -1) : [null],
          }))}
        > {tr("Newer")}{' '}</Button>
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={!data?.nextCursor || isFetching}
          onClick={() => {
            if (data?.nextCursor) {
              setPageState((prev) => ({
                scopeKey: filterScopeKey,
                cursorStack: prev.scopeKey === filterScopeKey
                  ? [...prev.cursorStack, data.nextCursor]
                  : [null, data.nextCursor],
              }));
            }
          }}
        > {tr("Older")}</Button>
      </div>
    </div>
  );
}
