import React from "react";
import { useTranslation } from "react-i18next";
import { VertexCredentialStatus } from "../../../types/vertex.types";

interface VertexCredentialValueProps {
  credential: VertexCredentialStatus;
}

const VertexCredentialValue: React.FC<VertexCredentialValueProps> = ({ credential }) => {
  const { t } = useTranslation();

  const email = credential.client_email?.trim();
  const projectId = credential.project_id?.trim();

  return (
    <div className="min-w-0">
      <p className="text-white font-semibold truncate">
        {email || t("geminiTab.vertexStatus.item.label")}
      </p>
      <p className="text-xs text-gray-400 mt-1 truncate">
        {projectId || t("geminiTab.vertexStatus.item.caption")}
      </p>
    </div>
  );
};

export default VertexCredentialValue;
