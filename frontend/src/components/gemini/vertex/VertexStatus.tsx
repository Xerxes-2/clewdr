import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  deleteVertexCredential,
  getVertexCredentials,
} from "../../../api/vertexApi";
import { VertexCredentialInfo } from "../../../types/vertex.types";
import Button from "../../common/Button";
import LoadingSpinner from "../../common/LoadingSpinner";
import StatusMessage from "../../common/StatusMessage";
import VertexCredentialValue from "./VertexCredentialValue";
import VertexDeleteButton from "./VertexDeleteButton";

interface VertexStatusProps {
  refreshToken: number;
}

const emptyStatus: VertexCredentialInfo = {
  credentials: [],
};

const VertexStatus: React.FC<VertexStatusProps> = ({ refreshToken }) => {
  const { t } = useTranslation();
  const [status, setStatus] = useState<VertexCredentialInfo>(emptyStatus);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [statusFeedback, setStatusFeedback] = useState<{
    type: "success" | "error" | "warning" | "info";
    message: string;
  }>({ type: "info", message: "" });
  const [deletingId, setDeletingId] = useState<string | null>(null);

  const translateError = useCallback(
    (message: string) => {
      if (message.includes("Database storage is unavailable")) {
        return t("common.dbUnavailable");
      }
      return message;
    },
    [t]
  );

  const fetchCredentials = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getVertexCredentials();
      const safeData: VertexCredentialInfo = {
        credentials: Array.isArray(data?.credentials) ? data.credentials : [],
      };
      setStatus(safeData);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(translateError(message));
      setStatus(emptyStatus);
    } finally {
      setLoading(false);
    }
  }, [translateError]);

  useEffect(() => {
    fetchCredentials();
  }, [fetchCredentials, refreshToken]);

  const handleRefresh = () => {
    setStatusFeedback({ type: "info", message: "" });
    fetchCredentials();
  };

  const handleDeleteCredential = async (id: string) => {
    if (!window.confirm(t("geminiTab.vertexStatus.deleteConfirm"))) {
      return;
    }

    setDeletingId(id);
    setStatusFeedback({ type: "info", message: "" });
    setError(null);

    try {
      await deleteVertexCredential(id);
      setStatusFeedback({
        type: "success",
        message: t("geminiTab.vertexStatus.cleared"),
      });
      await fetchCredentials();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setStatusFeedback({
        type: "error",
        message: t("geminiTab.vertexSubmit.error.general", { message }),
      });
    } finally {
      setDeletingId(null);
    }
  };

  const credentialCount = status.credentials.length;
  const hasCredentials = credentialCount > 0;

  return (
    <div className="space-y-6 w-full">
      <div className="flex justify-between items-center mb-4 w-full">
        <div>
          <h3 className="text-lg font-semibold text-white">
            {t("geminiTab.vertexStatus.title")}
          </h3>
          <p className="text-xs text-gray-400 mt-1">
            {t("geminiTab.vertexStatus.description")}
          </p>
        </div>
        <Button
          onClick={handleRefresh}
          className="p-2 bg-gray-700 hover:bg-gray-600 rounded-md transition-colors text-sm"
          disabled={loading}
          variant="secondary"
        >
          {loading ? (
            <span className="flex items-center">
              <LoadingSpinner size="sm" color="text-cyan-300" />
              <span className="ml-2">{t("geminiTab.status.loading")}</span>
            </span>
          ) : (
            <span className="flex items-center">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                className="h-4 w-4 mr-2"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                />
              </svg>
              {t("geminiTab.vertexStatus.refresh")}
            </span>
          )}
        </Button>
      </div>

      {error && <StatusMessage type="error" message={error} />}

      {statusFeedback.message && (
        <StatusMessage type={statusFeedback.type} message={statusFeedback.message} />
      )}

      <div className="rounded-lg border border-purple-800 bg-purple-900/20 p-4 space-y-4">
        <div className="flex items-center justify-between">
          <h4 className="text-purple-200 font-semibold">
            {hasCredentials
              ? t("geminiTab.vertexStatus.badge.configured")
              : t("geminiTab.vertexStatus.badge.empty")}
          </h4>
          {hasCredentials && (
            <span className="text-xs text-gray-400">
              {t("geminiTab.vertexStatus.total", { count: credentialCount })}
            </span>
          )}
        </div>

        {loading && !hasCredentials ? (
          <div className="flex items-center space-x-2 text-gray-300">
            <LoadingSpinner size="sm" color="text-cyan-300" />
            <span className="text-sm">{t("geminiTab.status.loading")}</span>
          </div>
        ) : !hasCredentials ? (
          <p className="text-sm text-gray-400">
            {t("geminiTab.vertexStatus.empty")}
          </p>
        ) : (
          <div className="space-y-2">
            {status.credentials
              .slice()
              .sort((a, b) => (b.count_403 ?? 0) - (a.count_403 ?? 0))
              .map((credential) => (
                <div
                  key={credential.id}
                  className="py-2 text-sm text-gray-300 flex flex-wrap justify-between items-start border-b border-purple-800/30 last:border-0"
                >
                  <div className="flex-grow mr-4 min-w-0 mb-1 sm:mb-0">
                    <VertexCredentialValue credential={credential} />
                  </div>
                  <div className="flex items-center space-x-3">
                    {typeof credential.count_403 === "number" && (
                      <span className="text-orange-300 bg-orange-900/30 px-2 py-0.5 rounded text-xs">
                        {t("geminiTab.vertexStatus.count", {
                          count: credential.count_403,
                        })}
                      </span>
                    )}
                    <VertexDeleteButton
                      credentialId={credential.id}
                      onDelete={handleDeleteCredential}
                      isDeleting={deletingId === credential.id}
                    />
                  </div>
                </div>
              ))}
          </div>
        )}
      </div>
    </div>
  );
};

export default VertexStatus;
