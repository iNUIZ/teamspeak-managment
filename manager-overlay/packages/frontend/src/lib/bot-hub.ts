/**
 * Bot hub wording: what a bot is doing right now, in one line.
 */

import type { BotMediaOverview } from '@ts6/common';
import { translate as tr } from './i18n';
import {
  SOURCE_MODE_LABELS, formatClock, lastStopLabel, qualityLabel, encoderLabel, formatTimeout, healthLabel,
} from './video-streaming';

export type HubTone = 'live' | 'music' | 'idle' | 'offline';

export function hubTone(bot: BotMediaOverview): HubTone {
  if (bot.status === 'stopped' || bot.status === 'error') return 'offline';
  if (bot.session?.kind === 'video') return 'live';
  if (bot.session?.kind === 'music') return 'music';
  return 'idle';
}

/** "Neon Skyline — Aurora", "Streaming from iptv.example", "Idle", "Offline". */
export function hubHeadline(bot: BotMediaOverview): string {
  const tone = hubTone(bot);
  if (tone === 'offline') return tr(bot.status === 'error' ? 'Error' : 'Offline');
  if (tone === 'live') {
    const label = bot.session?.label;
    return bot.session?.state === 'starting'
      ? (label ? tr('Starting stream from {0}…', label) : tr('Starting stream…'))
      : (label ? tr('Streaming from {0}', label) : tr('Streaming'));
  }
  if (tone === 'music' && bot.music) {
    const title = bot.music.title ?? tr('Unknown track');
    return bot.music.artist ? `${title} — ${bot.music.artist}` : title;
  }
  return tr('Idle');
}

/** Compact facts for the card: quality/encoder/viewers for video, progress for music. */
export function hubFacts(bot: BotMediaOverview, now: number): string[] {
  const facts: string[] = [];
  if (bot.video) {
    facts.push(qualityLabel(bot.video.quality, bot.video.preset));
    facts.push(encoderLabel(bot.video.encoder));
    if (bot.video.sourceMode) facts.push(tr(SOURCE_MODE_LABELS[bot.video.sourceMode]));
    if (bot.video.health?.speed != null) facts.push(healthLabel(bot.video.health));
    facts.push(tr(bot.video.viewerCount === 1 ? '{0} viewer' : '{0} viewers', bot.video.viewerCount));
    if (bot.video.startedAt) facts.push(tr('up {0}', formatClock(now - bot.video.startedAt)));
    if (bot.video.noViewer.stopAt) {
      facts.push(tr('auto-stop in {0}', formatClock(bot.video.noViewer.stopAt - now)));
    } else if (bot.video.noViewer.timeoutSec > 0) {
      facts.push(tr('stops after {0} idle', formatTimeout(bot.video.noViewer.timeoutSec)));
    }
  } else if (bot.music) {
    if (bot.music.live) {
      facts.push(tr('Live radio'));
      if (bot.music.position != null) facts.push(tr('up {0}', formatClock(bot.music.position * 1000)));
    } else if (bot.music.position != null) {
      facts.push(bot.music.duration
        ? `${formatClock(bot.music.position * 1000)} / ${formatClock(bot.music.duration * 1000)}`
        : formatClock(bot.music.position * 1000));
    }
    if (bot.status === 'paused') facts.push(tr('Paused'));
  }
  return facts;
}

/** Labelled video facts for the console's Now playing card. */
export function videoDetails(bot: BotMediaOverview, now: number): { label: string; value: string }[] {
  const v = bot.video;
  if (!v) return [];
  const details = [
    { label: 'Quality', value: qualityLabel(v.quality, v.preset) },
    { label: 'Encoder', value: encoderLabel(v.encoder) },
  ];
  if (v.sourceMode) details.push({ label: 'Source', value: tr(SOURCE_MODE_LABELS[v.sourceMode]) });
  if (v.health?.speed != null) details.push({ label: 'Encode health', value: healthLabel(v.health) });
  details.push({ label: 'Viewers', value: tr('{0} in channel', v.viewerCount) });
  if (v.startedAt) details.push({ label: 'Up for', value: formatClock(now - v.startedAt) });
  if (v.noViewer.stopAt) {
    details.push({ label: 'Auto-stop', value: tr('in {0}', formatClock(v.noViewer.stopAt - now)) });
  } else if (v.noViewer.timeoutSec > 0) {
    details.push({ label: 'Auto-stop', value: tr('if no viewers for {0}', formatTimeout(v.noViewer.timeoutSec)) });
  }
  return details;
}

/** "Track 2 of 8" from the bot's queue, or null when nothing is queued. */
export function queuePosition(state: { queue: unknown[]; currentIndex: number } | null | undefined): string | null {
  if (!state || state.queue.length === 0 || state.currentIndex < 0 || state.currentIndex >= state.queue.length) return null;
  return tr('Track {0} of {1}', state.currentIndex + 1, state.queue.length);
}

/** The newest stop across music and video, for idle cards. */
export function hubLastStop(bot: BotMediaOverview, now: number): string | null {
  const stops = [
    bot.lastVideoStop ? { ...bot.lastVideoStop, kind: 'Stream' } : null,
    bot.lastMusicStop ? { ...bot.lastMusicStop, kind: 'Music' } : null,
  ].filter((s): s is NonNullable<typeof s> => s != null).sort((a, b) => b.at - a.at);
  const latest = stops[0];
  if (!latest) return null;
  return tr('Last {0}: {1}', tr(latest.kind.toLowerCase()), lastStopLabel(latest, now));
}

/** Bots with music or a stream on right now, playing ones before paused ones. */
export function activeBots(bots: BotMediaOverview[]): BotMediaOverview[] {
  return bots
    .filter((b) => b.session && hubTone(b) !== 'offline')
    .sort((a, b) => Number(a.status === 'paused') - Number(b.status === 'paused'));
}

/** The header pill: "Neon Skyline — Aurora", "2 active bots", or null when nothing is on. */
export function activeBotsLabel(active: BotMediaOverview[]): string | null {
  if (active.length === 0) return null;
  if (active.length > 1) return tr('{0} active bots', active.length);
  return hubHeadline(active[0]);
}

export interface AutomationBadge {
  label: string;
  /** Longer explanation for the tooltip and screen readers. */
  title: string;
}

/** Badges for a bot's Auto-DJ and 24/7 settings; empty when both are off. */
export function automationBadges(bot: BotMediaOverview): AutomationBadge[] {
  const automation = bot.automation;
  if (!automation) return [];
  const badges: AutomationBadge[] = [];
  if (automation.autoDj) {
    const source = tr(automation.sourceKind === 'radio' ? 'a radio station' : 'a saved playlist');
    badges.push(automation.playing
      ? { label: tr('Auto-DJ playing'), title: tr('Auto-DJ is playing {0}. A request replaces it at once.', source) }
      : { label: tr('Auto-DJ'), title: tr('Auto-DJ is on: it plays {0} when nothing is queued.', source) });
  }
  if (automation.stayInChannel) badges.push({ label: tr('24/7'), title: tr('24/7 is on: the bot stays in its channel.') });
  return badges;
}
