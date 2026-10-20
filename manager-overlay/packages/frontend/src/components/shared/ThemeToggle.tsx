import { Moon, Sun } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useUiStore } from '@/stores/ui.store';
import { useTranslation } from '@/lib/i18n';

export function ThemeToggle() {
  const { t } = useTranslation();
  const { baseTheme, toggleTheme } = useUiStore();
  const nextTheme = baseTheme === 'light' ? 'dark' : 'light';
  return (
    <Button
      variant="ghost"
      size="icon"
      onClick={toggleTheme}
      className="h-10 w-10 sm:h-8 sm:w-8"
      aria-label={t(`Switch to ${nextTheme} theme`)}
      title={t(`Switch to ${nextTheme} theme`)}
    >
      {baseTheme === 'light' ? <Moon className="h-4 w-4" /> : <Sun className="h-4 w-4" />}
    </Button>
  );
}
