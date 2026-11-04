import { Link, useLocation } from 'react-router-dom';
import { ChevronLeft } from 'lucide-react';
import { useTranslation } from '@/lib/i18n';
import { settingsPageFor } from './Sidebar';

/** Pages opened from the Settings overview show the way back, since the menu does not list them. */
export function SettingsBackLink() {
  const { t } = useTranslation();
  const { pathname } = useLocation();
  const page = settingsPageFor(pathname);
  if (!page) return null;
  return (
    <nav aria-label={t('Breadcrumb')} className="mb-3 flex items-center gap-1 text-sm text-muted-foreground">
      <Link to="/settings" className="inline-flex items-center gap-1 rounded hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
        <ChevronLeft className="h-4 w-4" aria-hidden="true" />{t('Settings')}
      </Link>
      <span aria-hidden="true">/</span>
      <span className="text-foreground">{t(page.label)}</span>
    </nav>
  );
}
