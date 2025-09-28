import React, { useState } from "react";
import { useTranslation } from "react-i18next";

interface CookieValueProps {
  cookie: string;
  // Removed the unused cookieId prop
}

const CookieValue: React.FC<CookieValueProps> = ({ cookie }) => {
  const { t } = useTranslation();
  const [isExpanded, setIsExpanded] = useState(false);

  if (!cookie) return null;

  // Clean cookie value for display
  const cleanCookie = cookie.replace(/sessionKey=sk-ant-sid01-/, "");
  const displayText = isExpanded
    ? cleanCookie
    : `${cleanCookie.substring(0, 30)}${cleanCookie.length > 30 ? "..." : ""}`;

  const copyToClipboard = (text: string, event: React.MouseEvent) => {
    event.stopPropagation();
    navigator.clipboard
      .writeText(text)
      .then(() => console.log("Copied to clipboard"))
      .catch((err) => console.error("Failed to copy: ", err));
  };

  return (
    <div className="flex flex-wrap items-center">
      <div
        className="flex items-center cursor-pointer flex-1 mr-2 min-w-0"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <code
          className={`font-mono ${
            isExpanded ? "break-all" : "truncate"
          } w-full`}
        >
          {displayText}
        </code>
        {cleanCookie.length > 30 && (
          <span className="ml-2 text-gray-500 flex-shrink-0">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              className="h-4 w-4"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d={isExpanded ? "M5 15l7-7 7 7" : "M19 9l-7 7-7-7"}
              />
            </svg>
          </span>
        )}
      </div>
      <button
        onClick={(e) => copyToClipboard(cleanCookie, e)}
        className="p-1 bg-gray-700 hover:bg-gray-600 rounded text-xs text-gray-300 focus:outline-none flex-shrink-0"
        title={t("cookieStatus.copy")}
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          className="h-3 w-3"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M8 5H6a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2v-1M8 5a2 2 0 002 2h2a2 2 0 002-2M8 5a2 2 0 012-2h2a2 2 0 012 2m0 0h2a2 2 0 012 2v3m2 4H10m0 0l3-3m-3 3l3 3"
          />
        </svg>
      </button>
    </div>
  );
};

export default CookieValue;
