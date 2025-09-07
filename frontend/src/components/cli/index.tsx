import React, { useState } from "react";
import TabNavigation from "../common/TabNavigation";
import CliSubmitForm from "./CliSubmitForm";
import CliVisualization from "./CliVisualization";

const CliTabs: React.FC = () => {
  const [activeTab, setActiveTab] = useState<"submit" | "status">("submit");

  const tabs = [
    { id: "submit", label: "Submit", color: "purple" },
    { id: "status", label: "Status", color: "amber" },
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
