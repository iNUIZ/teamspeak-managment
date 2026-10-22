import { translate as uiText, useLanguageStore as useUiLanguageStore } from '@/lib/i18n';
import { useLanguageStore, useTranslation } from '@/lib/i18n';

export function LanguageSwitch() {
  useUiLanguageStore(state => state.language);
  const { language } = useTranslation();
  const setLanguage = useLanguageStore((s) => s.setLanguage);
  return (
    <div className="language-switch" role="group" aria-label={uiText("Dil / Language")}>
      {(['tr', 'en'] as const).map((value) => (
        <button key={value} type="button" lang={value} aria-pressed={language === value}
          aria-label={value === 'tr' ? uiText("Türkçe") : uiText("English")} onClick={() => setLanguage(value)}>
          {value.toUpperCase()}
        </button>
      ))}
    </div>
  );
}
