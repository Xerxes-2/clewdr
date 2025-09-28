import React from "react";
import { useTranslation } from "react-i18next";

interface CookieSectionProps {
  title: string;
  cookies: any[];
  color: string;
  renderStatus: (item: any, index: number) => React.ReactNode;
}

const CookieSection: React.FC<CookieSectionProps> = ({
  title,
  cookies,
  color,
  renderStatus,
}) => {
  const { t } = useTranslation();
  // sort cookie base on reset_time
  cookies.sort((a, b) => {
    const aTime = a.reset_time ? new Date(a.reset_time).getTime() : 0;
    const bTime = b.reset_time ? new Date(b.reset_time).getTime() : 0;
    return aTime - bTime;
  });

  return (
    <div className={`rounded-lg bg-gray-800 overflow-hidden w-full shadow-md`}>
      <div
        className={`bg-${color}-900 px-4 py-2 flex justify-between items-center border-b border-${color}-700`}
      >
        <h4 className={`font-medium text-${color}-100`}>{title}</h4>
        <span
          className={`bg-${color}-800 text-${color}-100 text-xs px-2 py-1 rounded-full`}
        >
          {cookies.length}
        </span>
      </div>
      {cookies.length > 0 ? (
        <div className="p-4 divide-y divide-gray-700">
          {cookies.map((item, index) => renderStatus(item, index))}
        </div>
      ) : (
        <div className="p-4 text-sm text-gray-400 italic">
          {t("cookieStatus.noCookies", { type: title.toLowerCase() })}
        </div>
      )}
    </div>
  );
};

export default CookieSection;
