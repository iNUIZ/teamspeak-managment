import { translate, translateUiError, useLanguageStore } from './i18n';

/** Localize the presentation, not the diagnostic evidence or the bot's state. */
export function runtimeIssueSummary(message:string,fallback:string):string {
  if(useLanguageStore.getState().language==='en')return message;
  const ts=message.match(/^TS3 error (\d+):\s*(.*)$/i);
  if(ts){
    if(ts[1]==='2568')return translate('TeamSpeak denied a bot action because its identity lacks permission. This is separate from YouTube cookies.');
    const description=translateUiError(ts[2],'The last bot operation was rejected.');
    return translate('TeamSpeak error {0}: {1}',ts[1],description);
  }
  return translateUiError(message,fallback);
}
