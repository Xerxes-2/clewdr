import React, { useState } from "react";
import { useTranslation } from "react-i18next";

interface KeyValueProps {
  keyString: string;
}

const KeyValue: React.FC<KeyValueProps> = ({ keyString }) => {
  const { t } = useTranslation();
  const [isRevealed, setIsRevealed] = useState(false);
  const [isExpanded, setIsExpanded] = useState(false);

  // Function to reveal/hide the full key
  const toggleReveal = (event: React.MouseEvent) => {
    event.stopPropagation();
    setIsRevealed(!isRevealed);
  };

  // Function to mask part of the key for security
  const getMaskedKey = () => {
    if (keyString.length <= 14) {
      return "****";
    }

    const prefix = keyString.substring(0, 10);
    const suffix = keyString.substring(keyString.length - 4);
    const maskedMiddle = "****";

    return `${prefix}${maskedMiddle}${suffix}`;
  };

  // Function to copy key to clipboard
  const copyToClipboard = (event: React.MouseEvent) => {
    event.stopPropagation();
    navigator.clipboard
      .writeText(keyString)
      .then(() => console.log("Copied to clipboard"))
      .catch((err) => console.error("Failed to copy: ", err));
  };

  const displayText = isRevealed
    ? keyString
    : getMaskedKey();
  
  const displayValue = isExpanded
    ? displayText
    : `${displayText.substring(0, 30)}${displayText.length > 30 ? "..." : ""}`;

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
          {displayValue}
        </code>
        {displayText.length > 30 && (
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
      <div className="flex items-center">
        <button
          onClick={toggleReveal}
          className="p-1 bg-gray-700 hover:bg-gray-600 rounded text-xs text-gray-300 focus:outline-none flex-shrink-0 mr-2"
          title={isRevealed ? t("common.hide") : t("common.reveal")}
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
              d={isRevealed ? 
                "M13.875 18.825A10.05 10.05 0 0112 19c-4.478 0-8.268-2.943-9.543-7a9.97 9.97 0 011.563-3.029m5.858.908a3 3 0 114.243 4.243M9.878 9.878l4.242 4.242M9.88 9.88l-3.29-3.29m7.532 7.532l3.29 3.29M3 3l3.59 3.59m0 0A9.953 9.953 0 0112 5c4.478 0 8.268 2.943 9.543 7a10.025 10.025 0 01-4.132 5.411m0 0L21 21" : 
                "M15 12a3 3 0 11-6 0 3 3 0 016 0z M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z"
              }
            />
          </svg>
        </button>
        <button
          onClick={copyToClipboard}
          className="p-1 bg-gray-700 hover:bg-gray-600 rounded text-xs text-gray-300 focus:outline-none flex-shrink-0"
          title={t("common.copy")}
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
    </div>
  );
};

export default KeyValue;
