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

/** Longer than this is a film, a mix or a whole album, not a song. */
const MAX_SONG_SECONDS = 20 * 60;

/** Upload noise that says nothing about the song: "(Official Video)", "[HD]", "#HERMANO", "| 8 Track". */
const TITLE_NOISE = /\s*[([](?:[^)\]]*\b(?:official|video|audio|lyrics?|s[oö]zleri|klip|clip|hd|hq|4k|visuali[sz]er|music)\b[^)\]]*)[)\]]/gi;

/**
 * A YouTube title such as "Heijan & Muti - BİRADER (Official Video) #HERMANO" carries the real artist
 * and song; the uploader ("AVA Media", "Lyrics Genius") often is only a channel.
 */
export function cleanTrack(rawTitle: string, rawArtist: string): { title: string; artist: string } {
  let title = rawTitle
    .replace(TITLE_NOISE, '')
    .replace(/\s+#\S+/g, '')
    .replace(/\s+\|.*$/, '')
    .replace(/\s{2,}/g, ' ')
    .trim();
  let artist = cleanArtist(rawArtist);
  const dash = title.match(/^(.+?)\s+[-–—]\s+(.+)$/);
  const credit = title.match(/^(.+?)\s*\(([^()]+)\)$/);
  if (dash) {
    artist = dash[1].trim();
    title = dash[2].trim();
  } else if (credit && normalise(rawArtist).includes(normalise(credit[2]))) {
    // "Birader (Osman Öztunç)" on "ZeMahşer Osman Öztunç TV": the name in brackets is the artist.
    title = credit[1].trim();
    artist = credit[2].trim();
  }
  return { title: title || rawTitle, artist: artist || rawArtist };
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
      if (raw.duration > MAX_SONG_SECONDS) continue;
      const score = qualityScore(raw, query, group.sourceLabel);
      const item = { ...raw, ...cleanTrack(raw.title, raw.artist) };
      const url = item.url ?? `https://www.youtube.com/watch?v=${item.id}`;
      unique.set(item.id, { ...item, url, sourceLabel: group.sourceLabel, score });
    }
  }
  // The official video and the plain upload of one song look the same once cleaned; keep the better one.
  const bySong = new Map<string, RankedResult>();
  for (const result of [...unique.values()].sort((a, b) => b.score - a.score || a.title.localeCompare(b.title))) {
    const key = normalise(`${result.artist} ${result.title}`);
    if (!bySong.has(key)) bySong.set(key, result);
  }
  return [...bySong.values()].slice(0, 10);
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

function markdownText(text: string, max = 48): string {
  const short = [...text].length > max ? `${[...text].slice(0, max - 1).join('')}…` : text;
  return short.replace(/([\\*_`~|[\]<>#])/g, '\\$1');
}

/**
 * TS6 renders Markdown in chat. One line per result, so the sender's split at line breaks
 * (a chat message is only ~480 bytes) never cuts a result in half.
 */
function formatResults(query: string, results: RankedResult[]): string {
  const lines = results.map((item, index) => {
    const duration = Number.isFinite(item.duration) && item.duration > 0
      ? ` · \`${Math.floor(item.duration / 60)}:${String(Math.floor(item.duration % 60)).padStart(2, '0')}\`` : '';
    const source = item.sourceLabel === 'Spotify' ? ' · Spotify' : '';
    return `**${index + 1}.** ${markdownText(item.title)} — *${markdownText(item.artist, 32)}*${duration}${source}`;
  });
  return [
    `### 🔎 ${markdownText(query, 60)}`,
    ...lines,
    '',
    `Çalmak için numarayı yaz: **1**–**${results.length}**`,
  ].join('\n');
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
    await context.reply(bot, userClid, 'YouTube çerezleri gerekli. Yönetici, panelde Kütüphane → YouTube erişimi bölümünden cookies.txt yüklemeli.');
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
