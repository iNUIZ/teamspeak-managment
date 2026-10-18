import { useLanguageStore, useTranslation } from '@/lib/i18n';

export function LanguageSwitch() {
  const { language } = useTranslation();
  const setLanguage = useLanguageStore((s) => s.setLanguage);
  return (
    <div className="language-switch" role="group" aria-label="Dil / Language">
      {(['tr', 'en'] as const).map((value) => (
        <button key={value} type="button" lang={value} aria-pressed={language === value}
          aria-label={value === 'tr' ? 'Türkçe' : 'English'} onClick={() => setLanguage(value)}>
          {value.toUpperCase()}
        </button>
      ))}
    </div>
  );
}
