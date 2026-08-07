import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { GitFork, Network, Scale } from "lucide-react";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { DiffSummary } from "../components/DiffSummary";
import { ErrorNotice } from "../components/ErrorNotice";
import type { GroupingQuery } from "../contracts";
import { api } from "../ipc";

export function AnalysisPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [threshold, setThreshold] = useState(0.6);
  const [source, setSource] = useState("");
  const [target, setTarget] = useState("");
  const [sign, setSign] = useState("");
  const query: GroupingQuery = { view: "default", threshold };
  const tree = useQuery({ queryKey: ["tree"], queryFn: api.tree });
  const grouping = useQuery({ queryKey: ["grouping", threshold], queryFn: () => api.grouping(query) });
  const compare = useMutation({ mutationFn: () => api.intelligibility(source, target) });
  const derive = useMutation({ mutationFn: () => api.derivation(sign) });
  const assign = useMutation({
    mutationFn: ({ node, group }: { node: string; group: string }) => api.assignGroup(query, node, group),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["grouping"] }),
  });
  const label = useMutation({
    mutationFn: ({ group, value }: { group: string; value: string }) => api.labelGroup(query, group, value),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["grouping"] }),
  });
  const groups = useMemo(() => Array.from(new Set(Object.values(grouping.data?.grouping.members ?? {}))).sort(), [grouping.data]);
  const nodeLabel = (id: string) => tree.data?.nodes.find((node) => node.id === id)?.label ?? id.slice(0, 10);

  return (
    <div className="page analysis-page">
      <header className="page-header"><div><p className="eyebrow">F4 / ANALYSIS</p><h1>{t("analysis.title")}</h1></div></header>
      <section className="panel grouping-panel">
        <div className="section-heading"><div><p className="eyebrow">TREE EDGE CUT</p><h2><Network />{t("analysis.groups")}</h2></div><code>{grouping.data?.grouping.measure_id ?? "exploratory_heuristic_v1"}</code></div>
        <label className="range-field"><span>{t("analysis.threshold")} <strong>{threshold.toFixed(2)}</strong></span><input type="range" min="0" max="1" step="0.01" value={threshold} onChange={(event) => setThreshold(Number(event.target.value))} /></label>
        <p className="muted">Reference edges are intentionally excluded from lineage grouping.</p>
        {grouping.error && <ErrorNotice error={grouping.error} />}
        <div className="group-cards">
          {groups.map((group) => (
            <article key={group}>
              <input
                aria-label="Group label"
                defaultValue={grouping.data?.grouping.labels[group] ?? ""}
                placeholder={group.slice(0, 12)}
                onBlur={(event) => label.mutate({ group, value: event.target.value })}
              />
              <div>{Object.entries(grouping.data?.grouping.members ?? {}).filter(([, id]) => id === group).map(([node]) => <span className="tag" key={node}>{nodeLabel(node)}</span>)}</div>
            </article>
          ))}
        </div>
        <div className="assignment-table">
          {Object.entries(grouping.data?.grouping.members ?? {}).map(([node, group]) => <label key={node}><span>{nodeLabel(node)}</span><select value={group} onChange={(event) => assign.mutate({ node, group: event.target.value })}>{groups.map((id) => <option key={id} value={id}>{grouping.data?.grouping.labels[id] ?? id.slice(0, 12)}</option>)}</select></label>)}
        </div>
      </section>
      <div className="split-grid">
        <section className="panel form-stack">
          <div className="section-heading"><div><p className="eyebrow">PAIRWISE</p><h2><Scale />{t("analysis.intelligibility")}</h2></div></div>
          <div className="field-grid"><label>Source<select value={source} onChange={(event) => setSource(event.target.value)}><option value="">—</option>{tree.data?.nodes.map((node) => <option value={node.id} key={node.id}>{node.label ?? node.id.slice(0, 10)}</option>)}</select></label><label>Target<select value={target} onChange={(event) => setTarget(event.target.value)}><option value="">—</option>{tree.data?.nodes.map((node) => <option value={node.id} key={node.id}>{node.label ?? node.id.slice(0, 10)}</option>)}</select></label></div>
          <button className="button secondary" type="button" disabled={!source || !target} onClick={() => compare.mutate()}>{t("analysis.compare")}</button>
          {compare.error && <ErrorNotice error={compare.error} />}
          {compare.data && <div className="score-card"><strong>{(compare.data.score.value * 100).toFixed(1)}%</strong><span>{compare.data.score.measure_id} · {compare.data.score.symmetric ? "symmetric" : "directed"}</span><DiffSummary diff={compare.data.diff} /></div>}
        </section>
        <section className="panel form-stack">
          <div className="section-heading"><div><p className="eyebrow">ORIGIN DAG</p><h2><GitFork />{t("analysis.derivation")}</h2></div></div>
          <label>Sign<input value={sign} onChange={(event) => setSign(event.target.value)} /></label>
          <button className="button secondary" type="button" disabled={!sign} onClick={() => derive.mutate()}>{t("analysis.inspect")}</button>
          {derive.error && <ErrorNotice error={derive.error} />}
          {derive.data && <div className="derivation-list">{derive.data.family.nodes.map((node) => <article key={node.name}><strong>{node.name}</strong><code>{node.underlying_form ?? "—"}</code><span>{node.gloss ?? "—"}</span><small>{node.origin ? `← ${node.origin}` : "root"}</small></article>)}{derive.data.family.dangling_origins.length > 0 && <div className="status-banner warning">{t("analysis.dangling")}: {derive.data.family.dangling_origins.join(", ")}</div>}</div>}
        </section>
      </div>
    </div>
  );
}

