import { useMutation, useQueryClient } from "@tanstack/react-query";
import { FileText, Save } from "lucide-react";
import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { useTranslation } from "react-i18next";
import type { EvolutionState, NodeDetail } from "../contracts";
import { api } from "../ipc";
import { ErrorNotice } from "./ErrorNotice";

type MetadataForm = { label: string; time: string; region: string; society: string };

export function NodeInspector({ detail }: { detail: NodeDetail }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const { register, handleSubmit, reset } = useForm<MetadataForm>();
  const [annotationPath, setAnnotationPath] = useState(detail.annotations[0] ?? "notes.md");
  const [annotation, setAnnotation] = useState("");
  useEffect(() => {
    reset({
      label: detail.label ?? "",
      time: detail.state.time ?? "",
      region: detail.state.region ?? "",
      society: detail.state.society.join(", "),
    });
  }, [detail, reset]);
  useEffect(() => {
    if (detail.annotations.includes(annotationPath)) {
      void api.readAnnotation(annotationPath).then(setAnnotation).catch(() => setAnnotation(""));
    } else setAnnotation("");
  }, [annotationPath, detail.annotations]);

  const refresh = async () => {
    await Promise.all([
      queryClient.invalidateQueries({ queryKey: ["detail"] }),
      queryClient.invalidateQueries({ queryKey: ["tree"] }),
      queryClient.invalidateQueries({ queryKey: ["project"] }),
    ]);
  };
  const metadata = useMutation({
    mutationFn: async (values: MetadataForm) => {
      await api.setLabel(values.label || undefined);
      const state: EvolutionState = {
        time: values.time || undefined,
        region: values.region || undefined,
        society: values.society.split(",").map((item) => item.trim()).filter(Boolean),
        contacts: detail.state.contacts,
      };
      return api.setState(state);
    },
    onSuccess: refresh,
  });
  const annotate = useMutation({
    mutationFn: () => api.writeAnnotation(annotationPath, annotation),
    onSuccess: refresh,
  });

  return (
    <div className="inspector-stack">
      <div className="node-identity"><span className="eyebrow">NODE</span><code>{detail.id}</code><span>{detail.sign_count} signs</span></div>
      <form className="form-stack" onSubmit={handleSubmit((values) => metadata.mutate(values))}>
        <label>Label<input {...register("label")} /></label>
        <div className="field-grid"><label>{t("state.time")}<input {...register("time")} /></label><label>{t("state.region")}<input {...register("region")} /></label></div>
        <label>{t("state.society")}<input {...register("society")} placeholder="urban, guild" /></label>
        {metadata.error && <ErrorNotice error={metadata.error} />}
        <button className="button secondary" type="submit" disabled={metadata.isPending}><Save />{t("common.save")}</button>
      </form>
      <div className="annotation-editor">
        <div className="section-heading compact"><h3><FileText />{t("state.annotations")}</h3></div>
        <input list="annotation-files" value={annotationPath} onChange={(event) => setAnnotationPath(event.target.value)} />
        <datalist id="annotation-files">{detail.annotations.map((path) => <option key={path} value={path} />)}</datalist>
        <textarea value={annotation} onChange={(event) => setAnnotation(event.target.value)} rows={7} />
        {annotate.error && <ErrorNotice error={annotate.error} />}
        <button className="button ghost" type="button" onClick={() => annotate.mutate()} disabled={!annotationPath || annotate.isPending}>{t("common.save")}</button>
      </div>
    </div>
  );
}

