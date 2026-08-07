import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { Clock3, FolderOpen, Languages, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { api, forgetRecent, readRecents, rememberProject, type RecentProject } from "../ipc";
import { ErrorNotice } from "../components/ErrorNotice";

export function Launcher() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [showCreate, setShowCreate] = useState(false);
  const [sourcePath, setSourcePath] = useState("");
  const [projectPath, setProjectPath] = useState("");
  const [name, setName] = useState("");
  const [namespace, setNamespace] = useState("evo:root");
  const recents = useQuery({ queryKey: ["recents"], queryFn: readRecents });

  const finish = async (summary: Awaited<ReturnType<typeof api.openProject>>) => {
    await rememberProject(summary);
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["project"] }),
      queryClient.invalidateQueries({ queryKey: ["recents"] }),
    ]);
  };
  const opener = useMutation({
    mutationFn: (path: string) => api.openProject(path),
    onSuccess: finish,
  });
  const creator = useMutation({
    mutationFn: () =>
      api.createProject({ path: projectPath, sourcePath: sourcePath || undefined, name: name || undefined, namespace }),
    onSuccess: finish,
  });

  const pickProject = async () => {
    const selected = await open({ directory: true, multiple: false, title: t("launcher.open") });
    if (typeof selected === "string") opener.mutate(selected);
  };
  const pickSource = async () => {
    const selected = await open({
      multiple: false,
      filters: [{ name: "LangCraft Language", extensions: ["lang"] }],
    });
    if (typeof selected === "string") setSourcePath(selected);
  };
  const pickDestination = async () => {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string") setProjectPath(selected);
  };

  const recentOpen = (item: RecentProject) => opener.mutate(item.path);
  const busy = opener.isPending || creator.isPending;
  const error = opener.error ?? creator.error;

  return (
    <main className="launcher">
      <section className="launcher-hero">
        <div className="hero-mark"><Languages aria-hidden="true" /></div>
        <p className="eyebrow">LANGCRAFT / DESKTOP</p>
        <h1>{t("launcher.title")}</h1>
        <p>{t("launcher.subtitle")}</p>
        <div className="launcher-actions">
          <button className="button primary" type="button" onClick={() => setShowCreate(true)}>
            <Plus />{t("launcher.create")}
          </button>
          <button className="button secondary" type="button" onClick={pickProject} disabled={busy}>
            <FolderOpen />{t("launcher.open")}
          </button>
        </div>
        {error && <ErrorNotice error={error} />}
      </section>

      <section className="recent-panel">
        <div className="section-heading"><div><p className="eyebrow">RECENT</p><h2>{t("launcher.recent")}</h2></div><Clock3 /></div>
        {recents.data?.length ? (
          <div className="recent-list">
            {recents.data.map((item) => (
              <div className="recent-row" key={item.path}>
                <button type="button" onClick={() => recentOpen(item)}>
                  <strong>{item.name ?? "Untitled"}</strong><span>{item.path}</span>
                </button>
                <button
                  className="icon-button"
                  type="button"
                  aria-label={t("common.delete")}
                  onClick={async () => {
                    await forgetRecent(item.path);
                    await recents.refetch();
                  }}
                ><Trash2 /></button>
              </div>
            ))}
          </div>
        ) : <p className="empty-state">{t("launcher.empty")}</p>}
      </section>

      {showCreate && (
        <div className="modal-backdrop" role="presentation" onMouseDown={() => setShowCreate(false)}>
          <section className="modal" role="dialog" aria-modal="true" onMouseDown={(event) => event.stopPropagation()}>
            <div><p className="eyebrow">NEW PROJECT</p><h2>{t("launcher.create")}</h2></div>
            <label>{t("launcher.source")}<div className="path-picker"><input value={sourcePath} readOnly placeholder={t("launcher.sourceHint")} /><button type="button" onClick={pickSource}>…</button>{sourcePath && <button type="button" onClick={() => setSourcePath("")}>{t("launcher.sourceClear")}</button>}</div></label>
            <label>{t("launcher.destination")}<div className="path-picker"><input value={projectPath} readOnly /><button type="button" onClick={pickDestination}>…</button></div></label>
            <label>{t("launcher.name")}<input value={name} onChange={(event) => setName(event.target.value)} /></label>
            <label>{t("launcher.namespace")}<input value={namespace} onChange={(event) => setNamespace(event.target.value)} /></label>
            {creator.error && <ErrorNotice error={creator.error} />}
            <div className="modal-actions">
              <button className="button ghost" type="button" onClick={() => setShowCreate(false)}>{t("common.cancel")}</button>
              <button className="button primary" type="button" disabled={!projectPath || !namespace || creator.isPending} onClick={() => creator.mutate()}>{t("common.create")}</button>
            </div>
          </section>
        </div>
      )}
    </main>
  );
}

