import type { CommandContext } from './context.js';
import type { VoiceBot } from '../voice-bot.js';
import { loadTtsSettings, synthesizeSpeech } from '../audio/tts.js';

export async function handleSay(context: CommandContext, botId: number, bot: VoiceBot, userClid: number, args: string): Promise<void> {
  // A public chat command never replaces active media without a confirmed stop.
  if (bot.status !== 'connected' || bot.mediaSession) throw new Error('Stop current media before using !say.');
  const settings = await loadTtsSettings(context.prisma, botId);
  const audio = await synthesizeSpeech(botId, args, settings);
  try {
    if (context.voiceBotManager.getBot(botId) !== bot || bot.status !== 'connected' || bot.mediaSession) { await audio.cleanup(); return; }
    await context.joinChannelForCommand(botId, bot, userClid);
    const id = `tts_${botId}_${Date.now()}`;
    await bot.play({ id, title: 'Text to speech', filePath: audio.file, source: 'local' }, context.chatMusicSwitch(bot));
    bot.once('trackEnd', () => { void audio.cleanup(); });
  } catch (error) { await audio.cleanup(); throw error; }
}
