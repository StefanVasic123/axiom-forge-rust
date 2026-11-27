import React from 'react';
import { AlertTriangle, Download, ArrowRight, X, ShieldAlert } from 'lucide-react';
import { useI18n } from '../i18n/I18nContext';

const DEFAULT_RELEASES_URL = 'https://github.com/StefanVasic123/axiom-forge-rust/releases/latest';

export default function UpdatePromptModal({
  updateInfo,
  hasPendingBuild,
  onDownload,
  onContinueAnyway,
  onClose,
}) {
  const { t } = useI18n();

  if (!updateInfo) return null;

  const isRequired = updateInfo.status === 'update_required';
  const installed = updateInfo.installedVersion || '?';
  const latest = updateInfo.latestVersion || '?';
  const behind = updateInfo.versionsBehind ?? 0;
  const threshold = updateInfo.mandatoryThreshold ?? 10;
  const downloadUrl = updateInfo.downloadUrl || DEFAULT_RELEASES_URL;

  const handleDownloadClick = () => {
    if (onDownload) {
      onDownload(downloadUrl);
    } else if (window.electronAPI?.shell?.openExternal) {
      window.electronAPI.shell.openExternal(downloadUrl);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/80 backdrop-blur-sm p-4">
      <div
        className={`w-full max-w-md rounded-2xl border bg-slate-900 shadow-2xl overflow-hidden ${
          isRequired ? 'border-rose-500/50' : 'border-amber-500/40'
        }`}
      >
        <div
          className={`px-5 py-4 flex items-start justify-between gap-3 border-b ${
            isRequired
              ? 'bg-rose-500/10 border-rose-500/30'
              : 'bg-amber-500/10 border-amber-500/25'
          }`}
        >
          <div className="flex items-start gap-3">
            <div
              className={`w-10 h-10 rounded-xl flex items-center justify-center shrink-0 ${
                isRequired
                  ? 'bg-rose-500/20 text-rose-400 border border-rose-500/30'
                  : 'bg-amber-500/20 text-amber-400 border border-amber-500/30'
              }`}
            >
              {isRequired ? (
                <ShieldAlert className="w-5 h-5" />
              ) : (
                <AlertTriangle className="w-5 h-5" />
              )}
            </div>
            <div>
              <h3
                className={`text-sm font-bold ${
                  isRequired ? 'text-rose-200' : 'text-amber-200'
                }`}
              >
                {isRequired
                  ? t('update.requiredTitle')
                  : t('update.recommendedTitle', { latest })}
              </h3>
              <span
                className={`inline-block mt-1 text-[11px] font-mono px-2 py-0.5 rounded-full border ${
                  isRequired
                    ? 'bg-rose-500/15 text-rose-300 border-rose-500/30'
                    : 'bg-amber-500/15 text-amber-300 border-amber-500/30'
                }`}
              >
                {t('update.behindBadge', { behind })}
              </span>
            </div>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="text-slate-400 hover:text-slate-200 p-1 rounded-lg hover:bg-slate-800 transition-colors"
            aria-label={t('update.closeBtn')}
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        <div className="p-5 space-y-4">
          <p className="text-xs text-slate-300 leading-relaxed">
            {isRequired
              ? t('update.requiredDesc', { installed, latest, behind, threshold })
              : t('update.recommendedDesc', { installed, latest, behind })}
          </p>

          <div className="grid grid-cols-2 gap-3 rounded-xl bg-slate-950/70 border border-slate-800 p-3 text-xs">
            <div>
              <span className="text-[10px] uppercase tracking-wider text-slate-500 block">
                {t('update.currentLabel')}
              </span>
              <span className="font-mono font-semibold text-slate-200">v{installed}</span>
            </div>
            <div>
              <span className="text-[10px] uppercase tracking-wider text-slate-500 block">
                {t('update.latestLabel')}
              </span>
              <span
                className={`font-mono font-semibold ${
                  isRequired ? 'text-rose-400' : 'text-emerald-400'
                }`}
              >
                v{latest}
              </span>
            </div>
          </div>

          <div className="flex flex-col gap-2.5 pt-1">
            <button
              type="button"
              onClick={handleDownloadClick}
              className={`w-full py-2.5 px-4 rounded-xl font-semibold text-xs flex items-center justify-center gap-2 transition-colors ${
                isRequired
                  ? 'bg-rose-600 hover:bg-rose-500 text-white'
                  : 'bg-amber-500 hover:bg-amber-400 text-slate-950'
              }`}
            >
              <Download className="w-4 h-4" />
              {t('update.downloadBtn', { latest })}
            </button>

            {!isRequired && hasPendingBuild && onContinueAnyway && (
              <button
                type="button"
                onClick={onContinueAnyway}
                className="w-full py-2.5 px-4 rounded-xl font-medium text-xs bg-slate-800 hover:bg-slate-700 text-slate-100 border border-slate-700 flex items-center justify-center gap-2 transition-colors"
              >
                {t('update.continueAnywayBtn')}
                <ArrowRight className="w-3.5 h-3.5" />
              </button>
            )}

            {(!hasPendingBuild || isRequired) && (
              <button
                type="button"
                onClick={onClose}
                className="w-full py-2 px-4 rounded-xl font-medium text-xs bg-slate-800/70 hover:bg-slate-800 text-slate-300 border border-slate-700/70 transition-colors"
              >
                {isRequired ? t('update.closeBtn') : t('update.laterBtn')}
              </button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
