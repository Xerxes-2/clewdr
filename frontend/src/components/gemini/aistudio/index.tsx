// frontend/src/components/gemini/aistudio/index.tsx
import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import TabNavigation from "../../common/TabNavigation";
import KeySubmitForm from "./KeySubmitForm";
import KeyVisualization from "./KeyVisualization";

const AiStudioTabs: React.FC = () => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<"submit" | "status">("submit");

  const tabs = [
    { id: "submit", label: t("keyTab.submit"), color: "purple" },
    { id: "status", label: t("keyTab.status"), color: "violet" },
  ];

  return (
    <div className="w-full">
      <TabNavigation
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={(tabId) => setActiveTab(tabId as "submit" | "status")}
        className="mb-6"
      />

      {activeTab === "submit" ? <KeySubmitForm /> : <KeyVisualization />}
    </div>
  );
};

export default AiStudioTabs;
