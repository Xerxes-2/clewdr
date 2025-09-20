import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import TabNavigation from "../common/TabNavigation";
import CookieSubmitForm from "./CookieSubmitForm";
import CookieVisualization from "./CookieVisualization";

const ClaudeTabs: React.FC = () => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<"submit" | "status">("submit");

  const tabs = [
    { id: "submit", label: t("cookieTab.submit"), color: "blue" },
    { id: "status", label: t("cookieTab.status"), color: "amber" },
  ];

  return (
    <div className="w-full">
      <TabNavigation
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={(tabId) => setActiveTab(tabId as "submit" | "status")}
        className="mb-6"
      />

      {activeTab === "submit" ? <CookieSubmitForm /> : <CookieVisualization />}
    </div>
  );
};

export default ClaudeTabs;
