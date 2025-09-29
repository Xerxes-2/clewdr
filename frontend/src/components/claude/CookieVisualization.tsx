import React, { useState, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { getCookieStatus, deleteCookie } from "../../api";
import { formatTimestamp } from "../../utils/formatters";
import { CookieStatusInfo } from "../../types/cookie.types";
import Button from "../common/Button";
import LoadingSpinner from "../common/LoadingSpinner";
import StatusMessage from "../common/StatusMessage";
import CookieSection from "./CookieSection";
import CookieValue from "./CookieValue";
import DeleteButton from "./DeleteButton";

// Default empty state
const emptyCookieStatus: CookieStatusInfo = {
  valid: [],
  exhausted: [],
  invalid: [],
};

const CookieVisualization: React.FC = () => {
  const { t } = useTranslation();
  const [cookieStatus, setCookieStatus] =
    useState<CookieStatusInfo>(emptyCookieStatus);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshCounter, setRefreshCounter] = useState(0);
  const [deletingCookie, setDeletingCookie] = useState<string | null>(null);

  // Fetch cookie data
  const fetchCookieStatus = async () => {
    setLoading(true);
    setError(null);

    try {
      const data = await getCookieStatus();
      const safeData: CookieStatusInfo = {
        valid: Array.isArray(data?.valid) ? data.valid : [],
        exhausted: Array.isArray(data?.exhausted) ? data.exhausted : [],
        invalid: Array.isArray(data?.invalid) ? data.invalid : [],
      };
      setCookieStatus(safeData);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(translateError(message));
      setCookieStatus(emptyCookieStatus);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchCookieStatus();
  }, [refreshCounter]);

  const handleRefresh = () => setRefreshCounter((prev) => prev + 1);

  const translateError = (message: string) => {
    if (message.includes("Database storage is unavailable")) {
      return t("common.dbUnavailable");
    }
    return message;
  };

  const handleDeleteCookie = async (cookie: string) => {
    if (!window.confirm(t("cookieStatus.deleteConfirm"))) return;

    setDeletingCookie(cookie);
    setError(null);

    try {
      const response = await deleteCookie(cookie);

      if (response.ok) {
        handleRefresh();
      } else {
        const errorMessage =
          response.status === 401
            ? t("cookieSubmit.error.auth")
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
      setDeletingCookie(null);
    }
  };

  // Helper for getting reason text from cookie reason object
  const getReasonText = (reason: any): string => {
    if (!reason) return t("cookieStatus.status.reasons.unknown");
    if (typeof reason === "string") return reason;

    try {
      if ("NonPro" in reason)
        return t("cookieStatus.status.reasons.freAccount");
      if ("Disabled" in reason)
        return t("cookieStatus.status.reasons.disabled");
      if ("Banned" in reason) return t("cookieStatus.status.reasons.banned");
      if ("Null" in reason) return t("cookieStatus.status.reasons.invalid");
      if ("Restricted" in reason && typeof reason.Restricted === "number")
        return t("cookieStatus.status.reasons.restricted", {
          time: formatTimestamp(reason.Restricted),
        });
      if (
        "TooManyRequest" in reason &&
        typeof reason.TooManyRequest === "number"
      )
        return t("cookieStatus.status.reasons.rateLimited", {
          time: formatTimestamp(reason.TooManyRequest),
        });
    } catch (e) {
      console.error("Error parsing reason:", e, reason);
    }
    return t("cookieStatus.status.reasons.unknown");
  };

  // Calculate total cookie count
  const totalCookies =
    cookieStatus.valid.length +
    cookieStatus.exhausted.length +
    cookieStatus.invalid.length;

  const renderContextBadge = (flag: boolean | null | undefined) => {
    if (flag === undefined) {
      return null;
    }

    const { label, classes } = (() => {
      if (flag === true) {
        return {
          label: t("cookieStatus.context.enabled"),
          classes:
            "bg-emerald-500/20 text-emerald-200 border border-emerald-400/60",
        };
      }
      if (flag === false) {
        return {
          label: t("cookieStatus.context.disabled"),
          classes: "bg-red-500/20 text-red-200 border border-red-500/60",
        };
      }
      return {
        label: t("cookieStatus.context.unknown"),
        classes: "bg-gray-700 text-gray-300 border border-gray-600/80",
      };
    })();

    return (
      <span
        className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium border ${classes}`}
      >
        {label}
      </span>
    );
  };

  return (
    <div className="space-y-6 w-full">
      {/* Header */}
      <div className="flex justify-between items-center mb-4 w-full">
        <div>
          <h3 className="text-lg font-semibold text-white">
            {t("cookieStatus.title")}
          </h3>
          <p className="text-xs text-gray-400 mt-1">
            {t("cookieStatus.total", { count: totalCookies })}
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
              {t("cookieStatus.refreshing")}
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
              {t("cookieStatus.refresh")}
            </span>
          )}
        </Button>
      </div>

      {/* Error Display */}
      {error && <StatusMessage type="error" message={error} />}

      {/* Loading State */}
      {loading && totalCookies === 0 && (
        <div className="flex justify-center py-8">
          <LoadingSpinner size="lg" color="text-cyan-500" />
        </div>
      )}

      {/* Cookie Sections */}
      <div className="space-y-6 w-full">
        {/* Valid Cookies */}
        <CookieSection
          title={t("cookieStatus.sections.valid")}
          cookies={cookieStatus.valid}
          color="green"
          renderStatus={(status, index) => {
            const contextBadge = renderContextBadge(status.supports_claude_1m);
            return (
              <div
                key={index}
                className="py-2 text-sm text-gray-300 flex flex-wrap justify-between items-start"
              >
                <div className="text-green-300 flex-grow mr-4 min-w-0 mb-1 sm:mb-0">
                  <CookieValue cookie={status.cookie} />
                  {contextBadge && (
                    <div className="mt-1 flex items-center gap-2">
                      {contextBadge}
                    </div>
                  )}
                </div>
                <div className="flex items-center">
                  <span className="text-gray-400">
                    {t("cookieStatus.status.available")}
                  </span>
                  <DeleteButton
                    cookie={status.cookie}
                    onDelete={handleDeleteCookie}
                    isDeleting={deletingCookie === status.cookie}
                  />
                </div>
              </div>
            );
          }}
        />

        {/* Exhausted Cookies */}
        <CookieSection
          title={t("cookieStatus.sections.exhausted")}
          cookies={cookieStatus.exhausted}
          color="yellow"
          renderStatus={(status, index) => {
            const contextBadge = renderContextBadge(status.supports_claude_1m);
            return (
              <div
                key={index}
                className="py-2 flex flex-wrap justify-between text-sm items-start"
              >
                <div className="text-yellow-300 flex-grow mr-4 min-w-0 mb-1 sm:mb-0">
                  <CookieValue cookie={status.cookie} />
                  {contextBadge && (
                    <div className="mt-1 flex items-center gap-2">
                      {contextBadge}
                    </div>
                  )}
                </div>
                <div className="flex items-center">
                  <span className="text-gray-400">
                    {status.reset_time
                      ? t("cookieStatus.status.resets", {
                          time: formatTimestamp(status.reset_time),
                        })
                      : t("cookieStatus.status.unknownReset")}
                  </span>
                  <DeleteButton
                    cookie={status.cookie}
                    onDelete={handleDeleteCookie}
                    isDeleting={deletingCookie === status.cookie}
                  />
                </div>
              </div>
            );
          }}
        />

        {/* Invalid Cookies */}
        <CookieSection
          title={t("cookieStatus.sections.invalid")}
          cookies={cookieStatus.invalid}
          color="red"
          renderStatus={(status, index) => {
            return (
              <div
                key={index}
                className="py-2 flex flex-wrap justify-between text-sm items-start"
              >
                <div className="text-red-300 flex-grow mr-4 min-w-0 mb-1 sm:mb-0">
                  <CookieValue cookie={status.cookie} />
                </div>
                <div className="flex items-center">
                  <span className="text-gray-400">
                    {getReasonText(status.reason)}
                  </span>
                  <DeleteButton
                    cookie={status.cookie}
                    onDelete={handleDeleteCookie}
                    isDeleting={deletingCookie === status.cookie}
                  />
                </div>
              </div>
            );
          }}
        />
      </div>
    </div>
  );
};

export default CookieVisualization;
