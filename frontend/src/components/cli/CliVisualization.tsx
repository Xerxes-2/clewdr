import React, { useEffect, useState } from "react";
import Button from "../common/Button";
import LoadingSpinner from "../common/LoadingSpinner";
import StatusMessage from "../common/StatusMessage";
import { deleteCliToken, getCliTokens } from "../../api";

type CliToken = {
  token: { inner?: string } | string;
  count_403?: number;
  expiry?: string | null;
  meta?: Record<string, any> | null;
};

type CliTokenStatusInfo = {
  valid: CliToken[];
};

const emptyStatus: CliTokenStatusInfo = { valid: [] };

const getTokenString = (tok: CliToken): string => {
  // Server may return token as string or as { inner: string }
  const v = (typeof tok.token === "string" ? tok.token : tok.token?.inner) || "";
  return v;
};

const CliVisualization: React.FC = () => {
  const [status, setStatus] = useState<CliTokenStatusInfo>(emptyStatus);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [refreshCounter, setRefreshCounter] = useState(0);
  const [deleting, setDeleting] = useState<string | null>(null);

  const fetchStatus = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getCliTokens();
      const safe: CliTokenStatusInfo = {
        valid: Array.isArray(data?.valid) ? data.valid : [],
      };
      setStatus(safe);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setStatus(emptyStatus);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    fetchStatus();
  }, [refreshCounter]);

  const handleRefresh = () => setRefreshCounter((p) => p + 1);

  const handleDelete = async (token: string) => {
    if (!window.confirm("Delete this CLI token?")) return;
    setDeleting(token);
    setError(null);
    try {
      const resp = await deleteCliToken(token);
      if (resp.ok) {
        handleRefresh();
      } else {
        const msg = await resp
          .json()
          .then((d) => d.error || `Error: ${resp.status}`)
          .catch(() => `Error: ${resp.status}`);
        setError(msg);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setDeleting(null);
    }
  };

  const total = status.valid.length;

  return (
    <div className="space-y-6 w-full">
      <div className="flex justify-between items-center mb-4 w-full">
        <div>
          <h3 className="text-lg font-semibold text-white">CLI Tokens</h3>
          <p className="text-xs text-gray-400 mt-1">Total: {total}</p>
        </div>
        <Button
          onClick={handleRefresh}
          className="p-2 bg-gray-700 hover:bg-gray-600 rounded-md transition-colors text-sm"
          disabled={loading}
          variant="secondary"
        >
          {loading ? (
            <span className="flex items-center">
              <svg className="animate-spin h-4 w-4 mr-2" fill="none" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
              </svg>
              Refreshing
            </span>
          ) : (
            <span className="flex items-center">
              <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4 mr-2" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              Refresh
            </span>
          )}
        </Button>
      </div>

      {error && <StatusMessage type="error" message={error} />} 

      {loading && total === 0 && (
        <div className="flex justify-center py-8">
          <LoadingSpinner size="lg" color="text-cyan-500" />
        </div>
      )}

      <div className="space-y-2">
        {status.valid.map((tok) => {
          const value = getTokenString(tok);
          const short = value ? `${value.slice(0, 10)}...` : "";
          return (
            <div key={value} className="flex items-center justify-between bg-gray-800/50 rounded p-2 text-sm">
              <div className="flex-1">
                <code className="bg-gray-900 px-2 py-1 rounded text-xs">{short}</code>
                {typeof tok.count_403 === "number" && (
                  <span className="ml-3 text-xs text-gray-400">403: {tok.count_403}</span>
                )}
                {tok.expiry && (
                  <span className="ml-3 text-xs text-gray-400">exp: {new Date(tok.expiry).toLocaleString()}</span>
                )}
              </div>
              <button
                className={`ml-2 p-1 rounded-md transition-colors ${
                  deleting === value
                    ? "bg-gray-700 text-gray-400 cursor-not-allowed"
                    : "text-red-400 hover:text-red-300 hover:bg-red-900/30"
                }`}
                disabled={deleting === value}
                onClick={() => handleDelete(value)}
                title="Delete"
              >
                {deleting === value ? (
                  <svg className="animate-spin h-4 w-4" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4"></circle>
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                  </svg>
                ) : (
                  <svg xmlns="http://www.w3.org/2000/svg" className="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                  </svg>
                )}
              </button>
            </div>
          );
        })}

        {!loading && total === 0 && (
          <div className="text-gray-500 text-sm">No CLI tokens</div>
        )}
      </div>
    </div>
  );
};

export default CliVisualization;

