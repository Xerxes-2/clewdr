import React, { useEffect, useState } from "react";
import Button from "../common/Button";
import FormInput from "../common/FormInput";
import StatusMessage from "../common/StatusMessage";
import { postCliToken, postCliTokenJson, getCliTokens, deleteCliToken } from "../../api";

const CliGuide: React.FC = () => {
  const [newToken, setNewToken] = useState("");
  const [status, setStatus] = useState<string>("");
  const [jsonText, setJsonText] = useState<string>("");
  const [list, setList] = useState<string[]>([]);

  const refresh = async () => {
    try {
      const data = await getCliTokens();
      const items: string[] = (data?.valid || []).map((v: any) => v.token?.inner || v.token);
      setList(items);
    } catch {
      setList([]);
    }
  };

  useEffect(() => { refresh(); }, []);

  const onSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setStatus("");
    const trimmed = newToken.trim();
    let res;
    if (trimmed.startsWith("{") && trimmed.endsWith("}")) {
      try {
        JSON.parse(trimmed);
        res = await postCliTokenJson(trimmed);
      } catch {
        const tokenOnly = trimmed.startsWith("Bearer ") ? trimmed.slice(7) : trimmed;
        if (!tokenOnly) return;
        res = await postCliToken(tokenOnly);
      }
    } else {
      const tokenOnly = trimmed.startsWith("Bearer ") ? trimmed.slice(7) : trimmed;
      if (!tokenOnly) return;
      res = await postCliToken(tokenOnly);
    }
    if (res.ok) {
      setStatus("已提交凭据");
      setNewToken("");
      refresh();
    } else {
      setStatus(`提交失败: ${res.status}`);
    }
  };

  const onDelete = async (tok: string) => {
    await deleteCliToken(tok);
    refresh();
  };

  return (
    <div className="space-y-6 text-sm">
      <div className="border-t border-gray-800 pt-4">
        <h3 className="font-semibold mb-2">提交凭据（管理员）</h3>
        <form onSubmit={onSubmit} className="space-y-3">
          <FormInput
            id="cliToken"
            name="cliToken"
            type="password"
            value={newToken}
            onChange={(e) => setNewToken(e.target.value)}
            label="凭据（ya29... 或粘贴 JSON）"
            placeholder='ya29... / {"access_token":"ya29..."}'
            onClear={() => setNewToken("")}
          />
          <Button type="submit" variant="primary" className="w-full">提交</Button>
        </form>
        {status && <StatusMessage type="info" message={status} />}

        <div className="mt-6">
          <h4 className="font-semibold mb-2">粘贴 JSON（自动提取 token/access_token）</h4>
          <textarea
            className="w-full bg-gray-900 rounded p-3 text-xs font-mono"
            rows={6}
            placeholder='{"client_id":"...","token":"ya29...","refresh_token":"..."}'
            value={jsonText}
            onChange={(e) => setJsonText(e.target.value)}
          />
          <div className="mt-2">
            <Button
              type="button"
              variant="secondary"
              className="w-full"
              onClick={async () => {
                setStatus("");
                try {
                  JSON.parse(jsonText);
                } catch {
                  setStatus("JSON 无效");
                  return;
                }
                const res = await postCliTokenJson(jsonText);
                if (res.ok) {
                  setStatus("已提交凭据");
                  setJsonText("");
                  refresh();
                } else {
                  setStatus(`提交失败: ${res.status}`);
                }
              }}
            >
              提交
            </Button>
          </div>
        </div>

        <div className="mt-4">
          <h4 className="font-semibold mb-2">已保存</h4>
          <ul className="space-y-1">
            {list.map((t) => (
              <li key={t} className="flex items-center justify-between">
                <code className="bg-gray-900 px-2 py-1 rounded text-xs">{t.slice(0, 10)}...</code>
                <button className="text-red-400 text-xs" onClick={() => onDelete(t)}>删除</button>
              </li>
            ))}
            {list.length === 0 && <li className="text-gray-500">暂无</li>}
          </ul>
        </div>
      </div>
    </div>
  );
};

export default CliGuide;
