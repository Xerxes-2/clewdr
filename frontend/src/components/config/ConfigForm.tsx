// frontend/src/components/config/ConfigForm.tsx
import React from "react";
import { useTranslation } from "react-i18next";
import { ConfigData } from "../../types/config.types";
import ConfigSection from "./ConfigSection";
import ConfigCheckbox from "./ConfigCheckbox";
import FormInput from "../common/FormInput";

interface ConfigFormProps {
  config: ConfigData;
  onChange: (
    e: React.ChangeEvent<
      HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement
    >
  ) => void;
}

const ConfigForm: React.FC<ConfigFormProps> = ({ config, onChange }) => {
  const { t } = useTranslation();

  return (
    <div className="space-y-6">
      {/* Server Settings Section */}
      <ConfigSection
        title={t("config.sections.server.title")}
        description={t("config.sections.server.description")}
      >
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          <FormInput
            id="ip"
            name="ip"
            type="text"
            value={config.ip}
            onChange={onChange}
            label={t("config.sections.server.ip")}
          />

          <FormInput
            id="port"
            name="port"
            type="number"
            value={config.port.toString()}
            onChange={onChange}
            label={t("config.sections.server.port")}
          />
        </div>
      </ConfigSection>

      {/* App Settings Section */}
      <ConfigSection title={t("config.sections.app.title")}>
        <div className="flex space-x-6">
          <ConfigCheckbox
            name="check_update"
            checked={config.check_update}
            onChange={onChange}
            label={t("config.sections.app.checkUpdate")}
          />

          <ConfigCheckbox
            name="auto_update"
            checked={config.auto_update}
            onChange={onChange}
            label={t("config.sections.app.autoUpdate")}
          />
        </div>
      </ConfigSection>

      {/* Network Settings Section */}
      <ConfigSection title={t("config.sections.network.title")}>
        <FormInput
          id="password"
          name="password"
          type="password"
          value={config.password}
          onChange={onChange}
          label={t("config.sections.network.password")}
        />

        <FormInput
          id="admin_password"
          name="admin_password"
          type="password"
          value={config.admin_password}
          onChange={onChange}
          label={t("config.sections.network.adminPassword")}
        />

        <FormInput
          id="proxy"
          name="proxy"
          type="text"
          value={config.proxy || ""}
          onChange={onChange}
          label={t("config.sections.network.proxy")}
          placeholder={t("config.sections.network.proxyPlaceholder")}
        />

        <FormInput
          id="rproxy"
          name="rproxy"
          type="text"
          value={config.rproxy || ""}
          onChange={onChange}
          label={t("config.sections.network.rproxy")}
          placeholder={t("config.sections.network.rproxyPlaceholder")}
        />
      </ConfigSection>

      {/* Vertex Settings Section */}
      <ConfigSection title={t("config.sections.vertex.title")}>
        <p className="text-sm text-gray-400">
          {t("config.sections.vertex.manageInPanel")}
        </p>
      </ConfigSection>

      {/* API Settings Section */}
      <ConfigSection title={t("config.sections.api.title")}>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-3">
          <FormInput
            id="max_retries"
            name="max_retries"
            type="number"
            value={config.max_retries.toString()}
            onChange={onChange}
            label={t("config.sections.api.maxRetries")}
          />
        </div>
        <div className="flex space-x-6">
          <ConfigCheckbox
            name="preserve_chats"
            checked={config.preserve_chats}
            onChange={onChange}
            label={t("config.sections.api.preserveChats")}
          />

          <ConfigCheckbox
            name="web_search"
            checked={config.web_search}
            onChange={onChange}
            label={t("config.sections.api.webSearch")}
          />
        </div>
      </ConfigSection>

      {/* Cookie Settings Section */}
      <ConfigSection title={t("config.sections.cookie.title")}>
        <ConfigCheckbox
          name="skip_non_pro"
          checked={config.skip_non_pro}
          onChange={onChange}
          label={t("config.sections.cookie.skipNonPro")}
        />
        <ConfigCheckbox
          name="skip_restricted"
          checked={config.skip_restricted}
          onChange={onChange}
          label={t("config.sections.cookie.skipRestricted")}
        />

        <ConfigCheckbox
          name="skip_second_warning"
          checked={config.skip_second_warning}
          onChange={onChange}
          label={t("config.sections.cookie.skipSecondWarning")}
        />

        <ConfigCheckbox
          name="skip_first_warning"
          checked={config.skip_first_warning}
          onChange={onChange}
          label={t("config.sections.cookie.skipFirstWarning")}
        />

        <ConfigCheckbox
          name="skip_normal_pro"
          checked={config.skip_normal_pro}
          onChange={onChange}
          label={t("config.sections.cookie.skipNormalPro")}
        />

        <ConfigCheckbox
          name="skip_rate_limit"
          checked={config.skip_rate_limit}
          onChange={onChange}
          label={t("config.sections.cookie.skipRateLimit")}
        />
      </ConfigSection>

      {/* Prompt Configurations Section */}
      <ConfigSection title={t("config.sections.prompt.title")}>
        <ConfigCheckbox
          name="use_real_roles"
          checked={config.use_real_roles}
          onChange={onChange}
          label={t("config.sections.prompt.realRoles")}
        />

        <FormInput
          id="custom_h"
          name="custom_h"
          type="text"
          value={config.custom_h || ""}
          onChange={onChange}
          label={t("config.sections.prompt.customH")}
        />

        <FormInput
          id="custom_a"
          name="custom_a"
          type="text"
          value={config.custom_a || ""}
          onChange={onChange}
          label={t("config.sections.prompt.customA")}
        />

        <FormInput
          id="custom_prompt"
          name="custom_prompt"
          value={config.custom_prompt}
          onChange={onChange}
          label={t("config.sections.prompt.customPrompt")}
          isTextarea={true}
          rows={3}
        />
      </ConfigSection>
    </div>
  );
};

export default ConfigForm;
