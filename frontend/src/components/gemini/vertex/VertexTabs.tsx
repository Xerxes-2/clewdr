import React, { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "react-hot-toast";
import TabNavigation from "../../common/TabNavigation";
import {
  addVertexCredential,
  deleteVertexCredential,
  getVertexCredentials,
} from "../../../api/vertexApi";
import {
  VertexCredentialInfo,
  VertexServiceAccount,
} from "../../../types/vertex.types";
import VertexCredentialUpload from "./VertexCredentialUpload";
import VertexCredentialList from "./VertexCredentialList";

const VertexTabs: React.FC = () => {
  const { t } = useTranslation();
  const [credentials, setCredentials] = useState<VertexCredentialInfo[]>([]);
  const [activeTab, setActiveTab] = useState<"upload" | "status">("upload");
  const [loading, setLoading] = useState(false);
  const [deletingEmail, setDeletingEmail] = useState<string | null>(null);

  const loadCredentials = useCallback(async () => {
    setLoading(true);
    try {
      const data = await getVertexCredentials();
      setCredentials(data);
    } catch (error) {
      console.error("Failed to load vertex credentials", error);
      toast.error(t("geminiVertex.errors.load"));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    loadCredentials();
  }, [loadCredentials]);

  const handleUpload = useCallback(
    async (credential: VertexServiceAccount) => {
      try {
        await addVertexCredential(credential);
        toast.success(t("geminiVertex.notifications.uploaded"));
        await loadCredentials();
      } catch (error) {
        console.error("Failed to upload vertex credential", error);
        toast.error(t("geminiVertex.errors.upload"));
      }
    },
    [loadCredentials, t],
  );

  const handleDelete = useCallback(
    async (email: string) => {
      setDeletingEmail(email);
      try {
        await deleteVertexCredential(email);
        toast.success(t("geminiVertex.notifications.deleted"));
        await loadCredentials();
      } catch (error) {
        console.error("Failed to delete vertex credential", error);
        toast.error(t("geminiVertex.errors.delete"));
      } finally {
        setDeletingEmail(null);
      }
    },
    [loadCredentials, t],
  );

  const tabs = useMemo(
    () => [
      { id: "upload", label: t("geminiVertex.tabUpload"), color: "indigo" },
      { id: "status", label: t("geminiVertex.tabStatus"), color: "cyan" },
    ],
    [t],
  );

  return (
    <div className="w-full">
      <TabNavigation
        tabs={tabs}
        activeTab={activeTab}
        onTabChange={(tabId) => setActiveTab(tabId as "upload" | "status")}
        className="mb-6"
      />

      {activeTab === "upload" ? (
        <VertexCredentialUpload onSubmit={handleUpload} />
      ) : (
        <VertexCredentialList
          loading={loading}
          credentials={credentials}
          onDelete={handleDelete}
          deletingEmail={deletingEmail}
        />
      )}
    </div>
  );
};

export default VertexTabs;
