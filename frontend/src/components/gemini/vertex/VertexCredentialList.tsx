import React from "react";
import { useTranslation } from "react-i18next";
import DeleteButton from "../DeleteButton";
import { VertexCredentialInfo } from "../../../types/vertex.types";

interface VertexCredentialListProps {
  loading: boolean;
  credentials: VertexCredentialInfo[];
  deletingEmail: string | null;
  onDelete: (clientEmail: string) => Promise<void>;
}

const VertexCredentialList: React.FC<VertexCredentialListProps> = ({
  loading,
  credentials,
  deletingEmail,
  onDelete,
}) => {
  const { t } = useTranslation();

  if (loading) {
    return <p className="text-sm text-gray-400">{t("geminiVertex.status.loading")}</p>;
  }

  if (!credentials.length) {
    return <p className="text-sm text-gray-400">{t("geminiVertex.status.empty")}</p>;
  }

  return (
    <div className="space-y-3">
      {credentials.map((cred) => (
        <div
          key={cred.client_email}
          className="flex items-center justify-between rounded-md border border-gray-700/70 bg-gray-900/40 px-3 py-2"
        >
          <div>
            <p className="text-sm font-medium text-gray-200">
              {cred.client_email}
            </p>
            <p className="text-xs text-gray-400">
              {t("geminiVertex.status.project", {
                project: cred.project_id || t("geminiVertex.status.unknown"),
              })}
            </p>
            {cred.private_key_id && (
              <p className="text-xs text-gray-500">
                {t("geminiVertex.status.keyId", { id: cred.private_key_id })}
              </p>
            )}
          </div>
          <DeleteButton
            keyString={cred.client_email}
            onDelete={onDelete}
            isDeleting={deletingEmail === cred.client_email}
          />
        </div>
      ))}
    </div>
  );
};

export default VertexCredentialList;
