import { translate as tr, useTranslation as useUiTranslation } from '@/lib/i18n';
import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { filesApi } from '@/api/files.api';
import { channelsApi } from '@/api/channels.api';
import { useServerStore } from '@/stores/server.store';
import { useAuthStore } from '@/stores/auth.store';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogFooter } from '@/components/ui/dialog';
import { ConfirmDialog } from '@/components/shared/ConfirmDialog';
import { PageLoader } from '@/components/shared/LoadingSpinner';
import { EmptyState } from '@/components/shared/EmptyState';
import { NoServerSelectedState } from '@/components/shared/NoServerSelectedState';
import {
  buildFilePath,
  canConfirmFileAction,
  shouldCloseFileDialog,
  type FileActionTarget,
} from '@/lib/action-ownership';
import {
  channelCoverageKey,
  channelSummaryDisplay,
  channelSummaryLabelText,
  connectionScope,
  consumePageEntryAttempt,
  deferCancelSummaryQuery,
  expensiveDiagnosticQueryOptions,
  markExpensiveDiagnosticStale,
  retainSummaryQueryScope,
  type SummaryObservation,
} from '@/lib/demand-driven-query-policy';
import {
  FILE_WRITE_READ_DOES_NOT_AUTHORIZE,
  fileBrowseErrorMessage,
  fileWriteErrorMessage,
} from '@/lib/action-guidance';
import { cn, formatBytes } from '@/lib/utils';
import { Link } from 'react-router-dom';
import {
  isCountedSummary,
  isPartialSummary,
  selectChannelsForSummaryScan,
  type ChannelFileSummaryResult,
} from '@/api/file-summary.types';
import {
  FolderOpen, File, Folder, ArrowLeft, FolderPlus, Trash2, Hash, HardDrive, AlertTriangle, RefreshCw, Upload, Download,
} from 'lucide-react';
import { toast } from 'sonner';
import { apiErrorMessage, isTeamSpeakSshDisconnected, teamSpeakQueryRetry, teamSpeakQueryRetryDelay } from '@/lib/api-error';

interface FileEntry {
  name: string;
  size: number;
  datetime: number;
  type: number; // 0 = file, 1 = directory
}

function transferErrorMessage(error: unknown, fallback: string): string {
  const data = (error as any)?.response?.data;
  if (data?.reason === 'ts_permission_denied') return tr('The Query identity is not allowed to transfer files in this channel.');
  if (isTeamSpeakSshDisconnected(error)) return tr('The Query connection is temporarily disconnected. Wait and retry.');
  return apiErrorMessage(error, fallback);
}

type SummaryQueryData = {
  summaries: ChannelFileSummaryResult[];
  observation: SummaryObservation;
};

export default function Files() {
  const { language } = useUiTranslation();
  const canManageFiles = useAuthStore(state => state.user?.role === 'admin');
  const { selectedConfigId: c, selectedSid: s } = useServerStore();
  const qc = useQueryClient();

  const [selectedCid, setSelectedCid] = useState<number | null>(null);
  const [currentPath, setCurrentPath] = useState('/');
  const [showMkdir, setShowMkdir] = useState(false);
  const [newDirName, setNewDirName] = useState('');
  const [deleteTarget, setDeleteTarget] = useState<FileActionTarget | null>(null);
  const [ownerGeneration, setOwnerGeneration] = useState(0);
  const [uploadProgress, setUploadProgress] = useState(0);
  const uploadInput = useRef<HTMLInputElement>(null);
  const deleteTargetRef = useRef<FileActionTarget | null>(null);
  deleteTargetRef.current = deleteTarget;

  // Fetch channel list for selector
  const { data: channelData } = useQuery({
    queryKey: ['channels-for-files', c, s],
    queryFn: () => channelsApi.list(c!, s!),
    enabled: !!c && !!s,
  });

  const channels = useMemo(() => {
    if (!channelData || !Array.isArray(channelData)) return [];
    return channelData.map((ch: any) => ({
      cid: Number(ch.cid),
      name: ch.channel_name,
    }));
  }, [channelData]);

  const channelIds = useMemo(() => channels.map((channel) => channel.cid), [channels]);
  const { scanIds, omittedIds } = useMemo(
    () => selectChannelsForSummaryScan(channelIds),
    [channelIds],
  );
  const eligibleChannelKey = useMemo(() => channelCoverageKey(scanIds), [scanIds]);
  const currentChannelKey = useMemo(() => channelCoverageKey(channelIds), [channelIds]);
  const scanIdsRef = useRef(scanIds);
  scanIdsRef.current = scanIds;
  const omittedIdsRef = useRef(omittedIds);
  omittedIdsRef.current = omittedIds;

  const summaryQueryKey = useMemo(() => ['file-summaries', c, s] as const, [c, s]);

  const {
    data: summaryPayload,
    isFetching: fetchingSummaries,
    isError: summaryIsError,
    refetch: refetchSummaries,
  } = useQuery({
    queryKey: summaryQueryKey,
    queryFn: async ({ signal }): Promise<SummaryQueryData> => {
      const ids = scanIdsRef.current;
      const omitted = omittedIdsRef.current;
      const response = await filesApi.summaries(c!, s!, ids, { signal });
      const summaries = Array.isArray(response?.summaries) ? response.summaries : [];
      // Observation covers only the bounded scan set; omitted IDs stay Not scanned.
      return {
        summaries: [
          ...summaries,
          // Ensure omitted channels appear as notScanned even if the server omitted them.
          ...omitted
            .filter((cid) => !summaries.some((row) => row.cid === cid))
            .map((cid) => ({ cid, notScanned: true as const, reason: 'channel-cap' as const })),
        ],
        observation: {
          scannedChannelKey: channelCoverageKey(ids),
          scannedAt: typeof response?.scannedAt === 'number' ? response.scannedAt : Date.now(),
        },
      };
    },
    enabled: false,
    ...expensiveDiagnosticQueryOptions,
  });

  // Disabled observers always report isStale=false; read invalidation from the cache.
  const summaryInvalidated = useSyncExternalStore(
    (onChange) => qc.getQueryCache().subscribe(onChange),
    () => qc.getQueryState(summaryQueryKey)?.isInvalidated ?? false,
    () => false,
  );

  const summaryData = summaryPayload?.summaries;
  const observation = summaryPayload?.observation ?? null;

  const summariesByChannel = useMemo(() => {
    const map = new Map<number, ChannelFileSummaryResult>();
    if (!Array.isArray(summaryData)) return map;
    for (const summary of summaryData) map.set(Number(summary.cid), summary);
    return map;
  }, [summaryData]);

  // Storage summaries are manual Refresh only. A page-entry scan issues rapid
  // ftgetfilelist across every channel and trips TeamSpeak Query flood (524),
  // which takes down the shared SSH session used for ordinary file browse.
  // Channels stay "Not checked." until the operator refreshes.

  // Connection revision: cancel in-flight summaries when the scope leaves.
  // Defer cancel so StrictMode same-scope remount can retain an in-flight scan.
  useEffect(() => {
    retainSummaryQueryScope(summaryQueryKey);
    return () => {
      deferCancelSummaryQuery(qc, summaryQueryKey);
    };
  }, [qc, summaryQueryKey]);

  const refreshSummaries = () => {
    if (!c || !s || channelIds.length === 0) return;
    consumePageEntryAttempt(connectionScope(c, s), false);
    // Compatible in-flight work is coalesced by TanStack Query on the same key.
    void refetchSummaries();
  };

  // Fetch files in selected channel + path
  // Ordinary list (not Slice 6 expensive diagnostic): retry through SSH flood cooldown.
  const {
    data: fileData,
    isLoading: loadingFiles,
    isFetching: fetchingFiles,
    error: filesError,
    refetch: refetchFiles,
  } = useQuery({
    queryKey: ['files', c, s, selectedCid, currentPath],
    queryFn: () => filesApi.list(c!, s!, selectedCid!, currentPath),
    enabled: !!c && !!s && !!selectedCid,
    // SSH disconnect: surface error + Retry immediately; recover via refetchInterval.
    // Other transient Query errors still use the shared retry budget.
    retry: (failureCount, error) => {
      if (isTeamSpeakSshDisconnected(error)) return false;
      return teamSpeakQueryRetry(failureCount, error);
    },
    retryDelay: teamSpeakQueryRetryDelay,
    // After SSH recovers mid-cooldown, keep probing without a sticky unavailable UI.
    refetchInterval: (query) => (
      isTeamSpeakSshDisconnected(query.state.error) ? 15_000 : false
    ),
  });

  const files: FileEntry[] = useMemo(() => {
    if (!fileData || !Array.isArray(fileData)) return [];
    return fileData.map((f: any) => ({
      name: f.name,
      size: Number(f.size) || 0,
      datetime: Number(f.datetime) > 1e12 ? Number(f.datetime) / 1000 : Number(f.datetime) || 0,
      // ServerQuery: 0 = directory, 1 = file. Keep the UI's internal enum explicit.
      type: Number(f.type) === 0 ? 1 : 0,
    })).sort((a: FileEntry, b: FileEntry) => {
      // Directories first, then alphabetical
      if (a.type !== b.type) return b.type - a.type;
      return a.name.localeCompare(b.name);
    });
  }, [fileData]);

  useEffect(() => {
    setOwnerGeneration((g) => g + 1);
    setSelectedCid(null);
    setCurrentPath('/');
    setShowMkdir(false);
    setNewDirName('');
    setDeleteTarget(null);
  }, [c, s]);

  useEffect(() => {
    setDeleteTarget(null);
  }, [selectedCid]);

  const mkdirMutation = useMutation({
    mutationFn: (target: FileActionTarget) =>
      filesApi.createDir(target.configId, target.sid, target.cid, target.fullPath),
    onSuccess: async (_data, target) => {
      toast.success(tr('Directory created'));
      if (_data?.renamed) toast.info(`${tr('The server used a compatible name:')} ${_data.name}`);
      qc.invalidateQueries({
        queryKey: ['files', target.configId, target.sid, target.cid],
      });
      // Drop an in-flight scan before invalidating so its completion cannot clear
      // isInvalidated and present pre-mutation counts as fresh.
      const summaryKey = ['file-summaries', target.configId, target.sid] as const;
      await qc.cancelQueries({ queryKey: summaryKey });
      await markExpensiveDiagnosticStale(qc, summaryKey);
      // Only dismiss mkdir UI when it still belongs to the submitted scope.
      if (
        c === target.configId
        && s === target.sid
        && selectedCid === target.cid
        && target.ownerGeneration === ownerGeneration
      ) {
        setShowMkdir(false);
        setNewDirName('');
      }
    },
    onError: (error) => toast.error(tr(fileWriteErrorMessage(error, 'create'))),
  });

  const deleteMutation = useMutation({
    mutationFn: (target: FileActionTarget) =>
      filesApi.delete(target.configId, target.sid, target.cid, target.fullPath),
    onSuccess: async (_data, target) => {
      toast.success(tr('File deleted'));
      qc.invalidateQueries({
        queryKey: ['files', target.configId, target.sid, target.cid],
      });
      const summaryKey = ['file-summaries', target.configId, target.sid] as const;
      await qc.cancelQueries({ queryKey: summaryKey });
      await markExpensiveDiagnosticStale(qc, summaryKey);
      if (shouldCloseFileDialog(deleteTargetRef.current, target)) {
        setDeleteTarget(null);
      }
    },
    onError: (error) => toast.error(tr(fileWriteErrorMessage(error, 'delete'))),
  });

  const uploadMutation = useMutation({
    mutationFn: ({ target, file }: { target: FileActionTarget; file: globalThis.File }) =>
      filesApi.upload(target.configId, target.sid, target.cid, target.fullPath, file, setUploadProgress),
    onSuccess: async (data, { target }) => {
      toast.success(tr('File uploaded and verified'));
      if (data?.renamed) toast.info(`${tr('The server used a compatible name:')} ${data.name}`);
      await qc.invalidateQueries({ queryKey: ['files', target.configId, target.sid, target.cid] });
      const key = ['file-summaries', target.configId, target.sid] as const;
      await qc.cancelQueries({ queryKey: key }); await markExpensiveDiagnosticStale(qc, key);
    },
    onError: (error) => toast.error(tr(transferErrorMessage(error, 'File upload failed'))),
    onSettled: () => setUploadProgress(0),
  });
  const downloadMutation = useMutation({
    mutationFn: async (target: FileActionTarget) => ({ target, blob: await filesApi.download(target.configId, target.sid, target.cid, target.fullPath) }),
    onSuccess: ({ target, blob }) => {
      const url = URL.createObjectURL(blob); const link = document.createElement('a');
      link.href = url; link.download = target.entryName; document.body.appendChild(link); link.click(); link.remove();
      setTimeout(() => URL.revokeObjectURL(url), 30_000);
    },
    onError: (error) => toast.error(tr(transferErrorMessage(error, 'File download failed'))),
  });

  const targetFor = (entryName: string, fullPath: string): FileActionTarget => ({ configId: c!, sid: s!, cid: selectedCid!, fullPath, entryName, ownerGeneration });
  const handleUpload = (file?: globalThis.File) => {
    if (!file || !c || !s || selectedCid == null) return;
    if (file.size > 25 * 1024 * 1024) { toast.error(tr('File must be at most 25 MB')); return; }
    uploadMutation.mutate({ target: targetFor(file.name, currentPath), file });
  };

  const navigateTo = (entry: FileEntry) => {
    if (entry.type === 1) {
      setCurrentPath(buildFilePath(currentPath, entry.name));
    }
  };

  const goUp = () => {
    if (currentPath === '/') return;
    const parts = currentPath.split('/').filter(Boolean);
    parts.pop();
    setCurrentPath(parts.length === 0 ? '/' : '/' + parts.join('/'));
  };

  const openDelete = (entry: FileEntry) => {
    if (!c || !s || selectedCid == null) return;
    setDeleteTarget({
      configId: c,
      sid: s,
      cid: selectedCid,
      fullPath: buildFilePath(currentPath, entry.name),
      ownerGeneration,
      entryName: entry.name,
    });
  };

  const handleMkdir = () => {
    if (!newDirName.trim() || !c || !s || selectedCid == null) return;
    const target: FileActionTarget = {
      configId: c,
      sid: s,
      cid: selectedCid,
      fullPath: buildFilePath(currentPath, newDirName.trim()),
      ownerGeneration,
      entryName: newDirName.trim(),
    };
    mkdirMutation.mutate(target);
  };

  const handleDelete = () => {
    if (!canConfirmFileAction(deleteTarget, { configId: c, sid: s, cid: selectedCid })) {
      // Context switched away from the dialog owner — do not retarget to live B.
      setDeleteTarget(null);
      toast.error(tr('Server context changed — delete cancelled'));
      return;
    }
    deleteMutation.mutate(deleteTarget);
  };

  const deleteLoading = Boolean(
    deleteMutation.isPending
    && deleteMutation.variables
    && deleteTarget
    && shouldCloseFileDialog(deleteTarget, deleteMutation.variables),
  );

  const formatDate = (ts: number) => {
    if (!ts) return '-';
    return new Date(ts * 1000).toLocaleDateString(language === 'tr' ? 'tr-TR' : 'en-GB', {
      day: '2-digit', month: '2-digit', year: 'numeric',
      hour: '2-digit', minute: '2-digit',
    });
  };

  // Breadcrumb parts
  const pathParts = currentPath.split('/').filter(Boolean);

  if (!c || !s) return <NoServerSelectedState pageTitle={tr("File Browser")} icon={FolderOpen} />;

  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <h1 className="text-xl font-semibold"> {tr("File Browser")}</h1>
        {selectedCid && canManageFiles && (
          <div className="flex flex-wrap items-center gap-2">
          <input ref={uploadInput} type="file" aria-label={tr('Choose file to upload')} className="hidden" onChange={e => { handleUpload(e.target.files?.[0]); e.target.value = ''; }} />
          <Button size="sm" variant="outline" onClick={() => uploadInput.current?.click()} disabled={loadingFiles || !!filesError || uploadMutation.isPending}>
            <Upload className="h-4 w-4 mr-1" />{uploadMutation.isPending ? `${tr('Uploading and verifying…')} ${uploadProgress}%` : tr('Upload file')}
          </Button>
          <Button size="sm" onClick={() => setShowMkdir(true)} disabled={loadingFiles || !!filesError}>
            <FolderPlus className="h-4 w-4 mr-1" /> {tr("New Folder")}</Button>
          </div>
        )}
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-12">
        {/* Channel Selector */}
        <Card className="lg:col-span-3">
          <CardHeader className="pb-2">
            <CardTitle className="flex items-center justify-between text-xs font-medium uppercase tracking-wider text-muted-foreground">
              <span className="flex items-center gap-1.5"><Hash className="h-3.5 w-3.5" /> {tr("Channels")}</span>
              <span className="flex items-center gap-1">
                {fetchingSummaries && <RefreshCw className="h-3 w-3 animate-spin text-sky-400" aria-label={tr('Scanning storage')} />}
                <Button
                  type="button"
                  variant="ghost"
                  size="icon"
                  className="h-6 w-6"
                  onClick={refreshSummaries}
                  disabled={fetchingSummaries || channelIds.length === 0}
                  aria-label={tr('Refresh storage summary')}
                  title={tr('Refresh storage summary')}
                >
                  <RefreshCw className="h-3 w-3" />
                </Button>
              </span>
            </CardTitle>
          </CardHeader>
          <CardContent className="p-0">
            <ScrollArea className="h-[min(20rem,40dvh)] lg:h-[500px]">
              <div className="p-2 space-y-0.5">
                {channels.map((ch) => {
                  const summary = summariesByChannel.get(ch.cid);
                  const display = channelSummaryDisplay({
                    offlineUnchecked: false,
                    isFetching: fetchingSummaries,
                    isError: summaryIsError,
                    hasErrorData: summaryIsError,
                    observation,
                    currentChannelKey,
                    eligibleChannelKey,
                    omittedChannelIds: omittedIds,
                    channelId: ch.cid,
                    summary,
                    isQueryInvalidated: summaryInvalidated,
                  });
                  const statusText = channelSummaryLabelText(display);
                  const counted = isCountedSummary(summary);
                  const showPartial = isPartialSummary(summary);
                  return (
                  <button
                    key={ch.cid}
                    onClick={() => { setSelectedCid(ch.cid); setCurrentPath('/'); }}
                    className={cn(
                      'w-full rounded-md px-2.5 py-2 text-left text-sm transition-colors',
                      selectedCid === ch.cid
                        ? 'bg-primary/10 text-primary'
                        : 'text-foreground hover:bg-muted/50',
                    )}
                  >
                    <span className="block truncate">{ch.name}</span>
                    <span className="mt-1 flex min-h-3.5 items-center gap-2 font-mono-data text-[9px] text-muted-foreground/80">
                      {statusText && display.kind !== 'partial' && display.kind !== 'stale-cached' ? (
                        <span className={cn(
                          display.kind === 'unavailable' || display.kind === 'error' ? 'text-amber-400/80' : undefined,
                        )}>
                          {tr(statusText)}
                        </span>
                      ) : counted ? (
                        <>
                          <span className="flex items-center gap-0.5" title={tr("Files")}><File className="h-3 w-3" />{summary.fileCount}</span>
                          <span className="flex items-center gap-0.5" title={tr('Folders')}><Folder className="h-3 w-3" />{summary.folderCount}</span>
                          <span className="ml-auto flex items-center gap-0.5" title={tr('Total size')}><HardDrive className="h-3 w-3" />{formatBytes(summary.totalSize)}</span>
                          {showPartial && (
                            <span className="text-amber-400/80" title={tr('Scan stopped before the full tree was counted')}>{tr('Partial')}</span>
                          )}
                          {display.kind === 'stale-cached' && (
                            <span className="text-amber-400/80" title={tr('Summary may be outdated')}>{tr('Stale')}</span>
                          )}
                        </>
                      ) : statusText ? (
                        <span className={cn(
                          display.kind === 'unavailable' || display.kind === 'error' ? 'text-amber-400/80' : undefined,
                        )}>
                          {tr(statusText)}
                        </span>
                      ) : null}
                    </span>
                  </button>
                  );
                })}
                {channels.length === 0 && (
                  <p className="text-xs text-muted-foreground text-center py-4">{tr('No channels')}</p>
                )}
              </div>
            </ScrollArea>
          </CardContent>
        </Card>

        {/* File List */}
        <Card className="min-w-0 lg:col-span-9">
          <CardHeader className="pb-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div className="flex min-w-0 items-center gap-2 overflow-x-auto">
                {selectedCid && currentPath !== '/' && (
                  <Button variant="ghost" size="icon" className="h-7 w-7" onClick={goUp} aria-label={tr('Go to parent folder')} title={tr('Go to parent folder')}>
                    <ArrowLeft className="h-3.5 w-3.5" />
                  </Button>
                )}
                <div className="flex items-center gap-1 text-xs text-muted-foreground">
                  <HardDrive className="h-3.5 w-3.5" />
                  <button onClick={() => setCurrentPath('/')} className="hover:text-foreground transition-colors">/</button>
                  {pathParts.map((part, i) => (
                    <span key={i} className="flex items-center gap-1">
                      <span>/</span>
                      <button
                        onClick={() => setCurrentPath('/' + pathParts.slice(0, i + 1).join('/'))}
                        className="hover:text-foreground transition-colors"
                      >
                        {part}
                      </button>
                    </span>
                  ))}
                </div>
              </div>
              {selectedCid && (
                <Badge variant="secondary" className="text-[10px] font-mono-data">
                  {files.length} {tr('items')}
                </Badge>
              )}
            </div>
          </CardHeader>
          <CardContent className="p-0">
            {!selectedCid ? (
              <div className="flex h-[min(400px,50dvh)] min-h-64 items-center justify-center">
                <p className="text-sm text-muted-foreground"> {tr("Select a channel to browse files")}</p>
              </div>
            ) : loadingFiles ? (
              <div className="flex h-[min(400px,50dvh)] min-h-64 items-center justify-center">
                <PageLoader />
              </div>
            ) : filesError ? (
              <div className="flex h-[min(400px,50dvh)] min-h-64 flex-col items-center justify-center gap-3 px-4 sm:px-8">
                <AlertTriangle className="h-8 w-8 text-amber-400" />
                <p className="text-sm font-medium text-foreground"> {tr("File Browser Unavailable")}</p>
                <p className="text-xs text-muted-foreground text-center max-w-md" role="alert">
                  {tr(fileBrowseErrorMessage(filesError))}
                </p>
                <Button asChild size="sm" variant="outline"><Link to="/settings?tab=connections">{tr('Configure Query connection')}</Link></Button>
                {(filesError as any)?.response?.data?.code != null && (
                  <p className="text-[10px] text-muted-foreground/60 mt-1">{tr('TeamSpeak error code:')} {(filesError as any).response.data.code}</p>
                )}
                <Button
                  size="sm"
                  variant="outline"
                  disabled={fetchingFiles}
                  onClick={() => { void refetchFiles(); }}
                  aria-busy={fetchingFiles}
                >
                  {fetchingFiles ? tr("Retrying…") : tr("Retry")}
                </Button>
              </div>
            ) : (
              <ScrollArea className="h-[min(460px,55dvh)] min-h-72">
                <div className="min-w-0">
                {/* File table header */}
                <div className="grid grid-cols-12 gap-2 px-4 py-2 text-[10px] text-muted-foreground uppercase tracking-wider border-b border-border">
                  <div className="col-span-6 sm:col-span-5"> {tr("Name")}</div>
                  <div className="col-span-3 sm:col-span-2 text-right">{tr('Size')}</div>
                  <div className="hidden sm:block col-span-3">{tr('Modified')}</div>
                  <div className="col-span-3 sm:col-span-2"></div>
                </div>

                {files.length === 0 ? (
                  <div className="flex items-center justify-center h-[350px]">
                    <EmptyState icon={FolderOpen} title={tr('Empty directory')} description={tr('No files in this directory.')} />
                  </div>
                ) : (
                  <div className="divide-y divide-border/50">
                    {files.map((file) => (
                      <div
                        key={file.name}
                        className={cn(
                          'grid grid-cols-12 gap-2 px-4 py-2 text-sm items-center group hover:bg-muted/20 transition-colors',
                          file.type === 1 && 'cursor-pointer',
                        )}
                        onClick={() => navigateTo(file)}
                      >
                        <div className="col-span-6 sm:col-span-5 flex items-center gap-2 truncate">
                          {file.type === 1 ? (
                            <Folder className="h-4 w-4 text-primary/70 shrink-0" />
                          ) : (
                            <File className="h-4 w-4 text-muted-foreground shrink-0" />
                          )}
                          {file.type === 1 ? <button className="truncate text-left" onClick={e => { e.stopPropagation(); navigateTo(file); }}>{file.name}</button> : <span className="truncate">{file.name}</span>}
                        </div>
                        <div className="col-span-3 sm:col-span-2 text-right text-xs text-muted-foreground font-mono-data">
                          {file.type === 0 ? formatBytes(file.size) : '-'}
                        </div>
                        <div className="hidden sm:block col-span-3 text-xs text-muted-foreground font-mono-data">
                          {formatDate(file.datetime)}
                        </div>
                        <div className="col-span-3 sm:col-span-2 flex justify-end">
                          {canManageFiles && file.type === 0 && <button onClick={e => { e.stopPropagation(); downloadMutation.mutate(targetFor(file.name, buildFilePath(currentPath, file.name))); }} disabled={downloadMutation.isPending} aria-label={`${tr('Download')} ${file.name}`} title={`${tr('Download')} ${file.name}`} className="flex h-8 w-8 shrink-0 items-center justify-center rounded hover:bg-muted"><Download className="h-3.5 w-3.5" /></button>}
                          {canManageFiles && <button
                            onClick={(e) => { e.stopPropagation(); openDelete(file); }}
                            className="flex h-8 w-8 shrink-0 items-center justify-center rounded text-muted-foreground transition-all hover:bg-destructive/10 hover:text-destructive"
                            aria-label={`${tr('Delete')} ${file.name}`}
                            title={`${tr('Delete')} ${file.name}`}
                          >
                            <Trash2 className="h-3.5 w-3.5" />
                          </button>}
                        </div>
                      </div>
                    ))}
                  </div>
                )}
                </div>
              </ScrollArea>
            )}
          </CardContent>
        </Card>
      </div>

      {/* Info notice */}
      <p className="text-xs text-muted-foreground text-center">{tr('Files are transferred through the configured TeamSpeak Query connection and file-transfer port. Maximum file size: 25 MB. Existing files are never overwritten.')}</p>

      {/* Create Directory Dialog */}
      <Dialog open={showMkdir} onOpenChange={setShowMkdir}>
        <DialogContent>
          <DialogHeader><DialogTitle> {tr("Create Directory")}</DialogTitle></DialogHeader>
          <div className="space-y-2">
            <div>
              <Label className="text-xs"> {tr("Directory Name")}</Label>
              <Input value={newDirName} onChange={(e) => setNewDirName(e.target.value)} placeholder={tr("New Folder")} autoFocus />
            </div>
            <p role="note" className="text-xs text-muted-foreground" data-testid="file-write-read-not-authorize">
              {tr(FILE_WRITE_READ_DOES_NOT_AUTHORIZE)}
            </p>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setShowMkdir(false)}> {tr("Cancel")}</Button>
            <Button onClick={handleMkdir} disabled={mkdirMutation.isPending || !newDirName.trim()}> {tr("Create")}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete Confirm — bound to deleteTarget ownership, not live store */}
      <ConfirmDialog
        open={!!deleteTarget}
        onOpenChange={() => setDeleteTarget(null)}
        title={tr('Delete File')}
        description={
          deleteTarget
            ? `${tr('Permanently delete:')} ${deleteTarget.entryName ?? deleteTarget.fullPath} (${deleteTarget.fullPath}). ${tr('This cannot be undone.')}`
            : ''
        }
        confirmLabel={tr("Delete")}
        destructive
        onConfirm={handleDelete}
        loading={deleteLoading}
      />
    </div>
  );
}
