import { AlertTriangle, Clipboard } from "lucide-react";
import { useTranslation } from "react-i18next";
import { LangCraftError } from "../ipc";

export function ErrorNotice({ error, onRetry }: { error: unknown; onRetry?: () => void }) {
  const { t } = useTranslation();
  const item =
    error instanceof LangCraftError
      ? error
      : new LangCraftError("APP_CLIENT", error instanceof Error ? error.message : String(error));
  const localized = t(`errors.${item.code}`, { defaultValue: item.message || t("errors.fallback") });

  return (
    <div className="error-notice" role="alert">
      <AlertTriangle aria-hidden="true" />
      <div>
        <strong>{localized}</strong>
        <code>{item.code}</code>
        {localized !== item.message && <p>{item.message}</p>}
      </div>
      <button
        className="icon-button"
        type="button"
        title={t("common.copy")}
        onClick={() => navigator.clipboard.writeText(`${item.code}: ${item.message}`)}
      >
        <Clipboard aria-hidden="true" />
      </button>
      {onRetry && (
        <button className="button secondary" type="button" onClick={onRetry}>
          {t("common.retry")}
        </button>
      )}
    </div>
  );
}

