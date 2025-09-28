import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import TabNavigation from "../common/TabNavigation";
import AiStudioTabs from "./AiStudioTabs";
import VertexTabs from "./vertex/VertexTabs";

const GeminiTabs: React.FC = () => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<"aistudio" | "vertex">("aistudio");

  const tabs = [
    { id: "aistudio", label: t("geminiTabs.aistudio"), color: "purple" },
    { id: "vertex", label: t("geminiTabs.vertex"), color: "cyan" },
  ];

  return (
    <div className="w-full">
      <TabNavigation
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={(tabId) => setActiveTab(tabId as "aistudio" | "vertex")}
        className="mb-6"
      />

      {activeTab === "aistudio" ? <AiStudioTabs /> : <VertexTabs />}
    </div>
  );
};

export default GeminiTabs;
