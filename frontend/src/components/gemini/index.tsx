import React, { useState } from "react";
import { useTranslation } from "react-i18next";

import StatusMessage from "../common/StatusMessage";
import TabNavigation from "../common/TabNavigation";
import FormInput from "../common/FormInput";
import Button from "../common/Button";
import AiStudioTabs from "./aistudio";
import VertexStatus from "./vertex/VertexStatus";
import { postVertexCredential } from "../../api/vertexApi";

const GeminiTab: React.FC = () => {
  const { t } = useTranslation();
  const [activeTab, setActiveTab] = useState<"aistudio" | "vertex">("aistudio");
  const [vertexTab, setVertexTab] = useState<"submit" | "status">("submit");
  const [credential, setCredential] = useState("");
  const [submitFeedback, setSubmitFeedback] = useState<{
    type: "success" | "error" | "warning" | "info";
    message: string;
  }>({ type: "info", message: "" });
  const [submitting, setSubmitting] = useState(false);
  const [vertexRefreshToken, setVertexRefreshToken] = useState(0);

  const tabs = [
    { id: "aistudio", label: t("geminiTab.tabs.aistudio"), color: "purple" },
    { id: "vertex", label: t("geminiTab.tabs.vertex"), color: "green" },
  ];

  const vertexTabs = [
    { id: "submit", label: t("geminiTab.vertexTabs.submit"), color: "purple" },
    { id: "status", label: t("geminiTab.vertexTabs.status"), color: "amber" },
  ];

  const handleCredentialSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const raw = credential.trim();
    if (!raw) {
      setSubmitFeedback({
        type: "error",
        message: t("geminiTab.vertexSubmit.error.empty"),
      });
      return;
    }
    setSubmitting(true);
    setSubmitFeedback({ type: "info", message: "" });
    try {
      await postVertexCredential(raw);
      setCredential("");
      setSubmitFeedback({
        type: "success",
        message: t("geminiTab.vertexSubmit.success"),
      });
      setVertexRefreshToken((prev) => prev + 1);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setSubmitFeedback({
        type: "error",
        message: t("geminiTab.vertexSubmit.error.general", { message }),
      });
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="w-full">
      <TabNavigation
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={(tabId) => setActiveTab(tabId as "aistudio" | "vertex")}
        className="mb-6"
      />

      {activeTab === "aistudio" ? (
        <AiStudioTabs />
      ) : (
        <div className="space-y-6">
          <TabNavigation
            tabs={vertexTabs}
            activeTab={vertexTab}
            onTabChange={(tabId) => setVertexTab(tabId as "submit" | "status")}
          />

          {vertexTab === "submit" ? (
            <form onSubmit={handleCredentialSubmit} className="space-y-4">
              <FormInput
                id="vertex-credential"
                name="vertex-credential"
                label={t("geminiTab.vertexSubmit.label")}
                value={credential}
                onChange={(event) => setCredential(event.target.value)}
                isTextarea
                rows={10}
                placeholder={t("geminiTab.vertexSubmit.placeholder")}
              />

              {submitFeedback.message && (
                <StatusMessage type={submitFeedback.type} message={submitFeedback.message} />
              )}

              <Button
                type="submit"
                isLoading={submitting}
                disabled={submitting}
                className="w-full"
              >
                {t("geminiTab.vertexSubmit.submit")}
              </Button>
            </form>
          ) : (
            <VertexStatus refreshToken={vertexRefreshToken} />
          )}
        </div>
      )}
    </div>
  );
};

export default GeminiTab;
