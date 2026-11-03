/**
 * Axiom Forge - I18n Context & Provider
 * Multi-language support (English, Serbian, Spanish, Portuguese, Italian, German)
 */

import React, { createContext, useContext, useState, useMemo, useCallback } from 'react';

import en from './locales/en.json';
import sr from './locales/sr.json';
import es from './locales/es.json';
import pt from './locales/pt.json';
import it from './locales/it.json';
import de from './locales/de.json';

export const SUPPORTED_LOCALES = {
  en: { code: 'en', name: 'English', nativeName: 'English', flag: '🇬🇧' },
  sr: { code: 'sr', name: 'Serbian', nativeName: 'Srpski', flag: '🇷🇸' },
  es: { code: 'es', name: 'Spanish', nativeName: 'Español', flag: '🇪🇸' },
  pt: { code: 'pt', name: 'Portuguese', nativeName: 'Português', flag: '🇵🇹' },
  it: { code: 'it', name: 'Italian', nativeName: 'Italiano', flag: '🇮🇹' },
  de: { code: 'de', name: 'German', nativeName: 'Deutsch', flag: '🇩🇪' },
};

const translations = { en, sr, es, pt, it, de };

const I18nContext = createContext(null);

export function I18nProvider({ children }) {
  const [locale, setLocaleState] = useState(() => {
    try {
      const saved = localStorage.getItem('axiom_forge_locale');
      if (saved && translations[saved]) return saved;
      const navLang = navigator.language?.split('-')[0]?.toLowerCase();
      if (navLang && translations[navLang]) return navLang;
    } catch (e) {
      // Fallback
    }
    return 'en';
  });

  const setLocale = useCallback((newLocale) => {
    if (translations[newLocale]) {
      setLocaleState(newLocale);
      try {
        localStorage.setItem('axiom_forge_locale', newLocale);
      } catch (e) {}
    }
  }, []);

  const getNestedValue = (obj, keyPath) => {
    if (!obj || !keyPath) return null;
    const parts = keyPath.split('.');
    let current = obj;
    for (const part of parts) {
      if (current && typeof current === 'object' && part in current) {
        current = current[part];
      } else {
        return null;
      }
    }
    return typeof current === 'string' ? current : null;
  };

  const t = useCallback((key, params = {}) => {
    // 1. Look in active locale
    let text = getNestedValue(translations[locale], key);
    // 2. Fallback to English if missing
    if (text === null && locale !== 'en') {
      text = getNestedValue(translations.en, key);
    }
    // 3. Fallback to key itself
    if (text === null) {
      return key;
    }
    // Interpolate {param}
    if (params && typeof params === 'object') {
      Object.entries(params).forEach(([paramKey, paramVal]) => {
        text = text.replace(new RegExp(`\\{${paramKey}\\}`, 'g'), String(paramVal));
      });
    }
    return text;
  }, [locale]);

  const value = useMemo(() => ({
    locale,
    setLocale,
    t,
    locales: SUPPORTED_LOCALES,
    currentLocaleMeta: SUPPORTED_LOCALES[locale] || SUPPORTED_LOCALES.en
  }), [locale, setLocale, t]);

  return (
    <I18nContext.Provider value={value}>
      {children}
    </I18nContext.Provider>
  );
}

export function useI18n() {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    return {
      t: (k) => k,
      locale: 'en',
      setLocale: () => {},
      locales: SUPPORTED_LOCALES,
      currentLocaleMeta: SUPPORTED_LOCALES.en
    };
  }
  return ctx;
}

export const useTranslation = useI18n;

export function LanguageSelector({ className = '' }) {
  const { locale, setLocale, locales } = useI18n();
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className={`relative inline-block ${className}`}>
      <button
        type="button"
        onClick={() => setIsOpen(!isOpen)}
        className="flex items-center gap-1.5 px-2.5 py-1.5 text-xs font-medium rounded-lg bg-slate-800/80 hover:bg-slate-700 text-slate-300 hover:text-white border border-slate-700/60 transition-colors shadow-sm"
        title="Select Language"
      >
        <span>{locales[locale]?.flag}</span>
        <span className="hidden sm:inline font-sans">{locales[locale]?.nativeName}</span>
      </button>

      {isOpen && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setIsOpen(false)} />
          <div className="absolute right-0 mt-1.5 w-40 bg-slate-900 border border-slate-700 rounded-xl shadow-2xl py-1 z-50 overflow-hidden animate-in fade-in duration-150">
            {Object.values(locales).map((item) => (
              <button
                key={item.code}
                type="button"
                onClick={() => {
                  setLocale(item.code);
                  setIsOpen(false);
                }}
                className={`w-full flex items-center gap-2.5 px-3 py-2 text-xs text-left transition-colors ${
                  locale === item.code
                    ? 'bg-indigo-600/20 text-indigo-400 font-semibold'
                    : 'text-slate-300 hover:bg-slate-800 hover:text-white'
                }`}
              >
                <span>{item.flag}</span>
                <span>{item.nativeName}</span>
              </button>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
