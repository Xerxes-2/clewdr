import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import TabNavigation from "../common/TabNavigation";
import CliSubmitForm from "./CliSubmitForm";
import CliVisualization from "./CliVisualization";

const CliTabs: React.FC = () => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<"submit" | "status">("submit");

  const tabs = [
    { id: "submit", label: t("cli.tab.submit"), color: "purple" },
    { id: "status", label: t("cli.tab.status"), color: "amber" },
  ];

  return (
    <div className="w-full">
      <TabNavigation
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={(tabId) => setActiveTab(tabId as "submit" | "status")}
        className="mb-6"
      />

      {activeTab === "submit" ? <CliSubmitForm /> : <CliVisualization />}
    </div>
  );
};

export default CliTabs;
