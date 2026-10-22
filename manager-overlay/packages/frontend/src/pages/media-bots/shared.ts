import axios from 'axios';
import { translate as tr, translateUiError, useLanguageStore } from '@/lib/i18n';
import type { YouTubeSearchResult } from '@ts6/common';


// ─── Helper ──────────────────────────────────────────────────────────────────

export function formatTime(seconds: number | null | undefined): string {
  if (!seconds || seconds <= 0) return '0:00';
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  return `${m}:${s.toString().padStart(2, '0')}`;
}


export interface UrlLoadInfo {
  type: 'video' | 'playlist';
  items: YouTubeSearchResult[];
  title?: string;
  sourceTrackCount?: number;
  matchedCount?: number;
  cappedAt?: number;
}


export function youtubeInfoErrorMessage(err: unknown): string {
  if (axios.isAxiosError(err) && err.code === 'ECONNABORTED') {
    return tr('Matching tracks is taking longer than expected — try again in a minute');
  }
  if (axios.isAxiosError(err) && err.response?.data?.error) {
    return translateUiError(String(err.response.data.error), 'Failed to load URL info');
  }
  return tr('Failed to load URL info');
}


export function urlInfoPlaylistLabel(info: UrlLoadInfo): string {
  if (info.type !== 'playlist') return tr('Single track');
  if (info.sourceTrackCount != null && info.matchedCount != null) {
    const cap = info.cappedAt ?? info.items.length;
    return tr('{0} matched of {1} (first {2} searched)', info.matchedCount, info.sourceTrackCount, cap);
  }
  return trackCount(info.items.length);
}


function trackCount(n: number): string {
  return tr(n === 1 ? '{0} track' : '{0} tracks', n);
}


/** The register endpoint accepts at most this many URLs per request. */
export const REGISTER_BATCH_SIZE = 200;


/** The playlist add endpoint accepts at most this many song IDs per request. */
export const ADD_SONGS_BATCH_SIZE = 1000;


export function chunk<T>(items: T[], size: number): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += size) out.push(items.slice(i, i + size));
  return out;
}


/** Toast text after adding URL tracks to a playlist. Stream tracks are saved as links and fetched when played. */
export function playlistAddMessage({
  added,
  alreadyInPlaylist,
  playlistName,
  stream,
}: {
  added: number;
  alreadyInPlaylist: number;
  playlistName: string;
  stream: boolean;
}): string {
  if (useLanguageStore.getState().language === 'tr') {
    if (!added) return alreadyInPlaylist ? tr('Selected tracks are already in {0}', playlistName) : tr('Nothing new to add');
    return tr('Added {0} to {1}', trackCount(added), playlistName)
      + (alreadyInPlaylist ? tr(' ({0} already there)', alreadyInPlaylist) : '')
      + (stream ? tr('. Nothing downloads until played.') : '');
  }
  if (added === 0) {
    return alreadyInPlaylist > 0
      ? `${alreadyInPlaylist === 1 ? 'That track is' : 'All selected tracks are'} already in ${playlistName}`
      : 'Nothing new to add';
  }
  const already = alreadyInPlaylist > 0 ? ` (${alreadyInPlaylist} already there)` : '';
  const tail = stream ? '. Nothing downloads until played.' : '';
  return `Added ${trackCount(added)} to ${playlistName}${already}${tail}`;
}


export function urlItemSelectKey(index: number): string {
  return String(index);
}


export function allUrlItemKeys(count: number): Set<string> {
  const keys = new Set<string>();
  for (let i = 0; i < count; i++) keys.add(urlItemSelectKey(i));
  return keys;
}


export function selectedUrlItems(items: YouTubeSearchResult[], selectedKeys: Set<string>): YouTubeSearchResult[] {
  return items.filter((_, i) => selectedKeys.has(urlItemSelectKey(i)));
}


export interface ImportJobStatus {
  phase?: 'matching' | 'importing';
  processed?: number;
  total?: number;
  matchProcessed?: number;
  matchTotal?: number;
  matched?: number;
  sourceTrackCount?: number;
  importCap?: number;
  enqueued?: number;
  musicBotId?: number;
  downloaded?: number;
  registered?: number;
  skipped?: number;
  errors?: string[];
}


export function importJobProgressLabel(job: ImportJobStatus | undefined | null): string {
  if (!job) return '?';
  if (useLanguageStore.getState().language === 'tr') return job.phase === 'matching'
    ? tr('Matching {0}/{1} ({2} hits)', job.matchProcessed ?? 0, job.matchTotal ?? '?', job.matched ?? 0)
    : tr('Importing {0}/{1}', job.processed ?? 0, job.total ?? '?');
  if (job.phase === 'matching') {
    const ofSource =
      job.sourceTrackCount != null &&
      job.sourceTrackCount > (job.matchTotal ?? 0)
        ? ` of ${job.sourceTrackCount}`
        : '';
    return `Matching ${job.matchProcessed ?? 0}/${job.matchTotal ?? '?'}${ofSource} (${job.matched ?? 0} hits)`;
  }
  return `Importing ${job.processed ?? 0}/${job.total ?? '?'}`;
}


export function importJobCompleteMessage(job: ImportJobStatus): string {
  if (useLanguageStore.getState().language === 'tr') {
    const parts: string[]=[];
    if(job.registered) parts.push(tr('{0} registered',job.registered));
    if(job.enqueued) parts.push(tr('{0} enqueued',job.enqueued));
    if(job.downloaded) parts.push(tr('{0} downloaded',job.downloaded));
    if(job.skipped) parts.push(tr('{0} skipped',job.skipped));
    const base=parts.length ? tr('Import complete: {0}',parts.join(', ')) : tr('Import complete');
    return job.sourceTrackCount != null && job.sourceTrackCount > (job.total ?? 0)
      ? base+tr('. Only first {0} of {1} source tracks — raise Max playlist import in Settings → Limits',job.importCap ?? job.total ?? 0,job.sourceTrackCount) : base;
  }
  const parts: string[] = [];
  if ((job.registered ?? 0) > 0) parts.push(`${job.registered} registered`);
  if ((job.enqueued ?? 0) > 0) parts.push(`${job.enqueued} enqueued`);
  if ((job.downloaded ?? 0) > 0) parts.push(`${job.downloaded} downloaded`);
  if ((job.skipped ?? 0) > 0) parts.push(`${job.skipped} skipped`);
  const base = parts.length ? `Import complete: ${parts.join(', ')}` : 'Import complete';
  if (
    job.sourceTrackCount != null &&
    job.sourceTrackCount > (job.total ?? 0)
  ) {
    const cap = job.importCap ?? job.total ?? 0;
    return `${base}. Only first ${cap} of ${job.sourceTrackCount} source tracks — raise Max playlist import in Settings → Limits`;
  }
  return base;
}


export function importCapHint(maxPlaylistImport: number | undefined): string {
  const cap = maxPlaylistImport ?? 250;
  return tr('Imports up to {0} tracks (Settings → Limits). Large Apple Music playlists may need a higher cap (max 500).', cap);
}

