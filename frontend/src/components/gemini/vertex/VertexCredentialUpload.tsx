import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "react-hot-toast";
import { VertexServiceAccount } from "../../../types/vertex.types";

interface VertexCredentialUploadProps {
  onSubmit: (credential: VertexServiceAccount) => Promise<void>;
}

const VertexCredentialUpload: React.FC<VertexCredentialUploadProps> = ({
  onSubmit,
}) => {
  const { t } = useTranslation();
  const [rawInput, setRawInput] = useState<string>("");
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleFile = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) {
      return;
    }

    try {
      const text = await file.text();
      setRawInput(text);
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

      <textarea
        className="w-full h-52 rounded-md bg-gray-900/60 border border-gray-700 p-3 text-sm focus:outline-none focus:ring-2 focus:ring-indigo-500"
        placeholder={t("geminiVertex.form.placeholder") || ""}
        value={rawInput}
        onChange={(event) => setRawInput(event.target.value)}
      />

      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
        <label className="block">
          <span className="text-sm text-gray-400">
            {t("geminiVertex.form.loadFromFile")}
          </span>
          <input
            type="file"
            accept="application/json"
            onChange={handleFile}
            className="mt-1 text-sm text-gray-300"
          />
        </label>

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
