import { zodResolver } from "@hookform/resolvers/zod";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { BarChart3, Check, Save, Sparkles } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import { z } from "zod";
import { ErrorNotice } from "../components/ErrorNotice";
import type { ProposalQuery } from "../contracts";
import { api } from "../ipc";

const formSchema = z.object({
  name: z.string().min(1),
  gloss: z.string(),
  categories: z.string(),
  template: z.string().min(1),
  count: z.number().int().min(1).max(512),
  seed: z.number().int().nonnegative(),
  weights: z.string().min(1),
});
type FormValues = z.infer<typeof formSchema>;

function parseWeights(text: string) {
  return text.split(/\r?\n/).map((line, index) => {
    const [segment, raw, ...rest] = line.split(/\t|\s{2,}/);
    const weight = Number(raw);
    if (!segment?.trim() || rest.length || !Number.isFinite(weight) || weight < 0) {
      throw new Error(`line ${index + 1}: expected segment<TAB>non-negative weight`);
    }
    return { segment: segment.trim(), weight };
  }).filter((item) => item.segment);
}

export function GeneratePage() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const [request, setRequest] = useState<ProposalQuery | null>(null);
  const { register, handleSubmit, getValues, setValue, formState: { errors } } = useForm<FormValues>({
    resolver: zodResolver(formSchema),
    defaultValues: { name: "new_sign", gloss: "", categories: "", template: "CVC", count: 8, seed: 0, weights: "p\t1\nt\t1\nk\t1\na\t1\ni\t1\nu\t1" },
  });
  const weightConfig = useQuery({ queryKey: ["weights"], queryFn: api.weightConfig });
  useEffect(() => {
    if (weightConfig.data?.manual.length) {
      setValue(
        "weights",
        weightConfig.data.manual.map((item) => `${item.segment}\t${item.weight}`).join("\n"),
      );
    }
  }, [setValue, weightConfig.data]);
  const saveWeights = useMutation({
    mutationFn: () => api.setWeights(parseWeights(getValues("weights"))),
    onSuccess: (value) => queryClient.setQueryData(["weights"], value),
  });
  const proposals = useMutation({ mutationFn: api.propose, onSuccess: (_, variables) => setRequest(variables) });
  const adopt = useMutation({
    mutationFn: (index: number) => {
      if (!request) throw new Error("No active proposal request");
      return api.adoptProposal(request, index);
    },
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["pending"] }),
        queryClient.invalidateQueries({ queryKey: ["project"] }),
      ]);
    },
  });
  const inventory = useMemo(() => request?.weights.map((item) => item.segment) ?? [], [request]);
  const stats = useQuery({ queryKey: ["stats", inventory], queryFn: () => api.stats(inventory) });
  const maxCount = Math.max(1, ...(stats.data?.segments.map((item) => item.count) ?? [1]));

  const submit = (values: FormValues) => {
    try {
      const query: ProposalQuery = {
        name: values.name,
        gloss: values.gloss || undefined,
        categories: values.categories.split(",").map((item) => item.trim()).filter(Boolean),
        template: values.template,
        count: values.count,
        seed: values.seed,
        weights: parseWeights(values.weights),
      };
      proposals.mutate(query);
    } catch (error) {
      proposals.reset();
      window.alert(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <div className="page generate-page">
      <header className="page-header"><div><p className="eyebrow">F3 / ASSISTED CREATION</p><h1>{t("generate.title")}</h1></div></header>
      <div className="split-grid">
        <section className="panel">
          <div className="section-heading"><div><p className="eyebrow">NEED → PROPOSAL</p><h2><Sparkles />Generator</h2></div></div>
          <form className="form-stack" onSubmit={handleSubmit(submit)}>
            <div className="field-grid"><label>{t("generate.name")}<input {...register("name")} /></label><label>{t("generate.gloss")}<input {...register("gloss")} /></label></div>
            <label>{t("generate.category")}<input {...register("categories")} /></label>
            <div className="field-grid three"><label>{t("generate.template")}<input {...register("template")} /></label><label>{t("generate.count")}<input type="number" {...register("count", { valueAsNumber: true })} /></label><label>{t("generate.seed")}<input type="number" {...register("seed", { valueAsNumber: true })} /></label></div>
            <label>{t("generate.weights")}<textarea className="mono" rows={9} {...register("weights")} /></label>
            {Object.keys(errors).length > 0 && <div className="status-banner error">Please check the generation fields.</div>}
            {proposals.error && <ErrorNotice error={proposals.error} />}
            <div className="button-row">
              <button className="button secondary" type="button" disabled={saveWeights.isPending} onClick={() => saveWeights.mutate()}><Save />{t("generate.saveWeights")}</button>
              <button className="button primary" type="submit" disabled={proposals.isPending}><Sparkles />{t("generate.propose")}</button>
            </div>
            {saveWeights.error && <ErrorNotice error={saveWeights.error} />}
          </form>
          <div className="proposal-list">
            {proposals.data?.proposals.map((proposal, index) => (
              <article key={`${proposal.phon}:${index}`}>
                <div><strong>{proposal.phon}</strong><span>score {proposal.score.toFixed(3)}</span><p>{proposal.rationale}</p></div>
                <button className="button secondary" type="button" onClick={() => adopt.mutate(index)} disabled={adopt.isPending}><Check />{t("generate.adopt")}</button>
              </article>
            ))}
          </div>
          {adopt.error && <ErrorNotice error={adopt.error} />}
        </section>
        <section className="panel stats-panel">
          <div className="section-heading"><div><p className="eyebrow">REPORT ONLY</p><h2><BarChart3 />Phoneme projection</h2></div></div>
          <div className="status-banner info">{t("generate.reportOnly")}</div>
          <p className="muted-copy">{t("generate.weightSource", { source: weightConfig.data?.declaration_source ?? "project.toml:weights" })}</p>
          <div className="weight-sources">
            {weightConfig.data?.effective.map((item) => <span key={item.segment}><code>{item.segment}</code> {item.weight} <em>{item.source}</em></span>)}
          </div>
          {weightConfig.error && <ErrorNotice error={weightConfig.error} />}
          {stats.error && <ErrorNotice error={stats.error} />}
          <div className="bar-chart">{stats.data?.segments.map((item) => <div className="bar-row" key={item.segment}><code>{item.segment}</code><div><i style={{ width: `${(item.count / maxCount) * 100}%` }} /></div><strong>{item.count}</strong></div>)}</div>
        </section>
      </div>
    </div>
  );
}
