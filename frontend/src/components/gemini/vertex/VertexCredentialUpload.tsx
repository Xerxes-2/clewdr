import React, { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "react-hot-toast";
import { VertexServiceAccount } from "../../../types/vertex.types";
import FormInput from "../../common/FormInput";

interface VertexCredentialUploadProps {
  onSubmit: (credential: VertexServiceAccount) => Promise<void>;
}

const VertexCredentialUpload: React.FC<VertexCredentialUploadProps> = ({
  onSubmit,
}) => {
  const { t } = useTranslation();
  const [rawInput, setRawInput] = useState<string>("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [selectedFile, setSelectedFile] = useState<string>("");
  const fileInputRef = useRef<HTMLInputElement | null>(null);

  const handleFile = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) {
      setSelectedFile("");
      return;
    }

    try {
      const text = await file.text();
      setRawInput(text);
      setSelectedFile(file.name);
      toast.success(t("geminiVertex.notifications.fileLoaded"));
    } catch (error) {
      console.error("Failed to read credential file", error);
      toast.error(t("geminiVertex.errors.readFile"));
    }
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!rawInput.trim()) {
      toast.error(t("geminiVertex.errors.empty"));
      return;
    }

    try {
      const parsed: VertexServiceAccount = JSON.parse(rawInput);
      if (!parsed.client_email) {
        toast.error(t("geminiVertex.errors.missingEmail"));
        return;
      }
      setIsSubmitting(true);
      await onSubmit(parsed);
      setRawInput("");
      setSelectedFile("");
      if (fileInputRef.current) {
        fileInputRef.current.value = "";
      }
    } catch (error) {
      console.error("Failed to parse credential JSON", error);
      toast.error(t("geminiVertex.errors.parse"));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="space-y-4">
      <p className="text-sm text-gray-400">
        {t("geminiVertex.form.instructions")}
      </p>

      <FormInput
        id="vertex-service-account"
        name="vertex-service-account"
        value={rawInput}
        onChange={(event) => setRawInput(event.target.value)}
        placeholder={t("geminiVertex.form.placeholder") || ""}
        label={t("geminiVertex.form.jsonLabel")}
        isTextarea
        rows={10}
        disabled={isSubmitting}
      />

      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <div className="flex items-center gap-3">
          <input
            ref={fileInputRef}
            type="file"
            accept="application/json"
            onChange={handleFile}
            className="hidden"
          />
          <button
            type="button"
            className="px-4 py-2 rounded-md text-sm font-medium bg-gray-700 hover:bg-gray-600 text-gray-100 transition-colors"
            onClick={() => fileInputRef.current?.click()}
            disabled={isSubmitting}
          >
            {t("geminiVertex.form.chooseFile")}
          </button>
          <span className="text-sm text-gray-400 truncate max-w-[220px]">
            {selectedFile || t("geminiVertex.form.noFile")}
          </span>
        </div>

        <button
          type="submit"
          disabled={isSubmitting}
          className={`px-4 py-2 rounded-md text-sm font-medium transition-colors ${
            isSubmitting
              ? "bg-indigo-900/50 text-indigo-200 cursor-not-allowed"
              : "bg-indigo-600 hover:bg-indigo-500 text-white"
          }`}
        >
          {isSubmitting
            ? t("geminiVertex.form.submitting")
            : t("geminiVertex.form.button")}
        </button>
      </div>
    </form>
  );
};

export default VertexCredentialUpload;
