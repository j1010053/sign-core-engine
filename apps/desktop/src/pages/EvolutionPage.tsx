import CodeMirror from "@uiw/react-codemirror";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { open, save } from "@tauri-apps/plugin-dialog";
import { ArrowLeft, ArrowRight, CheckCircle2, FileDown, FileUp, GitCommitHorizontal, RotateCcw, Save, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { DiffSummary } from "../components/DiffSummary";
import { ErrorNotice } from "../components/ErrorNotice";
import { api } from "../ipc";

export function EvolutionPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const project = useQuery({ queryKey: ["project"], queryFn: api.summary });
  const pending = useQuery({
    queryKey: ["pending"],
    queryFn: api.pendingChange,
    enabled: Boolean(project.data?.has_pending),
    retry: false,
  });
  const [draft, setDraft] = useState("");
  const [syncedSource, setSyncedSource] = useState("");
  const [rule, setRule] = useState("t => k");
  const [home, setHome] = useState("Core");
  const [label, setLabel] = useState("");
  useEffect(() => {
    if (pending.data?.source && pending.data.source !== syncedSource) {
      setDraft(pending.data.source);
      setSyncedSource(pending.data.source);
    }
  }, [pending.data?.source, syncedSource]);
  const localDirty = draft !== syncedSource;

  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["pending"] }),
      queryClient.invalidateQueries({ queryKey: ["project"] }),
      queryClient.invalidateQueries({ queryKey: ["tree"] }),
      queryClient.invalidateQueries({ queryKey: ["detail"] }),
      queryClient.invalidateQueries({ queryKey: ["lexicon"] }),
    ]);
  };
  const begin = useMutation({
    mutationFn: () => api.beginEdit(`ui:edit:${project.data?.node_count ?? 0}`),
    onSuccess: async (data) => { setDraft(data.source); setSyncedSource(data.source); await refresh(); },
  });
  const sync = useMutation({
    mutationFn: () => api.replacePendingSource(draft),
    onSuccess: async (data) => { setDraft(data.source); setSyncedSource(data.source); await refresh(); },
  });
  const stage = useMutation({ mutationFn: () => api.stageSoundChange(rule, home), onSuccess: refresh });
  const discard = useMutation({ mutationFn: api.discardLastEdit, onSuccess: refresh });
  const commit = useMutation({ mutationFn: () => api.commit(label || undefined), onSuccess: async () => { setDraft(""); setSyncedSource(""); setLabel(""); await refresh(); } });
  const persist = useMutation({ mutationFn: api.saveProject, onSuccess: refresh });
  const navigate = useMutation({ mutationFn: (direction: "undo" | "redo") => direction === "undo" ? api.undo() : api.redo(), onSuccess: refresh });
  const remove = useMutation({ mutationFn: api.removeActiveLeaf, onSuccess: refresh });
  const error = begin.error ?? sync.error ?? stage.error ?? discard.error ?? commit.error ?? persist.error ?? navigate.error ?? remove.error;

  const saveCopy = async () => {
    const path = await save({ filters: [{ name: "LangCraft ChangeSet", extensions: ["chg"] }] });
    if (path) await api.saveWorkingCopy(path);
  };
  const loadCopy = async () => {
    const path = await open({ multiple: false, filters: [{ name: "LangCraft ChangeSet", extensions: ["chg"] }] });
    if (typeof path === "string") { const data = await api.loadWorkingCopy(path); setDraft(data.source); setSyncedSource(data.source); await refresh(); }
  };

  return (
    <div className="page evolution-page">
      <header className="page-header">
        <div><p className="eyebrow">F2 / AUTHORING</p><h1>{t("editor.title")}</h1></div>
        <div className="header-actions">
          <button className="icon-button" type="button" title="Back" onClick={() => navigate.mutate("undo")} disabled={Boolean(project.data?.has_pending)}><ArrowLeft /></button>
          <button className="icon-button" type="button" title="Forward" onClick={() => navigate.mutate("redo")} disabled={Boolean(project.data?.has_pending)}><ArrowRight /></button>
          <button className="button secondary" type="button" onClick={() => persist.mutate()} disabled={!project.data?.graph_dirty}><Save />{t("editor.saveProject")}</button>
        </div>
      </header>
      {project.data?.graph_dirty && <div className="status-banner warning">{t("editor.dirtyGraph")}</div>}
      {error && <ErrorNotice error={error} />}
      {!project.data?.has_pending ? (
        <section className="panel empty-workbench">
          <GitCommitHorizontal />
          <h2>{t("editor.begin")}</h2>
          <p>.chg 是唯一 working-copy 格式；commit 前不會建立演化節點。</p>
          <div className="toolbar">
            <button className="button primary" type="button" onClick={() => begin.mutate()}>{t("editor.begin")}</button>
            <button className="button danger" type="button" onClick={() => window.confirm(t("editor.deleteConfirm")) && remove.mutate()}><Trash2 />{t("editor.deleteLeaf")}</button>
          </div>
        </section>
      ) : (
        <div className="workbench-grid">
          <section className="panel code-panel">
            <div className="section-heading"><div><p className="eyebrow">RAW .CHG</p><h2>{t("editor.pending", { count: pending.data?.statements ?? 0 })}</h2></div><div className="toolbar compact"><button className="icon-button" type="button" title={t("editor.load")} onClick={loadCopy}><FileUp /></button><button className="icon-button" type="button" title={t("editor.saveAs")} onClick={saveCopy}><FileDown /></button></div></div>
            <CodeMirror value={draft} height="540px" theme="dark" onChange={setDraft} basicSetup={{ foldGutter: true, lineNumbers: true, highlightActiveLine: true }} />
            <div className={`editor-status ${localDirty ? "warning" : "success"}`}>{localDirty ? t("editor.invalid") : <><CheckCircle2 />{t("editor.valid")}</>}</div>
            <button className="button primary" type="button" onClick={() => sync.mutate()} disabled={!localDirty || sync.isPending}>{t("editor.validate")}</button>
          </section>
          <aside className="workbench-side">
            <section className="panel form-stack">
              <div className="section-heading compact"><div><p className="eyebrow">STRUCTURED</p><h2>Sound change</h2></div></div>
              <label>{t("editor.rule")}<textarea rows={4} value={rule} onChange={(event) => setRule(event.target.value)} /></label>
              <label>{t("editor.home")}<input value={home} onChange={(event) => setHome(event.target.value)} /></label>
              <button className="button secondary" type="button" disabled={localDirty || !rule.trim()} onClick={() => stage.mutate()}>{t("editor.stage")}</button>
              <button className="button ghost" type="button" disabled={localDirty || !pending.data?.statements} onClick={() => discard.mutate()}><RotateCcw />{t("editor.undoStatement")}</button>
            </section>
            <section className="panel"><div className="section-heading compact"><h2>Preview</h2></div>{pending.data && <DiffSummary diff={pending.data.diff} />}</section>
            <section className="panel form-stack">
              <label>{t("editor.label")}<input value={label} onChange={(event) => setLabel(event.target.value)} /></label>
              <button className="button primary" type="button" disabled={localDirty || !pending.data?.statements} onClick={() => commit.mutate()}><GitCommitHorizontal />{t("editor.commit")}</button>
            </section>
          </aside>
        </div>
      )}
    </div>
  );
}
