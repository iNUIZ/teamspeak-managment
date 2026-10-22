import { translate, useLanguageStore } from '@/lib/i18n';
import { runtimeIssueSummary } from '@/lib/runtime-issue';

/** Keep the actionable localized summary visible and the original evidence available on demand. */
export function RuntimeIssue({message,fallback,className,role}:{message:string;fallback:string;className?:string;role?:'alert'|'status'}) {
  useLanguageStore(state=>state.language);
  const summary=runtimeIssueSummary(message,fallback);
  return <div role={role} className={className}>
    <p>{summary}</p>
    {summary!==message&&<details className="mt-1 min-w-0 text-xs">
      <summary className="cursor-pointer">{translate('Technical output (original language)')}</summary>
      <pre className="mt-1 max-w-full whitespace-pre-wrap break-words [overflow-wrap:anywhere]">{message}</pre>
    </details>}
  </div>;
}
