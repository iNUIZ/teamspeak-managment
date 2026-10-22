import { useSearchParams } from 'react-router-dom';
import { useTranslation } from '@/lib/i18n';
import ServerLogs from './ServerLogs';
import AuditHistory from './AuditHistory';
import ActivityJournal from './ActivityJournal';
import MediaDiagnostics from './MediaDiagnostics';
const views = [
  ['server', 'Server Logs', 'Native TeamSpeak logs: selected server or the entire server process.'],
  ['audit', 'Administrative Audit', 'Which Manager administrator did what, on which connection, and with what result.'],
  ['activity', 'Activity Journal', 'Optional join and leave history, recorded only after capture is enabled.'],
  ['media', 'Media diagnostics', 'Media tools and service readiness; this does not guarantee access to a media provider.'],
] as const;
export default function Reports() {
  const { t } = useTranslation(); const [params, setParams] = useSearchParams();
  const view = views.find(row => row[0] === params.get('view')) || views[0];
  return <div className="flex h-full min-h-0 flex-col gap-4" data-testid="reports-center">
    <h1 className="text-xl font-semibold">{t('Reports and diagnostics')}</h1>
    <nav aria-label={t('Report type')} className="flex shrink-0 flex-wrap gap-2">
      {views.map(([id, label]) => <button key={id} type="button" aria-current={view[0] === id ? 'page' : undefined}
        className={`rounded-md border px-3 py-2 text-sm ${view[0] === id ? 'bg-primary text-primary-foreground' : 'bg-card'}`}
        onClick={() => { const next = new URLSearchParams(params); next.set('view', id); setParams(next); }}>{t(label)}</button>)}
    </nav>
    <p className="shrink-0 text-sm text-muted-foreground">{t(view[2])}</p>
    <div className="min-h-0 flex-1 overflow-auto">
      {view[0] === 'server' ? <ServerLogs /> : view[0] === 'audit' ? <AuditHistory /> : view[0] === 'activity' ? <ActivityJournal /> : <MediaDiagnostics />}
    </div>
  </div>;
}
