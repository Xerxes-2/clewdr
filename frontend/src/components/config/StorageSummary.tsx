import { useState } from "react";
import { useTranslation } from "react-i18next";

interface StorageSummaryProps {
  status: any;
  mode?: string;
}

export function StorageSummary({ status, mode }: StorageSummaryProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);

  const resolvedMode = mode ?? "file";
  const modeLabel = t(`config.storage.modes.${resolvedMode}`, {
    defaultValue: resolvedMode,
  });

  if (!status) {
    return (
      <div className="space-y-2">
        <div className="text-md font-medium text-cyan-300">
          {t("config.storage.modeLabel")}
        </div>
        <div className="text-white text-sm font-medium">{modeLabel}</div>
      </div>
    );
  }

  const infoLines: string[] = [];
  if (status.details?.driver) {
    infoLines.push(t("config.storage.driver", { driver: status.details.driver }));
  }
  if (typeof status.details?.latency_ms === "number") {
    infoLines.push(t("config.storage.latency", { latency: status.details.latency_ms }));
  }
  if (status.details?.sqlite_path) {
    infoLines.push(t("config.storage.sqlitePath", { path: status.details.sqlite_path }));
  }
  if (status.details?.database_url) {
    infoLines.push(t("config.storage.databaseUrl", { url: status.details.database_url }));
  }
  if (typeof status.last_write_ts === "number" && status.last_write_ts > 0) {
    infoLines.push(
      t("config.storage.lastWrite", { time: new Date(status.last_write_ts * 1000).toLocaleString() })
    );
  }
  if (typeof status.total_writes === "number") {
    infoLines.push(t("config.storage.totalWrites", { count: status.total_writes }));
  }
  if (typeof status.avg_write_ms === "number") {
    infoLines.push(t("config.storage.avgWriteMs", { ms: status.avg_write_ms.toFixed(2) }));
  }
  if (typeof status.failure_ratio === "number") {
    infoLines.push(
      t("config.storage.failureRatio", {
        ratio: (status.failure_ratio * 100).toFixed(2),
      })
    );
  }
  if (typeof status.retry_count === "number") {
    infoLines.push(t("config.storage.retryCount", { count: status.retry_count }));
  }
  if (typeof status.write_error_count === "number") {
    infoLines.push(t("config.storage.writeErrors", { count: status.write_error_count }));
  }
  if (status.error) {
    infoLines.push(t("config.storage.error", { error: status.error }));
  }
  if (status.last_error) {
    infoLines.push(t("config.storage.lastError", { error: status.last_error }));
  }

  const hasDetails = infoLines.length > 0;

  const isEnabled = Boolean(status.enabled);
  const isFileMode = resolvedMode === "file";
  const healthKey = !isEnabled || isFileMode
    ? "na"
    : status.healthy
    ? "ok"
    : "down";

  const allowDetails = hasDetails;

  return (
    <div className="space-y-2">
      <div className="text-md font-medium text-cyan-300">
        {t("config.storage.modeLabel")}
      </div>
      <div className="text-white text-sm font-medium">{modeLabel}</div>
      <div className="text-xs text-gray-400">
        {t("config.storage.healthLabel")}: {t(`config.storage.health.${healthKey}`)}
      </div>
      {allowDetails && (
        <button
          type="button"
          onClick={() => setExpanded((prev) => !prev)}
          className="mt-1 text-xs text-gray-400 underline hover:text-gray-200 transition-colors block w-fit"
        >
          {expanded
            ? t("config.storage.details.hide")
            : t("config.storage.details.show")}
        </button>
      )}
      {expanded && allowDetails && (
        <div className="text-xs text-gray-400 space-y-1">
          {infoLines.map((line, index) => (
            <div key={index} className="break-words">
              {line}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
