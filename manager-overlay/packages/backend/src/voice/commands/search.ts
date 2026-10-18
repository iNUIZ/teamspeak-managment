import type { VoiceBot } from '../voice-bot.js';
import type { YouTubeSearchResult } from '../audio/youtube.js';
import { searchYouTube, searchYouTubeMusic } from '../audio/youtube.js';
import type { CommandContext } from './context.js';

const SEARCH_SESSION_MS = 5 * 60 * 1000;
const MAX_QUERY_LENGTH = 160;

type SearchSource = 'YouTube Music' | 'YouTube' | 'Spotify match';
type RankedResult = YouTubeSearchResult & { sourceLabel: SearchSource; score: number };
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
  if (sourceLabel === 'YouTube Music') score += 18;
  if (sourceLabel === 'Spotify match') score += 12;
  if (/official|topic|vevo|audio|provided to youtube/i.test(`${item.artist} ${item.title}`)) score += 12;
  if (item.duration >= 75 && item.duration <= 480) score += 8;
  if (/live|reaction|cover|nightcore|slowed|reverb|karaoke/i.test(item.title)) score -= 8;
  return score;
}

function mergeAndRank(
  query: string,
  groups: Array<{ sourceLabel: SearchSource; items: YouTubeSearchResult[] }>,
): RankedResult[] {
  const unique = new Map<string, RankedResult>();
  for (const group of groups) {
    for (const item of group.items) {
      if (!item.id || unique.has(item.id)) continue;
      unique.set(item.id, { ...item, sourceLabel: group.sourceLabel, score: qualityScore(item, query, group.sourceLabel) });
    }
  }
  return [...unique.values()].sort((a, b) => b.score - a.score || a.title.localeCompare(b.title)).slice(0, 10);
}

function formatResults(query: string, results: RankedResult[]): string {
  const lines = results.map((item, index) => `${index + 1}. [${item.sourceLabel}] ${item.artist} - ${item.title}`);
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
  await context.reply(bot, userClid, 'YouTube Music, YouTube ve Spotify eşleşmeleri kaliteye göre aranıyor...');
  const signal = AbortSignal.timeout(180_000);
  const settled = await Promise.allSettled([
    searchYouTubeMusic(query, 8, signal),
    searchYouTube(query, 8, signal),
    // Credential-free fallback for Spotify catalogue wording. A future Spotify
    // provider extension can replace this group without changing chat routing.
    searchYouTube(`${query} spotify official audio`, 8, signal),
  ]);
  const items = (index: number): YouTubeSearchResult[] => settled[index]?.status === 'fulfilled'
    ? settled[index].value
    : [];
  const results = mergeAndRank(query, [
    { sourceLabel: 'YouTube Music', items: items(0) },
    { sourceLabel: 'YouTube', items: items(1) },
    { sourceLabel: 'Spotify match', items: items(2) },
  ]);
  if (!results.length) {
    await context.reply(bot, userClid, `“${query}” için sonuç bulunamadı.`);
    return;
  }
  sessions.set(sessionKey(botId, userClid), { query, results, expiresAt: Date.now() + SEARCH_SESSION_MS });
  await context.reply(bot, userClid, formatResults(query, results));
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
  await context.reply(bot, userClid, `Seçildi: ${selected.artist} - ${selected.title}`);
  await context.enqueueMediaUrl(botId, bot, userClid, `https://www.youtube.com/watch?v=${selected.id}`);
  return true;
}

export const searchInternals = { mergeAndRank, formatResults };
