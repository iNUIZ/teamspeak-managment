import { Link } from 'react-router-dom';
import { useTranslation } from '@/lib/i18n';
export function ServiceTabs({ kind, selected }: { kind: 'servers' | 'permissions'; selected: string }) {
  const { t } = useTranslation();
  const rows = kind === 'servers'
    ? [['/servers', 'servers', 'TeamSpeak servers'], ['/servers?tab=service', 'service', 'Server service information']]
    : [['/permissions', 'permissions', 'Permissions'], ['/permissions?tab=service-groups', 'groups', 'New-server group defaults']];
  return <nav className="mb-4 flex flex-wrap gap-2" aria-label={t('Server management sections')}>{rows.map(([url, id, label]) =>
    <Link key={id} to={url} aria-current={selected === id ? 'page' : undefined} className={`rounded-md border px-3 py-2 text-sm ${selected === id ? 'bg-primary text-primary-foreground' : 'bg-card'}`}>{t(label)}</Link>)}</nav>;
}
