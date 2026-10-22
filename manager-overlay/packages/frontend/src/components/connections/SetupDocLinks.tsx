import { ExternalLink } from 'lucide-react';
import { translate as uiText, useLanguageStore } from '@/lib/i18n';
import type { SetupDocLink } from '@/content/teamspeak-docs';

interface SetupDocLinksProps {
  docs: readonly SetupDocLink[];
  className?: string;
}

export function SetupDocLinks({ docs, className }: SetupDocLinksProps) {
  useLanguageStore(state => state.language);
  if (docs.length === 0) return null;

  return (
    <div className={className ?? 'flex flex-wrap gap-x-3 gap-y-1'}>
      {docs.map((doc) => (
        <a
          key={`${doc.url}-${doc.label}`}
          href={doc.url}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-1 text-[11px] text-primary hover:underline"
        >
          <ExternalLink className="h-3 w-3 shrink-0" />
          {uiText(doc.label)}
        </a>
      ))}
    </div>
  );
}
