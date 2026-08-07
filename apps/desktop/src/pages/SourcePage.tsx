import CodeMirror from "@uiw/react-codemirror";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, Braces, CheckCircle2, GitBranch, RotateCcw } from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router-dom";
import { ErrorNotice } from "../components/ErrorNotice";
import { api } from "../ipc";

export function SourcePage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const project = useQuery({ queryKey: ["project"], queryFn: api.summary });
  const source = useQuery({ queryKey: ["source"], queryFn: api.source });
  const tree = useQuery({ queryKey: ["tree"], queryFn: api.tree });
  const [draft, setDraft] = useState("");
  const [syncedNode, setSyncedNode] = useState("");
  const [rebaseNode, setRebaseNode] = useState("");
  const [rebaseOnto, setRebaseOnto] = useState("");

  useEffect(() => {
    if (source.data && source.data.node !== syncedNode) {
      setDraft(source.data.source);
      setSyncedNode(source.data.node);
    }
  }, [source.data, syncedNode]);

  const reconcile = useMutation({
    mutationFn: () => api.reconcileSource(draft),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["project"] }),
        queryClient.invalidateQueries({ queryKey: ["pending"] }),
      ]);
    },
  });
  const selectedRebaseNode = rebaseNode || tree.data?.nodes.find((node) => node.parents.length > 0)?.id || "";
  const selectedRebaseOnto = rebaseOnto || tree.data?.active || "";
  const rebasePreview = useMutation({
    mutationFn: () => api.previewRebase(selectedRebaseNode, selectedRebaseOnto),
  });
  const applyRebase = useMutation({
    mutationFn: () => api.applyRebase(selectedRebaseNode, selectedRebaseOnto),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["project"] }),
        queryClient.invalidateQueries({ queryKey: ["tree"] }),
        queryClient.invalidateQueries({ queryKey: ["source"] }),
      ]);
    },
  });
  const dirty = Boolean(source.data && draft !== source.data.source);

  return (
    <div className="page source-page">
      <header className="page-header">
        <div><p className="eyebrow">F5 / EXPERT EDITOR</p><h1>{t("source.title")}</h1></div>
        <div className="header-actions">
          <button className="button ghost" type="button" disabled={!dirty} onClick={() => setDraft(source.data?.source ?? "")}><RotateCcw />{t("source.reset")}</button>
          <button className="button primary" type="button" disabled={!dirty || Boolean(project.data?.has_pending) || reconcile.isPending} onClick={() => reconcile.mutate()}><Braces />{t("source.stage")}</button>
        </div>
      </header>
      <div className="status-banner info"><Braces />{t("source.note")}</div>
      {project.data?.has_pending && <div className="status-banner warning">{t("source.pendingBlocked")}</div>}
      {(source.error || reconcile.error || rebasePreview.error || applyRebase.error) && <ErrorNotice error={source.error ?? reconcile.error ?? rebasePreview.error ?? applyRebase.error} onRetry={() => source.refetch()} />}
      {reconcile.data && (
        <section className="panel reconcile-report">
          <div className="section-heading">
            <div><p className="eyebrow">IDENTITY RECONCILE</p><h2><CheckCircle2 />{t("source.report")}</h2></div>
            <Link className="button secondary" to="/evolution">{t("source.reviewPending")}<ArrowRight /></Link>
          </div>
          <div className="metrics-grid">
            <div><strong>{reconcile.data.matched}</strong><span>{t("source.matched")}</span></div>
            <div><strong>{reconcile.data.inserted}</strong><span>{t("source.inserted")}</span></div>
            <div><strong>{reconcile.data.deleted}</strong><span>{t("source.deleted")}</span></div>
            <div><strong>{reconcile.data.primitive_edits}</strong><span>{t("source.edits")}</span></div>
          </div>
          <p>{t("source.reconcileBoundary")}</p>
        </section>
      )}
      <section className="panel code-panel">
        <div className="section-heading"><div><p className="eyebrow">CANONICAL SNAPSHOT</p><h2>{source.data?.node.slice(0, 16) ?? "—"}</h2></div>{dirty && <span className="badge warning">{t("source.draft")}</span>}</div>
        <CodeMirror value={draft} height="680px" theme="dark" editable={!project.data?.has_pending} onChange={setDraft} basicSetup={{ lineNumbers: true, foldGutter: true }} />
      </section>
      <section className="panel form-stack">
        <div className="section-heading"><div><p className="eyebrow">REBASE</p><h2><GitBranch />{t("source.rebaseTitle")}</h2></div></div>
        <p>{t("source.rebaseNote")}</p>
        <div className="form-grid two">
          <label>{t("source.rebaseNode")}
            <select value={selectedRebaseNode} onChange={(event) => { setRebaseNode(event.target.value); rebasePreview.reset(); }}>
              {tree.data?.nodes.filter((node) => node.parents.length > 0).map((node) => <option key={node.id} value={node.id}>{node.label ?? node.id.slice(0, 16)}</option>)}
            </select>
          </label>
          <label>{t("source.rebaseOnto")}
            <select value={selectedRebaseOnto} onChange={(event) => { setRebaseOnto(event.target.value); rebasePreview.reset(); }}>
              {tree.data?.nodes.map((node) => <option key={node.id} value={node.id}>{node.label ?? node.id.slice(0, 16)}</option>)}
            </select>
          </label>
        </div>
        <div className="toolbar">
          <button className="button secondary" type="button" disabled={!selectedRebaseNode || !selectedRebaseOnto || Boolean(project.data?.has_pending)} onClick={() => rebasePreview.mutate()}>{t("source.previewRebase")}</button>
          <button className="button primary" type="button" disabled={rebasePreview.data?.status !== "clean" || applyRebase.isPending} onClick={() => window.confirm(t("source.applyRebaseConfirm")) && applyRebase.mutate()}>{t("source.applyRebase")}</button>
        </div>
        {rebasePreview.data && <div className={`status-banner ${rebasePreview.data.status === "clean" ? "success" : "warning"}`}><strong>{rebasePreview.data.status}</strong>{rebasePreview.data.statement !== undefined && <span>#{rebasePreview.data.statement}</span>}<span>{rebasePreview.data.message ?? rebasePreview.data.result}</span></div>}
        {applyRebase.data?.status === "clean" && <div className="status-banner success"><CheckCircle2 />{t("source.rebaseApplied")}</div>}
      </section>
    </div>
  );
}
