import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Filter, GitBranch, Search } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ErrorNotice } from "../components/ErrorNotice";
import { EvolutionGraph } from "../components/EvolutionGraph";
import { LexiconTable } from "../components/LexiconTable";
import { NodeInspector } from "../components/NodeInspector";
import { api } from "../ipc";

export function OverviewPage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [gloss, setGloss] = useState("");
  const [category, setCategory] = useState("");
  const [sort, setSort] = useState<"name" | "form" | "gloss">("name");
  const tree = useQuery({ queryKey: ["tree"], queryFn: api.tree });
  const detail = useQuery({ queryKey: ["detail", tree.data?.active], queryFn: api.nodeDetail, enabled: Boolean(tree.data?.active) });
  const lexicon = useQuery({
    queryKey: ["lexicon", tree.data?.active, gloss, category, sort],
    queryFn: () => api.lexicon({ gloss_contains: gloss || undefined, category: category || undefined, sort }),
    enabled: Boolean(tree.data?.active),
  });
  const select = useMutation({
    mutationFn: api.selectNode,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["tree"] }),
        queryClient.invalidateQueries({ queryKey: ["detail"] }),
        queryClient.invalidateQueries({ queryKey: ["lexicon"] }),
        queryClient.invalidateQueries({ queryKey: ["project"] }),
      ]);
    },
  });

  return (
    <div className="page overview-page">
      <header className="page-header"><div><p className="eyebrow">F1 / WORKSPACE</p><h1>{t("overview.title")}</h1></div></header>
      {(tree.error || select.error) && <ErrorNotice error={tree.error ?? select.error} onRetry={() => tree.refetch()} />}
      <section className="panel graph-panel">
        <div className="section-heading"><div><p className="eyebrow">LINEAGE</p><h2><GitBranch />{t("overview.tree")}</h2></div><div className="legend"><span><i className="line trunk" />trunk</span><span><i className="line reference" />reference</span></div></div>
        {tree.data?.nodes.length ? <EvolutionGraph tree={tree.data} onSelect={(id) => select.mutate(id)} /> : <p className="empty-state">{t("overview.emptyTree")}</p>}
      </section>
      <div className="overview-lower">
        <section className="panel lexicon-panel">
          <div className="section-heading"><div><p className="eyebrow">LEXICON</p><h2>{t("overview.lexicon")}</h2></div>{lexicon.data && <span>{t("overview.entries", { shown: lexicon.data.lexicon.entries.length, total: lexicon.data.lexicon.total_before_filter })}</span>}</div>
          <div className="toolbar">
            <label className="search-field"><Search /><input value={gloss} onChange={(event) => setGloss(event.target.value)} placeholder={t("overview.filter")} /></label>
            <label className="search-field"><Filter /><input value={category} onChange={(event) => setCategory(event.target.value)} placeholder={t("overview.category")} /></label>
            <select value={sort} onChange={(event) => setSort(event.target.value as typeof sort)}><option value="name">Name</option><option value="form">UR</option><option value="gloss">Gloss</option></select>
          </div>
          {lexicon.error ? <ErrorNotice error={lexicon.error} /> : <LexiconTable entries={lexicon.data?.lexicon.entries ?? []} />}
        </section>
        <aside className="panel inspector-panel"><div className="section-heading"><div><p className="eyebrow">METADATA</p><h2>{t("overview.node")}</h2></div></div>{detail.error ? <ErrorNotice error={detail.error} /> : detail.data ? <NodeInspector detail={detail.data} /> : <p className="empty-state">{t("common.none")}</p>}</aside>
      </div>
    </div>
  );
}

