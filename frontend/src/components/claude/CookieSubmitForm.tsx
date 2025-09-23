import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { postCookie } from "../../api";
import Button from "../common/Button";
import FormInput from "../common/FormInput";
import StatusMessage from "../common/StatusMessage";

interface CookieResult {
  cookie: string;
  status: "success" | "error";
  message: string;
}

const CookieSubmitForm: React.FC = () => {
  const { t } = useTranslation();
  const [cookies, setCookies] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [results, setResults] = useState<CookieResult[]>([]);
  const [overallStatus, setOverallStatus] = useState({
    type: "info" as "info" | "success" | "error" | "warning",
    message: "",
  });

  const handleSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();

    const cookieLines = cookies
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.length > 0);

    if (cookieLines.length === 0) {
      setOverallStatus({
        type: "error",
        message: t("cookieSubmit.error.empty"),
      });
      return;
    }

    setIsSubmitting(true);
    setOverallStatus({ type: "info", message: "" });
    setResults([]);

    const newResults: CookieResult[] = [];
    let successCount = 0;
    let errorCount = 0;

    // Process each cookie line
    for (const cookieStr of cookieLines) {
      try {
        await postCookie(cookieStr);
        newResults.push({
          cookie: cookieStr,
          status: "success",
          message: t("cookieSubmit.success"),
        });
        successCount++;
      } catch (e) {
        // Handle error for this specific cookie
        const errorMessage = e instanceof Error ? e.message : "Unknown error";
        let translatedError = errorMessage;

        if (errorMessage.includes("Invalid cookie format")) {
          translatedError = t("cookieSubmit.error.format");
        } else if (errorMessage.includes("Authentication failed")) {
          translatedError = t("cookieSubmit.error.auth");
        } else if (errorMessage.includes("Server error")) {
          translatedError = t("cookieSubmit.error.server");
        } else if (errorMessage.includes("Database storage is unavailable")) {
          translatedError = t("cookieSubmit.error.storage");
        }

        newResults.push({
          cookie: cookieStr,
          status: "error",
          message: translatedError,
        });
        errorCount++;
      }
    }

    setResults(newResults);

    // Set overall status message
    if (errorCount === 0) {
      setOverallStatus({
        type: "success",
        message: t("cookieSubmit.allSuccess", { count: successCount }),
      });
      // Don't clear the input field if there were any errors
      if (errorCount === 0) {
        setCookies("");
      }
    } else if (successCount === 0) {
      setOverallStatus({
        type: "error",
        message: t("cookieSubmit.allFailed", { count: errorCount }),
      });
    } else {
      setOverallStatus({
        type: "warning",
        message: t("cookieSubmit.partialSuccess", {
          successCount,
          errorCount,
          total: successCount + errorCount,
        }),
      });
    }

    setIsSubmitting(false);
  };

  return (
    <div>
      <form onSubmit={handleSubmit} className="space-y-6">
        <FormInput
          id="cookie"
          name="cookie"
          value={cookies}
          onChange={(e) => setCookies(e.target.value)}
          placeholder={t("cookieSubmit.placeholderMulti")}
          label={t("cookieSubmit.value")}
          isTextarea={true}
          rows={5}
          onClear={() => setCookies("")}
          disabled={isSubmitting}
        />

        <p className="text-xs text-gray-400 mt-1">
          {t("cookieSubmit.descriptionMulti")}
        </p>

        {overallStatus.message && (
          <StatusMessage
            type={overallStatus.type}
            message={overallStatus.message}
          />
        )}

        {/* Results listing */}
        {results.length > 0 && (
          <div className="mt-4 bg-gray-800 rounded-md p-3 max-h-60 overflow-y-auto">
            <h4 className="text-sm font-medium text-gray-300 mb-2">
              {t("cookieSubmit.resultDetails")}:
            </h4>
            <div className="space-y-2">
              {results.map((result, index) => (
                <div
                  key={index}
                  className={`text-xs p-2 rounded ${
                    result.status === "success"
                      ? "bg-green-900/30 border border-green-800"
                      : "bg-red-900/30 border border-red-800"
                  }`}
                >
                  <div className="flex items-start">
                    <div
                      className={`mr-2 ${
                        result.status === "success"
                          ? "text-green-400"
                          : "text-red-400"
                      }`}
                    >
                      {result.status === "success" ? "✓" : "✗"}
                    </div>
                    <div className="flex-1">
                      <div className="font-mono text-gray-400 truncate w-full">
                        {result.cookie.substring(0, 30)}
                        {result.cookie.length > 30 ? "..." : ""}
                      </div>
                      <div
                        className={`mt-1 ${
                          result.status === "success"
                            ? "text-green-400"
                            : "text-red-400"
                        }`}
                      >
                        {result.message}
                      </div>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        <Button
          type="submit"
          disabled={isSubmitting}
          isLoading={isSubmitting}
          className="w-full"
        >
          {isSubmitting
            ? t("cookieSubmit.submitting")
            : t("cookieSubmit.submitButton")}
        </Button>
      </form>
    </div>
  );
};

export default CookieSubmitForm;
