// frontend/src/components/gemini/aistudio/KeyVisualization.tsx
import React, { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { getKeyStatus, deleteKey } from "../../../api/keyApi";
import { KeyStatusInfo } from "../../../types/key.types";
import Button from "../../common/Button";
import LoadingSpinner from "../../common/LoadingSpinner";
import StatusMessage from "../../common/StatusMessage";
import KeyValue from "./KeyValue";
import DeleteButton from "./DeleteButton";

// Default empty state
const emptyKeyStatus: KeyStatusInfo = {
  valid: [],
};

const KeyVisualization: React.FC = () => {
  const { t } = useTranslation();
  const [keyStatus, setKeyStatus] = useState<KeyStatusInfo>(emptyKeyStatus);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshCounter, setRefreshCounter] = useState(0);
  const [deletingKey, setDeletingKey] = useState<string | null>(null);

  // Fetch key data
  const fetchKeyStatus = async () => {
    setLoading(true);
    setError(null);

    try {
      const data = await getKeyStatus();
      const safeData: KeyStatusInfo = {
        valid: Array.isArray(data?.valid) ? data.valid : [],
      };
      setKeyStatus(safeData);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(translateError(message));
      setKeyStatus(emptyKeyStatus);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchKeyStatus();
  }, [refreshCounter]);

  const handleRefresh = () => setRefreshCounter((prev) => prev + 1);

  const translateError = (message: string) => {
    if (message.includes("Database storage is unavailable")) {
      return t("common.dbUnavailable");
    }
    return message;
  };

  const handleDeleteKey = async (key: string) => {
    if (!window.confirm(t("keyStatus.deleteConfirm"))) return;

    setDeletingKey(key);
    setError(null);

    try {
      const response = await deleteKey(key);

      if (response.ok) {
        handleRefresh();
      } else {
        const errorMessage =
          response.status === 401
            ? t("keySubmit.error.auth")
            : await response
                .json()
                .then(
                  (data) =>
                    data.error ||
                    t("common.error", { message: response.status })
                );
        setError(translateError(errorMessage));
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(translateError(message));
    } finally {
      setDeletingKey(null);
    }
  };

  // Calculate total key count
  const totalKeys = keyStatus.valid.length;

  return (
    <div className="space-y-6 w-full">
      {/* Header */}
      <div className="flex justify-between items-center mb-4 w-full">
        <div>
          <h3 className="text-lg font-semibold text-white">
            {t("keyStatus.title")}
          </h3>
          <p className="text-xs text-gray-400 mt-1">
            {t("keyStatus.total", { count: totalKeys })}
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
              <svg
                className="animate-spin h-4 w-4 mr-2"
                fill="none"
                viewBox="0 0 24 24"
              >
                <circle
                  className="opacity-25"
                  cx="12"
                  cy="12"
                  r="10"
                  stroke="currentColor"
                  strokeWidth="4"
                ></circle>
                <path
                  className="opacity-75"
                  fill="currentColor"
                  d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
                ></path>
              </svg>
              {t("keyStatus.refreshing")}
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
              {t("keyStatus.refresh")}
            </span>
          )}
        </Button>
      </div>

      {/* Error Display */}
      {error && <StatusMessage type="error" message={error} />}

      {/* Loading State */}
      {loading && totalKeys === 0 && (
        <div className="flex justify-center py-8">
          <LoadingSpinner size="lg" color="text-cyan-500" />
        </div>
      )}

      {/* Valid Keys Section */}
      <div className="rounded-lg border border-purple-800 bg-purple-900/20 p-4">
        <h4 className="text-purple-300 font-medium mb-3">
          {t("keyStatus.sections.valid")}
        </h4>

        {keyStatus.valid.length === 0 ? (
          <p className="text-sm text-gray-400 py-2">{t("keyStatus.noKeys")}</p>
        ) : (
          <div className="space-y-2">
            {keyStatus.valid
              .slice()
              .sort((a, b) => (b.count_403 || 0) - (a.count_403 || 0))
              .map((status, index) => (
                <div
                  key={index}
                  className="py-2 text-sm text-gray-300 flex flex-wrap justify-between items-start border-b border-purple-800/30 last:border-0"
                >
                  <div className="text-purple-300 flex-grow mr-4 min-w-0 mb-1 sm:mb-0">
                    <KeyValue keyString={status.key} />
                  </div>
                  <div className="flex items-center space-x-3">
                    {typeof status.count_403 === "number" && (
                      <span className="text-orange-400 bg-orange-900/30 px-2 py-0.5 rounded text-xs">
                        403: {status.count_403}
                      </span>
                    )}
                    <DeleteButton
                      keyString={status.key}
                      onDelete={handleDeleteKey}
                      isDeleting={deletingKey === status.key}
                    />
                  </div>
                </div>
              ))}
          </div>
        )}
      </div>

      {/* No Keys Help Text */}
      {!loading && totalKeys === 0 && (
        <div className="mt-4 px-4 py-3 bg-gray-800/50 border border-gray-700 rounded-md">
          <p className="text-sm text-gray-300">{t("keyStatus.emptyHelp")}</p>
        </div>
      )}
    </div>
  );
};

export default KeyVisualization;
