import React, { useState } from "react";
import Button from "../common/Button";
import FormInput from "../common/FormInput";
import StatusMessage from "../common/StatusMessage";
import { postCliToken, postCliTokenJson } from "../../api";

interface CliResult {
  item: string;
  status: "success" | "error";
  message: string;
}

const CliSubmitForm: React.FC = () => {
  const [input, setInput] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [results, setResults] = useState<CliResult[]>([]);
  const [overallStatus, setOverallStatus] = useState({
    type: "info" as "info" | "success" | "error" | "warning",
    message: "",
  });

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();

    const trimmedAll = input.trim();

    if (trimmedAll.length === 0) {
      setOverallStatus({ type: "error", message: "Please enter at least one item" });
      return;
    }

    setIsSubmitting(true);
    setOverallStatus({ type: "info", message: "" });
    setResults([]);

    const newResults: CliResult[] = [];
    let successCount = 0;
    let errorCount = 0;

    // If entire input looks like a JSON blob, submit once as JSON
    if (trimmedAll.startsWith("{") && trimmedAll.endsWith("}")) {
      try {
        JSON.parse(trimmedAll);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        setResults([{ item: trimmedAll, status: "error", message: `Invalid JSON: ${msg}` }]);
        setOverallStatus({ type: "error", message: "Invalid JSON" });
        setIsSubmitting(false);
        return;
      }
      try {
        await postCliTokenJson(trimmedAll);
        newResults.push({ item: "<JSON>", status: "success", message: "Submitted" });
        successCount = 1;
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        newResults.push({ item: "<JSON>", status: "error", message: msg });
        errorCount = 1;
      }
    } else {
      const items = trimmedAll
        .split("\n")
        .map((line) => line.trim())
        .filter((line) => line.length > 0);

      for (const raw of items) {
        try {
          // Decide how to post: JSON or raw token
          if (raw.startsWith("{") && raw.endsWith("}")) {
            try { JSON.parse(raw); } catch { throw new Error("Invalid JSON"); }
            await postCliTokenJson(raw);
          } else {
            const tokenOnly = raw.startsWith("Bearer ") ? raw.slice(7) : raw;
            await postCliToken(tokenOnly);
          }
          newResults.push({ item: raw, status: "success", message: "Submitted" });
          successCount++;
        } catch (e) {
          const msg = e instanceof Error ? e.message : String(e);
          newResults.push({ item: raw, status: "error", message: msg });
          errorCount++;
        }
      }
    }

    setResults(newResults);

    if (errorCount === 0) {
      setOverallStatus({ type: "success", message: `All ${successCount} submitted successfully` });
      setInput("");
    } else if (successCount === 0) {
      setOverallStatus({ type: "error", message: `All ${errorCount} failed to submit` });
    } else {
      setOverallStatus({
        type: "warning",
        message: `${successCount} of ${successCount + errorCount} submitted successfully (${errorCount} failed)`,
      });
    }

    setIsSubmitting(false);
  };

  return (
    <div>
      <form onSubmit={handleSubmit} className="space-y-6">
        <FormInput
          id="cli"
          name="cli"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder={"Paste tokens or JSON (one per line)"}
          label={"CLI Credentials"}
          isTextarea={true}
          rows={5}
          onClear={() => setInput("")}
          disabled={isSubmitting}
        />

        <p className="text-xs text-gray-400 mt-1">
          {"Accepts raw access tokens (ya29...) or full OAuth JSON (token/access_token present)."}
        </p>

        {overallStatus.message && (
          <StatusMessage type={overallStatus.type} message={overallStatus.message} />
        )}

        {results.length > 0 && (
          <div className="mt-4 bg-gray-800 rounded-md p-3 max-h-60 overflow-y-auto">
            <h4 className="text-sm font-medium text-gray-300 mb-2">Submission details:</h4>
            <div className="space-y-2">
              {results.map((r, i) => (
                <div
                  key={i}
                  className={`text-xs p-2 rounded ${
                    r.status === "success"
                      ? "bg-green-900/30 border border-green-800"
                      : "bg-red-900/30 border border-red-800"
                  }`}
                >
                  <div className="flex items-start">
                    <div
                      className={`mr-2 ${
                        r.status === "success" ? "text-green-400" : "text-red-400"
                      }`}
                    >
                      {r.status === "success" ? "✓" : "✗"}
                    </div>
                    <div className="flex-1">
                      <div className="font-mono text-gray-400 truncate w-full">
                        {r.item.substring(0, 30)}
                        {r.item.length > 30 ? "..." : ""}
                      </div>
                      <div
                        className={`mt-1 ${
                          r.status === "success" ? "text-green-400" : "text-red-400"
                        }`}
                      >
                        {r.message}
                      </div>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        <Button type="submit" disabled={isSubmitting} isLoading={isSubmitting} className="w-full">
          {isSubmitting ? "Submitting..." : "Submit"}
        </Button>
      </form>
    </div>
  );
};

export default CliSubmitForm;
