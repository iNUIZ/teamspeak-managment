import type { VoiceBot } from '../voice-bot.js';
import type { YouTubeSearchResult } from '../audio/youtube.js';
import { getYtCookieFile, searchYouTube } from '../audio/youtube.js';
import { searchSpotifyTracks, spotifyCredentials, type SpotifyTrackResult } from '../audio/spotify-search.js';
import type { CommandContext } from './context.js';

const SEARCH_SESSION_MS = 5 * 60 * 1000;
const MAX_QUERY_LENGTH = 160;

/** 'Spotify' is the real catalogue (needs credentials); everything else is a YouTube video. */
type SearchSource = 'YouTube' | 'Spotify';
/** `url` is what gets queued: a YouTube watch page, or a Spotify track matched on YouTube at play time. */
type Candidate = YouTubeSearchResult & { url?: string };
export type RankedResult = Candidate & { sourceLabel: SearchSource; score: number; url: string };
type SearchSession = { query: string; results: RankedResult[]; expiresAt: number };

const sessions = new Map<string, SearchSession>();

function sessionKey(botId: number, userClid: number): string {
  return `${botId}:${userClid}`;
}

function normalise(value: string): string {
  return value.toLocaleLowerCase('tr-TR').normalize('NFKD').replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9\s]/g, ' ').replace(/\s+/g, ' ').trim();
}

/** Relevance and likely audio quality; source is deliberately not used as a hard partition. */
export function qualityScore(item: YouTubeSearchResult, query: string, sourceLabel: SearchSource): number {
  const haystack = normalise(`${item.artist} ${item.title}`);
  const terms = normalise(query).split(' ').filter(Boolean);
  const matchedTerms = terms.filter((term) => haystack.includes(term)).length;
  let score = terms.length ? (matchedTerms / terms.length) * 60 : 0;
  if (sourceLabel === 'Spotify') score += 12;
  if (/official|topic|vevo|audio|provided to youtube/i.test(`${item.artist} ${item.title}`)) score += 12;
  if (item.duration >= 75 && item.duration <= 480) score += 8;
  if (/live|reaction|cover|nightcore|slowed|reverb|karaoke/i.test(item.title)) score -= 8;
  return score;
}

function mergeAndRank(
  query: string,
  groups: Array<{ sourceLabel: SearchSource; items: Candidate[] }>,
): RankedResult[] {
  const unique = new Map<string, RankedResult>();
  for (const group of groups) {
    for (const raw of group.items) {
      // Channels and playlists show up in YouTube search too; only single videos play.
      if (!raw.id || unique.has(raw.id)) continue;
      if (group.sourceLabel === 'YouTube' && !/^[A-Za-z0-9_-]{11}$/.test(raw.id)) continue;
      const item = { ...raw, artist: cleanArtist(raw.artist) };
      const url = item.url ?? `https://www.youtube.com/watch?v=${item.id}`;
      unique.set(item.id, { ...item, url, sourceLabel: group.sourceLabel, score: qualityScore(item, query, group.sourceLabel) });
    }
  }
  return [...unique.values()].sort((a, b) => b.score - a.score || a.title.localeCompare(b.title)).slice(0, 10);
}

/** "Tarkan - Topic" and "TarkanVEVO" are YouTube channel names, not artists. */
function cleanArtist(artist: string): string {
  const cleaned = artist.replace(/\s+-\s+topic$/i, '').replace(/vevo$/i, '').trim();
  return cleaned || artist;
}

/** Spotify ids cannot collide with YouTube ids in the merged list. */
function spotifyCandidate(track: SpotifyTrackResult): Candidate {
  return { id: `spotify:${track.id}`, title: track.title, artist: track.artist, duration: track.duration, thumbnail: '', url: track.url };
}

function formatResults(query: string, results: RankedResult[]): string {
  const lines = results.map((item, index) => {
    const source = item.sourceLabel;
    const duration = Number.isFinite(item.duration) && item.duration > 0
      ? ` (${Math.floor(item.duration / 60)}:${String(Math.floor(item.duration % 60)).padStart(2, '0')})` : '';
    return `${index + 1}. [${source}] ${item.artist} - ${item.title}${duration}`;
  });
  return `🔎 ${query}\n${lines.join('\n')}\n\nÇalmak için 1-10 arasında yalnızca numarayı yaz.`;
}

export async function handleSearch(
  context: CommandContext,
  botId: number,
  bot: VoiceBot,
  userClid: number,
  rawQuery: string,
): Promise<void> {
  const query = rawQuery.trim().slice(0, MAX_QUERY_LENGTH);
  if (query.length < 2) {
    await context.reply(bot, userClid, 'Kullanım: !ara <şarkı veya sanatçı>');
    return;
  }
  if (!getYtCookieFile()) {
    sessions.delete(sessionKey(botId, userClid));
    await context.reply(bot, userClid, 'YouTube çerezleri gerekli. Yönetici, panelde Ayarlar → YouTube bölümünden cookies.txt yüklemeli.');
    return;
  }
  await context.reply(bot, userClid, spotifyCredentials() !== null
    ? `YouTube ve Spotify'da aranıyor: ${query}`
    : `YouTube'da aranıyor: ${query}`);
  const results = await searchMusic(query, AbortSignal.timeout(180_000));
  if (!results.length) {
    await context.reply(bot, userClid, `“${query}” için sonuç bulunamadı.`);
    return;
  }
  sessions.set(sessionKey(botId, userClid), { query, results, expiresAt: Date.now() + SEARCH_SESSION_MS });
  await context.reply(bot, userClid, formatResults(query, results));
}

/** The ranked mixed search behind `!ara` and the panel search box. Each result carries the URL to queue. */
export async function searchMusic(rawQuery: string, signal?: AbortSignal): Promise<RankedResult[]> {
  const query = rawQuery.trim().slice(0, MAX_QUERY_LENGTH);
  const spotify = spotifyCredentials() !== null;
  // Plain YouTube search, plus the official-audio upload that the plain search often buries.
  const settled = await Promise.allSettled<Candidate[]>([
    searchYouTube(query, 10, signal),
    searchYouTube(`${query} official audio`, 5, signal),
    spotify
      ? searchSpotifyTracks(query, 5, signal).then((tracks) => tracks.map(spotifyCandidate))
      : Promise.resolve([]),
  ]);
  const items = (index: number): Candidate[] => settled[index]?.status === 'fulfilled'
    ? settled[index].value
    : [];
  if (spotify && settled[2]?.status === 'rejected') {
    console.warn(`[MusicCmd] Spotify search failed: ${settled[2].reason instanceof Error ? settled[2].reason.message : settled[2].reason}`);
  }
  return mergeAndRank(query, [
    { sourceLabel: 'YouTube', items: [...items(0), ...items(1)] },
    { sourceLabel: 'Spotify', items: items(2) },
  ]);
}

/** Returns true only when a live search session consumed the number. */
export async function handleSearchChoice(
  context: CommandContext,
  botId: number,
  bot: VoiceBot,
  userClid: number,
  choice: number,
): Promise<boolean> {
  const key = sessionKey(botId, userClid);
  const session = sessions.get(key);
  if (!session) return false;
  if (session.expiresAt <= Date.now()) {
    sessions.delete(key);
    await context.reply(bot, userClid, 'Arama süresi doldu. Tekrar !ara yazın.');
    return true;
  }
  const selected = session.results[choice - 1];
  if (!selected) {
    await context.reply(bot, userClid, 'Geçersiz seçim. Listede görünen bir numarayı yazın.');
    return true;
  }
  sessions.delete(key);
  await context.reply(bot, userClid, `Hazırlanıyor: ${selected.artist} - ${selected.title}`);
  try {
    await context.enqueueMediaUrl(botId, bot, userClid, selected.url);
  } catch (error) {
    // Do not leak downloader paths, proxy credentials or cookie locations to chat.
    const blocked = /sign in to confirm|not a bot|bot.check/i.test(error instanceof Error ? error.message : '');
    await context.reply(bot, userClid, blocked
      ? 'Çalınamadı: YouTube sunucu bağlantısında doğrulama istiyor. Arama sonucu bulunması, sesin indirilebildiği anlamına gelmez. Yönetici Medya tanılama bölümünü kontrol etmeli.'
      : 'Şarkı hazırlanamadı. Yönetici Medya tanılama bölümünü ve bot kayıtlarını kontrol etmeli. Tekrar !ara ile deneyebilirsiniz.');
  }
  return true;
}

export const searchInternals = { mergeAndRank, formatResults };
