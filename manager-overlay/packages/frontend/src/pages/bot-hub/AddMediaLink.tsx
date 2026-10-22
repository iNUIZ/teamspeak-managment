import { Plus } from 'lucide-react';
import { translate as uiText, useLanguageStore } from '@/lib/i18n';
import { Link } from 'react-router-dom';
import { Button } from '@/components/ui/button';

/** Shortcut from a console source tab to the page that manages its media. */
export function AddMediaLink({ to, label }: { to: string; label: string }) {
  useLanguageStore(state => state.language);
  return (
    <Button asChild variant="outline" className="min-h-11 shrink-0">
      <Link to={to}>
        <Plus className="mr-1.5 h-4 w-4" aria-hidden="true" /> {uiText(label)}
      </Link>
    </Button>
  );
}
