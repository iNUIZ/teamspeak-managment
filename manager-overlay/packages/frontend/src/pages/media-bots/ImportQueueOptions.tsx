import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { MusicBotSummary } from '@ts6/common';


export function ImportQueueOptions({
  bots,
  configId,
  botId,
  onBotIdChange,
  clearFirst,
  onClearFirstChange,
}: {
  bots: MusicBotSummary[];
  configId: number | null;
  botId: string;
  onBotIdChange: (id: string) => void;
  clearFirst: boolean;
  onClearFirstChange: (v: boolean) => void;
}) {
  useUiLanguageStore(state => state.language);
  const running = bots.filter(
    (b) =>
      b.serverConfigId === configId &&
      b.status !== 'stopped' &&
      b.status !== 'error',
  );
  if (!configId || running.length === 0) {
    return (
      <p className="text-[10px] text-muted-foreground">
        {uiText("Start a media bot on this server to import directly to its queue.")}</p>
    );
  }
  return (
    <div className="flex flex-wrap items-center gap-2">
      <Select value={botId || 'none'} onValueChange={(v) => onBotIdChange(v === 'none' ? '' : v)}>
        <SelectTrigger className="h-8 w-44 text-xs">
          <SelectValue placeholder={uiText("Enqueue to bot...")} />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="none">{uiText("No bot queue")}</SelectItem>
          {running.map((b) => (
            <SelectItem key={b.id} value={String(b.id)}>{b.name}</SelectItem>
          ))}
        </SelectContent>
      </Select>
      {botId && (
        <label className="flex items-center gap-1.5 text-[10px] text-muted-foreground cursor-pointer">
          <input
            type="checkbox"
            checked={clearFirst}
            onChange={(e) => onClearFirstChange(e.target.checked)}
            className="accent-primary"
          />
          {uiText("Clear queue first")}</label>
      )}
    </div>
  );
}

