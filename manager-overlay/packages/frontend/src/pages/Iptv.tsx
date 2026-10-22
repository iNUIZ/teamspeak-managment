import { translateUiError as uiError } from '@/lib/i18n';
import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';
import { RuntimeIssue } from '@/components/shared/RuntimeIssue';
import { useState, useEffect, useRef } from 'react';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import {
  useIptvPlaylists, useCreateIptvPlaylist, useUploadIptvPlaylist, useReplaceIptvPlaylistFile,
  useDeleteIptvPlaylist, useRefreshIptvPlaylist,
  useIptvGroups, useIptvChannels,
} from '@/hooks/use-iptv';
import { useMusicBots } from '@/hooks/use-music-bots';
import { useServers } from '@/hooks/use-servers';
import { useServerStore } from '@/stores/server.store';
import { PageLoader } from '@/components/shared/LoadingSpinner';
import { EmptyState } from '@/components/shared/EmptyState';
import { ConfirmDialog } from '@/components/shared/ConfirmDialog';
import { PageHeader } from '@/components/shared/PageHeader';
import { RefreshStatus, StaleDataNotice } from '@/components/shared/RefreshStatus';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { IptvLocalHostsDialog, IptvLocalHostsTrigger } from '@/components/iptv/IptvLocalHostsDialog';
import { useAuthStore } from '@/stores/auth.store';
import { Badge } from '@/components/ui/badge';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter, DialogDescription } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { Tv, Plus, Trash2, RefreshCw, Play, Search, ChevronLeft, ChevronRight, Loader2, Radio, AlertCircle, Upload, Link2, FileUp } from 'lucide-react';
import { toast } from 'sonner';
import type { IptvPlaylistSummary, IptvChannelInfo, IptvChannelPage, MusicBotSummary } from '@ts6/common';
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuTrigger } from '@/components/ui/dropdown-menu';
import { iptvConsolePath } from '@/pages/bot-hub/iptv-deep-link';
import { formatLocalDateTime, formatNumber } from '@/lib/formatting';
import { apiErrorMessage } from '@/lib/api-error';
import { RuntimeMediaDiagnostics } from '@/components/media/RuntimeMediaDiagnostics';

const ACCEPT_PLAYLIST = '.m3u,.m3u8,.txt,audio/x-mpegurl,application/vnd.apple.mpegurl,text/plain';

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

// ─── Channel browser ─────────────────────────────────────────────────────────

function ChannelBrowser({ playlist, bots }: {
  playlist: IptvPlaylistSummary;
  bots: MusicBotSummary[];
}) {
  useUiLanguageStore(state => state.language);
  const navigate = useNavigate();
  const [search, setSearch] = useState('');
  const [debounced, setDebounced] = useState('');
  const [group, setGroup] = useState('');
  const [page, setPage] = useState(1);
  const pageSize = 24;

  useEffect(() => {
    const t = setTimeout(() => { setDebounced(search); setPage(1); }, 300);
    return () => clearTimeout(t);
  }, [search]);

  const { data: groups } = useIptvGroups(playlist.id);
  const groupList: string[] = Array.isArray(groups) ? groups : [];
  const { data, isFetching } = useIptvChannels(playlist.id, {
    search: debounced || undefined,
    group: group || undefined,
    page,
    pageSize,
  }) as { data: IptvChannelPage | undefined; isFetching: boolean };

  // Any bot on this playlist's server can stream it; the console starts a stopped bot.
  const serverBots = bots.filter((b) => b.serverConfigId === playlist.serverConfigId);

  const channels: IptvChannelInfo[] = data?.channels ?? [];
  const total = data?.total ?? 0;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));

  return (
    <div className="space-y-3">
      <RuntimeMediaDiagnostics
        focus={['sidecar', 'ffmpeg', 'yt-dlp']}
        showPrerequisite
      />
      {serverBots.length === 0 ? (
        <p className="text-[11px] text-amber-500 flex items-center gap-1">
          <AlertCircle className="h-3.5 w-3.5" />
          <span>{uiText("Create a media bot on this server in the ")}<Link to="/bot-hub" className="underline underline-offset-2">{uiText("Bot Hub")}</Link> {uiText(" to stream.")}</span>
        </p>
      ) : (
        <p className="text-xs text-muted-foreground px-0.5">
          {uiText("Stream on… opens that bot's console with the channel selected. Press Stream there to start.")}</p>
      )}

      {/* Filters */}
      <div className="flex flex-wrap gap-2">
        <div className="relative flex-1 min-w-[180px]">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
          <Input
            aria-label={uiText("Search IPTV channels")}
            placeholder={uiText("Search channels...")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="h-8 text-xs pl-8"
          />
        </div>
        {groupList.length > 0 && (
          <Select value={group || '__all__'} onValueChange={(v) => { setGroup(v === '__all__' ? '' : v); setPage(1); }}>
            <SelectTrigger className="h-10 w-full text-xs sm:h-8 sm:w-52"><SelectValue placeholder={uiText("All groups")} /></SelectTrigger>
            <SelectContent>
              <SelectItem value="__all__">{uiText("All groups")}</SelectItem>
              {groupList.map((g) => <SelectItem key={g} value={g}>{g}</SelectItem>)}
            </SelectContent>
          </Select>
        )}
      </div>

      {/* Channel grid */}
      {channels.length === 0 ? (
        <p className="text-xs text-muted-foreground py-6 text-center">
          {isFetching ? uiText("Loading…") : uiText("No channels match your filters.")}
        </p>
      ) : (
        <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
          {channels.map((c) => (
            <div key={c.id} className="flex items-center gap-2 rounded-md border p-2 group hover:border-primary/40 transition-colors">
              <div className="h-9 w-9 rounded bg-muted flex items-center justify-center shrink-0 overflow-hidden">
                {c.logo
                  ? <img src={c.logo} alt="" className="h-full w-full object-contain" onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }} />
                  : <Radio className="h-4 w-4 text-muted-foreground" />}
              </div>
              <div className="min-w-0 flex-1">
                <p className="text-xs font-medium truncate">{c.name}</p>
                {c.groupTitle && <p className="text-[10px] text-muted-foreground truncate">{c.groupTitle}</p>}
              </div>
              <DropdownMenu modal={false}>
                <DropdownMenuTrigger asChild>
                  <Button
                    size="sm"
                    variant="ghost"
                    className="h-9 shrink-0 px-2 text-xs"
                    disabled={serverBots.length === 0}
                    aria-label={uiText("Stream {0} on…", c.name)}
                  >
                    <Play className="h-3.5 w-3.5 mr-1" aria-hidden="true" /> {uiText(" Stream on…")}</Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end">
                  <DropdownMenuLabel className="text-xs font-medium text-muted-foreground">{uiText("Open in a bot's console")}</DropdownMenuLabel>
                  {serverBots.map((b) => (
                    <DropdownMenuItem key={b.id} onSelect={() => navigate(iptvConsolePath(b.id, playlist.id, c))}>
                      {b.name}
                      <span className="ml-2 text-[10px] capitalize text-muted-foreground">{b.status}</span>
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
            </div>
          ))}
        </div>
      )}

      {/* Pagination */}
      <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
        <span>{formatNumber(total)} {uiText(" channels")}{isFetching ? uiText(" · updating…") : ''}</span>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="icon" className="h-7 w-7" aria-label={uiText("Previous channel page")} disabled={page <= 1} onClick={() => setPage((p) => p - 1)}>
            <ChevronLeft className="h-3.5 w-3.5" />
          </Button>
          <span className="tabular-nums">{page} / {totalPages}</span>
          <Button variant="outline" size="icon" className="h-7 w-7" aria-label={uiText("Next channel page")} disabled={page >= totalPages} onClick={() => setPage((p) => p + 1)}>
            <ChevronRight className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
    </div>
  );
}

// ─── Add playlist dialog ─────────────────────────────────────────────────────

function AddPlaylistDialog({ open, onClose, serverConfigId }: { open: boolean; onClose: () => void; serverConfigId: number | null }) {
  useUiLanguageStore(state => state.language);
  const create = useCreateIptvPlaylist();
  const upload = useUploadIptvPlaylist();
  const [source, setSource] = useState<'url' | 'upload'>('url');
  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [refresh, setRefresh] = useState('0');
  const [file, setFile] = useState<File | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const pending = create.isPending || upload.isPending;

  const reset = () => {
    setName('');
    setUrl('');
    setRefresh('0');
    setFile(null);
    setSource('url');
    if (fileInputRef.current) fileInputRef.current.value = '';
  };

  const submit = () => {
    if (!serverConfigId) { toast.error(uiText("Select a server first")); return; }
    if (!name.trim()) { toast.error(uiText("Name is required")); return; }

    if (source === 'url') {
      if (!url.trim()) { toast.error(uiText("M3U URL is required")); return; }
      create.mutate(
        { name: name.trim(), url: url.trim(), serverConfigId, autoRefreshMinutes: parseInt(refresh) || 0 },
        {
          onSuccess: (res: any) => {
            if (res?.refreshError) toast.warning(uiText("Playlist added, but refresh failed: {0}", res.refreshError));
            else toast.success(uiText("Playlist added — {0} channels", formatNumber(Number(res?.channelCount ?? 0))));
            reset();
            onClose();
          },
          onError: (e: any) => toast.error(uiError(String(e?.response?.data?.error || uiText("Failed to add playlist")), uiText("Failed to add playlist"))),
        },
      );
      return;
    }

    if (!file) { toast.error(uiText("Choose a playlist file to upload")); return; }
    upload.mutate(
      { name: name.trim(), serverConfigId, file },
      {
        onSuccess: (res: any) => {
          toast.success(uiText("Playlist uploaded — {0} channels", formatNumber(Number(res?.channelCount ?? 0))));
          reset();
          onClose();
        },
        onError: (e: any) => toast.error(uiError(String(e?.response?.data?.error || uiText("Failed to upload playlist")), uiText("Failed to upload playlist"))),
      },
    );
  };

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) { reset(); onClose(); } }}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{uiText("Add IPTV Playlist")}</DialogTitle>
          <DialogDescription>
            {uiText("Add a remote M3U/M3U8 URL or upload a playlist file stored on this manager.")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-3 py-1">
          <Tabs value={source} onValueChange={(v) => setSource(v as 'url' | 'upload')}>
            <TabsList className="w-full">
              <TabsTrigger value="url" className="flex-1">
                <Link2 className="h-3.5 w-3.5 mr-1.5" /> {uiText(" Playlist URL")}</TabsTrigger>
              <TabsTrigger value="upload" className="flex-1">
                <Upload className="h-3.5 w-3.5 mr-1.5" /> {uiText(" Upload file")}</TabsTrigger>
            </TabsList>
          </Tabs>

          <div className="space-y-1.5">
            <Label htmlFor="iptv-playlist-name" className="text-xs">{uiText("Name")}</Label>
            <Input id="iptv-playlist-name" value={name} onChange={(e) => setName(e.target.value)} placeholder={uiText("My IPTV")} autoFocus />
          </div>

          {source === 'url' ? (
            <>
              <div className="space-y-1.5">
                <Label htmlFor="iptv-playlist-url" className="text-xs">{uiText("M3U URL")}</Label>
                <Input id="iptv-playlist-url" value={url} onChange={(e) => setUrl(e.target.value)} placeholder="http://provider/get.php?...&type=m3u_plus" />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="iptv-playlist-refresh" className="text-xs">{uiText("Auto-refresh (minutes, 0 = manual)")}</Label>
                <Input id="iptv-playlist-refresh" type="number" min={0} value={refresh} onChange={(e) => setRefresh(e.target.value)} />
              </div>
            </>
          ) : (
            <div className="space-y-1.5">
              <Label htmlFor="iptv-playlist-file" className="text-xs">{uiText("Playlist file")}</Label>
              <Input
                id="iptv-playlist-file"
                ref={fileInputRef}
                type="file"
                accept={ACCEPT_PLAYLIST}
                className="cursor-pointer text-xs file:mr-3"
                onChange={(e) => setFile(e.target.files?.[0] ?? null)}
              />
              <p className="text-[11px] text-muted-foreground">
                {uiText("Accepts .m3u, .m3u8, or .txt containing valid M3U entries. Max 64 MB.")}</p>
              {file && (
                <p className="text-[11px] text-muted-foreground truncate">
                  {uiText("Selected: ")}{file.name} ({formatFileSize(file.size)})
                </p>
              )}
            </div>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => { reset(); onClose(); }}>{uiText("Cancel")}</Button>
          <Button onClick={submit} disabled={pending} aria-busy={pending}>
            {pending
              ? <><Loader2 className="h-4 w-4 mr-1.5 animate-spin" /> {source === 'upload' ? uiText("Uploading…") : uiText("Adding…")}</>
              : (source === 'upload' ? uiText("Upload & Load") : uiText("Add & Load"))}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ─── Replace uploaded file dialog ────────────────────────────────────────────

function ReplaceFileDialog({
  playlist,
  open,
  onClose,
}: {
  playlist: IptvPlaylistSummary | null;
  open: boolean;
  onClose: () => void;
}) {
  useUiLanguageStore(state => state.language);
  const replace = useReplaceIptvPlaylistFile();
  const [file, setFile] = useState<File | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const reset = () => {
    setFile(null);
    if (fileInputRef.current) fileInputRef.current.value = '';
  };

  const submit = () => {
    if (!playlist || !file) { toast.error(uiText("Choose a playlist file")); return; }
    replace.mutate(
      { id: playlist.id, file },
      {
        onSuccess: (res: any) => {
          toast.success(uiText("File replaced — {0} channels", formatNumber(Number(res?.channelCount ?? 0))));
          reset();
          onClose();
        },
        onError: (e: any) => toast.error(uiError(String(e?.response?.data?.error || uiText("Failed to replace file")), uiText("Failed to replace file"))),
      },
    );
  };

  return (
    <Dialog open={open} onOpenChange={(o) => { if (!o) { reset(); onClose(); } }}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{uiText("Replace playlist file")}</DialogTitle>
          <DialogDescription>
            {uiText("Upload a new M3U/M3U8 file for “")}{playlist?.name}{uiText("”. Channels are re-parsed after a successful upload.")}</DialogDescription>
        </DialogHeader>
        <div className="space-y-1.5 py-1">
          <Label htmlFor="iptv-replace-file" className="text-xs">{uiText("New playlist file")}</Label>
          <Input
            id="iptv-replace-file"
            ref={fileInputRef}
            type="file"
            accept={ACCEPT_PLAYLIST}
            className="cursor-pointer text-xs file:mr-3"
            onChange={(e) => setFile(e.target.files?.[0] ?? null)}
          />
          {file && (
            <p className="text-[11px] text-muted-foreground truncate">
              {uiText("Selected: ")}{file.name} ({formatFileSize(file.size)})
            </p>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => { reset(); onClose(); }}>{uiText("Cancel")}</Button>
          <Button onClick={submit} disabled={!file || replace.isPending} aria-busy={replace.isPending}>
            {replace.isPending
              ? <><Loader2 className="h-4 w-4 mr-1.5 animate-spin" /> {uiText(" Replacing…")}</>
              : uiText("Replace & Refresh")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

// ─── Page ────────────────────────────────────────────────────────────────────

export default function Iptv() {
  useUiLanguageStore(state => state.language);
  const [searchParams] = useSearchParams();
  const linkedServer = Number(searchParams.get('server')) || null;
  const { data: servers } = useServers();
  const serverList = Array.isArray(servers) ? servers : [];
  const { selectedConfigId, setServer } = useServerStore();
  const isAdmin = useAuthStore((state) => state.isAdmin());

  // Prefer ?server= from Media Bot Play, else default to the first server.
  useEffect(() => {
    if (linkedServer) {
      setServer(linkedServer);
      return;
    }
    if (!selectedConfigId && serverList.length > 0) setServer(serverList[0].id);
  }, [serverList, selectedConfigId, setServer, linkedServer]);

  const { data: playlists, isLoading, error, refetch, isFetching } = useIptvPlaylists(selectedConfigId ?? undefined);
  const { data: bots } = useMusicBots();
  const botList: MusicBotSummary[] = Array.isArray(bots) ? bots : [];

  const deletePlaylist = useDeleteIptvPlaylist();
  const refreshPlaylist = useRefreshIptvPlaylist();

  const [addOpen, setAddOpen] = useState(false);
  const [hostsOpen, setHostsOpen] = useState(false);
  const [replaceTarget, setReplaceTarget] = useState<IptvPlaylistSummary | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<IptvPlaylistSummary | null>(null);
  const [deleteError, setDeleteError] = useState('');
  const [selectedPlaylistId, setSelectedPlaylistId] = useState<number | null>(null);

  if (isLoading && playlists === undefined) return <PageLoader />;
  if (error && playlists === undefined) {
    return (
      <div className="space-y-4">
        <EmptyState icon={Tv} title={uiText("Could not load IPTV playlists")} description={apiErrorMessage(error, 'Check the selected connection and try again.')} />
        <div className="flex justify-center">
          <Button size="sm" variant="outline" onClick={() => { void refetch(); }} disabled={isFetching} aria-busy={isFetching}>
            {isFetching ? uiText("Retrying…") : uiText("Retry")}
          </Button>
        </div>
      </div>
    );
  }

  const playlistList: IptvPlaylistSummary[] = Array.isArray(playlists) ? playlists : [];
  const selectedPlaylist = playlistList.find((p) => p.id === selectedPlaylistId) ?? playlistList[0] ?? null;
  const backgroundError = error
    ? apiErrorMessage(error, 'Playlist refresh failed. The last successful list is still displayed.')
    : null;

  return (
    <div className="space-y-6">
      <PageHeader
        title={uiText("IPTV")}
        icon={Tv}
        description={uiText("Stream live IPTV channels into TeamSpeak via a Media Bot's video sidecar.")}
        actions={(
          <>
            <Select value={selectedConfigId ? String(selectedConfigId) : ''} onValueChange={(v) => { setServer(parseInt(v)); setSelectedPlaylistId(null); }}>
              <SelectTrigger aria-label={uiText("IPTV server")} className="h-10 min-w-0 flex-1 sm:h-9 sm:w-48 sm:flex-none"><SelectValue placeholder={uiText("Select server")} /></SelectTrigger>
              <SelectContent>
                {serverList.map((s: any) => <SelectItem key={s.id} value={String(s.id)}>{s.name}</SelectItem>)}
              </SelectContent>
            </Select>
            {isAdmin && <IptvLocalHostsTrigger onClick={() => setHostsOpen(true)} />}
            <Button onClick={() => setAddOpen(true)} disabled={!selectedConfigId}>
              <Plus className="h-4 w-4 mr-1.5" /> {uiText(" Add Playlist")}</Button>
          </>
        )}
        metadata={<RefreshStatus isRefreshing={isFetching} idleLabel={uiText("Playlist data up to date")} refreshingLabel={uiText("Refreshing playlists…")} />}
      />

      {backgroundError && (
        <StaleDataNotice message={backgroundError} onRetry={() => { void refetch(); }} isRetrying={isFetching} />
      )}

      {playlistList.length === 0 ? (
        <EmptyState
          icon={Tv}
          title={uiText("No IPTV playlists")}
          description={uiText("Add an M3U/M3U8 playlist URL or upload a playlist file to browse channels and stream them into a TeamSpeak channel.")}
        >
          <div className="flex flex-col items-center gap-3">
            <Button onClick={() => setAddOpen(true)} disabled={!selectedConfigId}>
              <Plus className="h-4 w-4 mr-1.5" /> {uiText(" Add Playlist")}</Button>
            {isAdmin && (
              <button
                type="button"
                className="text-xs text-muted-foreground underline-offset-4 hover:underline"
                onClick={() => setHostsOpen(true)}
              >
                {uiText("Using Threadfin, xTeVe or TVHeadend? Allow its host first.")}</button>
            )}
          </div>
        </EmptyState>
      ) : (
        <>
          {/* Playlist cards */}
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {playlistList.map((p) => {
              const isUpload = p.sourceType === 'upload';
              return (
              <Card
                key={p.id}
                className={`cursor-pointer transition-colors ${selectedPlaylist?.id === p.id ? 'border-primary/50' : 'hover:border-primary/30'}`}
                onClick={() => setSelectedPlaylistId(p.id)}
                onKeyDown={(event) => {
                  if (event.target !== event.currentTarget || (event.key !== 'Enter' && event.key !== ' ')) return;
                  event.preventDefault();
                  setSelectedPlaylistId(p.id);
                }}
                role="button"
                tabIndex={0}
                aria-label={uiText("View {0} channels", p.name)}
                aria-pressed={selectedPlaylist?.id === p.id}
              >
                <CardHeader className="pb-2">
                  <div className="flex items-center justify-between">
                    <CardTitle className="text-sm truncate">{p.name}</CardTitle>
                    <div className="flex items-center gap-1">
                      {isUpload && (
                        <Button
                          variant="ghost" size="icon" className="h-7 w-7"
                          onClick={(e) => { e.stopPropagation(); setReplaceTarget(p); }}
                          aria-label={uiText("Replace file for {0}", p.name)}
                          title={uiText("Replace file")}
                        >
                          <FileUp className="h-3.5 w-3.5" />
                        </Button>
                      )}
                      <Button
                        variant="ghost" size="icon" className="h-7 w-7"
                        onClick={(e) => { e.stopPropagation(); refreshPlaylist.mutate(p.id, {
                          onSuccess: (r: any) => toast.success(uiText("Refreshed — {0} channels", formatNumber(Number(r.channelCount)))),
                          onError: (err: any) => toast.error(uiError(String(err?.response?.data?.error || uiText("Refresh failed")), uiText("Refresh failed"))),
                        }); }}
                        disabled={refreshPlaylist.isPending}
                        aria-label={uiText("Refresh {0}", p.name)}
                        title={isUpload ? uiText("Re-parse stored file") : uiText("Re-fetch URL")}
                      >
                        <RefreshCw className={`h-3.5 w-3.5 ${refreshPlaylist.isPending ? 'animate-spin' : ''}`} />
                      </Button>
                      <Button
                        variant="ghost" size="icon" className="h-7 w-7 text-destructive"
                        onClick={(e) => { e.stopPropagation(); setDeleteTarget(p); }}
                        aria-label={uiText("Delete {0}", p.name)}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  </div>
                </CardHeader>
                <CardContent className="space-y-1.5">
                  <div className="flex items-center gap-2 flex-wrap">
                    <Badge variant="secondary" className="text-[10px]">{formatNumber(p.channelCount)} {uiText(" channels")}</Badge>
                    <Badge variant="outline" className="text-[10px]">
                      {isUpload ? uiText("Uploaded file") : uiText("URL")}
                    </Badge>
                    {isUpload && p.originalFilename && (
                      <Badge variant="outline" className="text-[10px] max-w-[10rem] truncate" title={p.originalFilename}>
                        {p.originalFilename}
                      </Badge>
                    )}
                    {!isUpload && p.autoRefreshMinutes > 0 && (
                      <Badge variant="outline" className="text-[10px]">{uiText('Auto refresh: {0} min', p.autoRefreshMinutes)}</Badge>
                    )}
                  </div>
                  {p.lastError
                    ? <RuntimeIssue message={p.lastError} fallback="Playlist refresh failed. The last successful list is still displayed." className="text-[10px] text-destructive" />
                    : <p className="text-[10px] text-muted-foreground">{p.lastRefreshedAt ? uiText("Updated {0}", formatLocalDateTime(p.lastRefreshedAt)) : uiText("Not refreshed yet")}</p>}
                </CardContent>
              </Card>
              );
            })}
          </div>

          {/* Channel browser for selected playlist */}
          {selectedPlaylist && (
            <Card>
              <CardHeader className="pb-3">
                <CardTitle className="text-sm flex items-center gap-2"><Radio className="h-4 w-4" /> {selectedPlaylist.name} {uiText(" — Channels")}</CardTitle>
              </CardHeader>
              <CardContent>
                <ChannelBrowser playlist={selectedPlaylist} bots={botList} />
              </CardContent>
            </Card>
          )}
        </>
      )}

      {isAdmin && <IptvLocalHostsDialog open={hostsOpen} onOpenChange={setHostsOpen} />}

      <AddPlaylistDialog open={addOpen} onClose={() => setAddOpen(false)} serverConfigId={selectedConfigId} />
      <ReplaceFileDialog
        playlist={replaceTarget}
        open={!!replaceTarget}
        onClose={() => setReplaceTarget(null)}
      />

      <ConfirmDialog
        open={!!deleteTarget}
        onOpenChange={(o) => { if (!o) { setDeleteTarget(null); setDeleteError(''); } }}
        title={uiText("Delete playlist?")}
        description={uiText("This removes \"{0}\" and all its channels{1}.", deleteTarget?.name, deleteTarget?.sourceType === 'upload' ? uiText(", including the stored playlist file") : '')}
        confirmLabel={uiText("Delete")}
        destructive
        loading={deletePlaylist.isPending}
        error={deleteError}
        onConfirm={() => {
          if (deleteTarget) {
            setDeleteError('');
            deletePlaylist.mutate(deleteTarget.id, {
              onSuccess: () => {
                toast.success(uiText("Playlist deleted"));
                setDeleteError('');
                setDeleteTarget(null);
              },
              onError: (e: unknown) => {
                const message = apiErrorMessage(e, 'Failed to delete playlist');
                setDeleteError(message);
                toast.error(message);
              },
            });
          }
        }}
      />
    </div>
  );
}
