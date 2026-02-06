import React, { useState, useEffect, useCallback } from "react";
import { useTranslation } from "react-i18next";
import { getCookieStatus, deleteCookie } from "../../api";
import { formatTimestamp, formatIsoTimestamp } from "../../utils/formatters";
import { CookieStatusInfo, CookieItem } from "../../types/cookie.types";
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
  const [isForceRefreshing, setIsForceRefreshing] = useState(false);

  // Fetch cookie data
  const fetchCookieStatus = useCallback(async (forceRefresh = false) => {
    setLoading(true);
    setError(null);
    if (forceRefresh) {
      setIsForceRefreshing(true);
    }

    try {
      const response = await getCookieStatus(forceRefresh);
      const safeData: CookieStatusInfo = {
        valid: Array.isArray(response.data?.valid) ? response.data.valid : [],
        exhausted: Array.isArray(response.data?.exhausted)
          ? response.data.exhausted
          : [],
        invalid: Array.isArray(response.data?.invalid)
          ? response.data.invalid
          : [],
      };
      setCookieStatus(safeData);
      // Cache info is available in response.cacheInfo for debugging
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      setCookieStatus(emptyCookieStatus);
    } finally {
      setLoading(false);
      setIsForceRefreshing(false);
    }
  }, []);

  useEffect(() => {
    fetchCookieStatus();
  }, [fetchCookieStatus, refreshCounter]);

  const handleRefresh = (event?: React.MouseEvent<HTMLButtonElement>) => {
    const forceRefresh = event ? event.ctrlKey || event.metaKey : false;
    if (forceRefresh) {
      fetchCookieStatus(true);
    } else {
      setRefreshCounter((prev) => prev + 1);
    }
  };

  const getCooldownDisplay = (status: CookieItem) => {
    if (status.reset_time) {
      return {
        label: t("cookieStatus.status.cooldownFull") as string,
        time: formatTimestamp(status.reset_time),
      };
    }

    if (status.seven_day_opus_resets_at) {
      return {
        label: t("cookieStatus.status.cooldownOpus") as string,
        time: formatIsoTimestamp(status.seven_day_opus_resets_at),
      };
    }

    if (status.seven_day_sonnet_resets_at) {
      return {
        label: t("cookieStatus.status.cooldownSonnet") as string,
        time: formatIsoTimestamp(status.seven_day_sonnet_resets_at),
      };
    }

    if (status.seven_day_resets_at) {
      return {
        label: t("cookieStatus.status.cooldownFull") as string,
        time: formatIsoTimestamp(status.seven_day_resets_at),
      };
    }

    return null;
  };

  const renderUsageStats = (status: CookieItem) => {
    const s = status.session_usage || {};
    const w = status.weekly_usage || {};
    const ws = status.weekly_sonnet_usage || {};
    const wo = status.weekly_opus_usage || {};
    const lt = status.lifetime_usage || {};

    const groups: Array<{
      title: string;
      b: Required<
        Pick<
          typeof s,
          | "total_input_tokens"
          | "total_output_tokens"
          | "sonnet_input_tokens"
          | "sonnet_output_tokens"
          | "opus_input_tokens"
          | "opus_output_tokens"
        >
      >;
      showSonnet: boolean;
      showOpus: boolean;
    }> = [];

    const toReq = (x: typeof s) => ({
      total_input_tokens: x.total_input_tokens ?? 0,
      total_output_tokens: x.total_output_tokens ?? 0,
      sonnet_input_tokens: x.sonnet_input_tokens ?? 0,
      sonnet_output_tokens: x.sonnet_output_tokens ?? 0,
      opus_input_tokens: x.opus_input_tokens ?? 0,
      opus_output_tokens: x.opus_output_tokens ?? 0,
    });

    const sReq = toReq(s);
    const wReq = toReq(w);
    const wsReq = toReq(ws);
    const woReq = toReq(wo);
    const ltReq = toReq(lt);

    const anyNonZero = (req: typeof sReq) =>
      req.total_input_tokens > 0 ||
      req.total_output_tokens > 0 ||
      req.sonnet_input_tokens > 0 ||
      req.sonnet_output_tokens > 0 ||
      req.opus_input_tokens > 0 ||
      req.opus_output_tokens > 0;

    if (anyNonZero(sReq)) {
      groups.push({
        title: t("cookieStatus.quota.session") as string,
        b: sReq,
        showSonnet:
          sReq.sonnet_input_tokens > 0 || sReq.sonnet_output_tokens > 0,
        showOpus: sReq.opus_input_tokens > 0 || sReq.opus_output_tokens > 0,
      });
    }
    if (anyNonZero(wReq)) {
      groups.push({
        title: t("cookieStatus.quota.sevenDay") as string,
        b: wReq,
        showSonnet:
          wReq.sonnet_input_tokens > 0 || wReq.sonnet_output_tokens > 0,
        showOpus: wReq.opus_input_tokens > 0 || wReq.opus_output_tokens > 0,
      });
    }
    if (anyNonZero(wsReq)) {
      groups.push({
        title: t("cookieStatus.quota.sevenDaySonnet") as string,
        b: wsReq,
        showSonnet: true,
        showOpus: wsReq.opus_input_tokens > 0 || wsReq.opus_output_tokens > 0,
      });
    }
    if (anyNonZero(woReq)) {
      groups.push({
        title: t("cookieStatus.quota.sevenDayOpus") as string,
        b: woReq,
        // weekly_opus bucket only counts Opus; still guard by > 0
        showSonnet:
          woReq.sonnet_input_tokens > 0 || woReq.sonnet_output_tokens > 0,
        showOpus: woReq.opus_input_tokens > 0 || woReq.opus_output_tokens > 0,
      });
    }
    if (anyNonZero(ltReq)) {
      groups.push({
        title: t("cookieStatus.quota.total") as string,
        b: ltReq,
        showSonnet:
          ltReq.sonnet_input_tokens > 0 || ltReq.sonnet_output_tokens > 0,
        showOpus: ltReq.opus_input_tokens > 0 || ltReq.opus_output_tokens > 0,
      });
    }

    if (groups.length === 0) return null;

    const Row = ({ label, value }: { label: string; value: number }) => (
      <span>
        {label}: {value}
      </span>
    );

    return (
      <div className="grid gap-2 text-xs text-gray-400">
        {groups.map(({ title, b, showSonnet, showOpus }, idx) => (
          <div key={idx} className="flex flex-col gap-1">
            <div className="flex gap-3 flex-wrap">
              <span>
                {title} · {t("cookieStatus.usage.totalInput")}:{" "}
                {b.total_input_tokens}
              </span>
              <span>
                {t("cookieStatus.usage.totalOutput")}: {b.total_output_tokens}
              </span>
            </div>
            {showSonnet && (
              <div className="flex gap-3 flex-wrap pl-1 text-gray-500">
                <Row
                  label={t("cookieStatus.usage.sonnetInput") as string}
                  value={b.sonnet_input_tokens}
                />
                <Row
                  label={t("cookieStatus.usage.sonnetOutput") as string}
                  value={b.sonnet_output_tokens}
                />
              </div>
            )}
            {showOpus && (
              <div className="flex gap-3 flex-wrap pl-1 text-gray-500">
                <Row
                  label={t("cookieStatus.usage.opusInput") as string}
                  value={b.opus_input_tokens}
                />
                <Row
                  label={t("cookieStatus.usage.opusOutput") as string}
                  value={b.opus_output_tokens}
                />
              </div>
            )}
          </div>
        ))}
      </div>
    );
  };

  const renderQuotaStats = (status: CookieItem) => {
    const sess = status.session_utilization;
    const seven = status.seven_day_utilization;
    const sevenSonnet = status.seven_day_sonnet_utilization;
    const opus = status.seven_day_opus_utilization;
    const hasAny =
      typeof sess === "number" ||
      typeof seven === "number" ||
      typeof opus === "number" ||
      typeof sevenSonnet === "number";
    if (!hasAny) return null;
    return (
      <div className="grid gap-1 text-xs text-gray-400">
        {typeof sess === "number" && (
          <div>
            {t("cookieStatus.quota.session")}: {sess}%
            {status.session_resets_at && (
              <span className="ml-1 text-gray-500">
                {t("cookieStatus.quota.resetsAt", {
                  time: formatIsoTimestamp(status.session_resets_at),
                })}
              </span>
            )}
          </div>
        )}
        {typeof seven === "number" && (
          <div>
            {t("cookieStatus.quota.sevenDay")}: {seven}%
            {status.seven_day_resets_at && (
              <span className="ml-1 text-gray-500">
                {t("cookieStatus.quota.resetsAt", {
                  time: formatIsoTimestamp(status.seven_day_resets_at),
                })}
              </span>
            )}
          </div>
        )}
        {typeof sevenSonnet === "number" && (
          <div>
            {t("cookieStatus.quota.sevenDaySonnet")}: {sevenSonnet}%
            {status.seven_day_sonnet_resets_at && (
              <span className="ml-1 text-gray-500">
                {t("cookieStatus.quota.resetsAt", {
                  time: formatIsoTimestamp(status.seven_day_sonnet_resets_at),
                })}
              </span>
            )}
          </div>
        )}
        {typeof opus === "number" && (
          <div>
            {t("cookieStatus.quota.sevenDayOpus")}: {opus}%
            {status.seven_day_opus_resets_at && (
              <span className="ml-1 text-gray-500">
                {t("cookieStatus.quota.resetsAt", {
                  time: formatIsoTimestamp(status.seven_day_opus_resets_at),
                })}
              </span>
            )}
          </div>
        )}
      </div>
    );
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
        setError(errorMessage);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setDeletingCookie(null);
    }
  };

  // Helper for getting reason text from cookie reason object
  const getReasonText = (reason: unknown): string => {
    if (!reason) return t("cookieStatus.status.reasons.unknown");
    if (typeof reason === "string") return reason;

    try {
      if (typeof reason === "object" && reason !== null) {
        const r = reason as Record<string, unknown>;
        if ("Free" in r) return t("cookieStatus.status.reasons.freAccount");
        if ("Disabled" in r) return t("cookieStatus.status.reasons.disabled");
        if ("Banned" in r) return t("cookieStatus.status.reasons.banned");
        if ("Null" in r) return t("cookieStatus.status.reasons.invalid");
        if ("Restricted" in r && typeof r["Restricted"] === "number")
          return t("cookieStatus.status.reasons.restricted", {
            time: formatTimestamp(r["Restricted"] as number),
          });
        if ("TooManyRequest" in r && typeof r["TooManyRequest"] === "number")
          return t("cookieStatus.status.reasons.rateLimited", {
            time: formatTimestamp(r["TooManyRequest"] as number),
          });
      }
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

  const renderContextBadge = (
    laneLabel: string,
    flag: boolean | null | undefined
  ) => {
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
        {laneLabel}: {label}
      </span>
    );
  };

  const renderContextBadges = (status: CookieItem) => {
    const sonnetFlag = status.supports_claude_1m_sonnet;
    const opusFlag = status.supports_claude_1m_opus;
    if (sonnetFlag === undefined && opusFlag === undefined) {
      return null;
    }
    return (
      <>
        {renderContextBadge(t("cookieStatus.context.sonnetLane"), sonnetFlag)}
        {renderContextBadge(t("cookieStatus.context.opusLane"), opusFlag)}
      </>
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
        <div className="relative group">
          <Button
            onClick={handleRefresh}
            className={`p-2 rounded-md transition-colors text-sm ${
              isForceRefreshing
                ? "bg-orange-600 hover:bg-orange-500"
                : "bg-gray-700 hover:bg-gray-600"
            }`}
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
                {isForceRefreshing
                  ? t("cookieStatus.forceRefreshing")
                  : t("cookieStatus.refreshing")}
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
          {/* Tooltip */}
          <div className="invisible group-hover:visible opacity-0 group-hover:opacity-100 transition-opacity absolute right-0 top-full mt-2 w-auto whitespace-nowrap bg-gray-800 text-white text-xs rounded-lg p-2 z-10 shadow-lg">
            <div className="text-gray-200">
              {t("cookieStatus.refreshTooltip")}
            </div>
          </div>
        </div>
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
            const contextBadge = renderContextBadges(status);
            const usageStats = renderUsageStats(status);
            const quotaStats = renderQuotaStats(status);
            const hasMeta = contextBadge || usageStats || quotaStats;
            return (
              <div
                key={index}
                className="py-2 text-sm text-gray-300 flex flex-wrap justify-between items-start"
              >
                <div className="text-green-300 flex-grow mr-4 min-w-0 mb-1 sm:mb-0">
                  <CookieValue cookie={status.cookie} />
                  {hasMeta && (
                    <details className="mt-1 text-xs text-gray-400">
                      <summary className="cursor-pointer text-gray-500 hover:text-gray-300">
                        {t("cookieStatus.meta.summary")}
                      </summary>
                      <div className="mt-2 space-y-2">
                        {contextBadge && (
                          <div className="flex items-center gap-2 text-gray-300">
                            {contextBadge}
                          </div>
                        )}
                        {usageStats}
                        {quotaStats}
                      </div>
                    </details>
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
            const contextBadge = renderContextBadges(status);
            const usageStats = renderUsageStats(status);
            const quotaStats = renderQuotaStats(status);
            const hasMeta = contextBadge || usageStats || quotaStats;
            return (
              <div
                key={index}
                className="py-2 flex flex-wrap justify-between text-sm items-start"
              >
                <div className="text-yellow-300 flex-grow mr-4 min-w-0 mb-1 sm:mb-0">
                  <CookieValue cookie={status.cookie} />
                  {hasMeta && (
                    <details className="mt-1 text-xs text-gray-400">
                      <summary className="cursor-pointer text-gray-500 hover:text-gray-300">
                        {t("cookieStatus.meta.summary")}
                      </summary>
                      <div className="mt-2 space-y-2">
                        {contextBadge && (
                          <div className="flex items-center gap-2 text-gray-300">
                            {contextBadge}
                          </div>
                        )}
                        {usageStats}
                        {quotaStats}
                      </div>
                    </details>
                  )}
                </div>
                <div className="flex items-center">
                  <span className="text-gray-400">
                    {(() => {
                      const cooldown = getCooldownDisplay(status);
                      if (!cooldown)
                        return t("cookieStatus.status.unknownReset");
                      return `${cooldown.label}: ${cooldown.time}`;
                    })()}
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
                  {renderUsageStats(status)}
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
