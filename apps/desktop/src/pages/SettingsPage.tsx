import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, Box, FolderX, Languages, PackageOpen, RotateCw } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import type { PackageSelection, ProjectSummary } from "../contracts";
import { ErrorNotice } from "../components/ErrorNotice";
import { api } from "../ipc";
import { setLocale } from "../i18n";

export function SettingsPage({ project }: { project: ProjectSummary }) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const catalog = useQuery({ queryKey: ["package-catalog"], queryFn: api.packageCatalog });
  const [declared, setDeclared] = useState<Set<string>>(new Set());

  useEffect(() => {
    if (catalog.data) {
      setDeclared(new Set(catalog.data.packages.filter((item) => item.declared).map((item) => item.id)));
    }
  }, [catalog.data]);

  const original = useMemo(
    () => catalog.data?.packages.filter((item) => item.declared).map((item) => item.id).sort() ?? [],
    [catalog.data],
  );
  const changed = original.join("\n") !== [...declared].sort().join("\n");
  const dirty = project.graph_dirty || project.has_pending;

  const packageSelection = (): PackageSelection => {
    const packages = catalog.data?.packages.filter((item) => declared.has(item.id)) ?? [];
    return {
      std: packages.filter((item) => item.kind === "std").map((item) => item.id),
      natural: packages.find((item) => item.kind === "natural")?.id,
      plugins: packages.filter((item) => item.kind === "plugin").map((item) => item.id),
    };
  };

  const configure = useMutation({
    mutationFn: () => api.configurePackages(packageSelection()),
    onSuccess: (summary) => {
      queryClient.setQueryData(["project"], summary);
      void queryClient.invalidateQueries();
    },
  });

  const togglePackage = (id: string, kind: string, checked: boolean) => {
    setDeclared((current) => {
      const next = new Set(current);
      if (kind === "natural" && checked) {
        for (const item of catalog.data?.packages ?? []) {
          if (item.kind === "natural") next.delete(item.id);
        }
      }
      if (checked) next.add(id);
      else next.delete(id);
      return next;
    });
  };

  const close = useMutation({
    mutationFn: async () => {
      if (dirty) {
        const shouldSave = window.confirm(
          "Save the project before closing?\nOK = Save, Cancel = choose discard/cancel",
        );
        if (shouldSave) {
          if (project.has_pending) throw new Error("Commit or save the pending .chg before closing.");
          await api.saveProject();
          await api.closeProject(false);
          return;
        }
        const discard = window.confirm(
          "Discard unsaved session state and close?\nCancel keeps the project open.",
        );
        if (!discard) return;
        await api.closeProject(true);
        return;
      }
      await api.closeProject(false);
    },
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ["project"] }),
  });

  return (
    <div className="page settings-page">
      <header className="page-header">
        <div><p className="eyebrow">F5–F6 / PROJECT</p><h1>{t("settings.title")}</h1></div>
      </header>
      <div className="settings-grid">
        <section className="panel form-stack">
          <div className="section-heading"><div><p className="eyebrow">INTERFACE</p><h2><Languages />{t("settings.language")}</h2></div></div>
          <select value={i18n.language} onChange={(event) => void setLocale(event.target.value as "zh-TW" | "en")}>
            <option value="zh-TW">{t("settings.zh")}</option>
            <option value="en">{t("settings.en")}</option>
          </select>
        </section>

        <section className="panel">
          <div className="section-heading"><div><p className="eyebrow">PROJECT</p><h2><Box />{project.name ?? "Untitled"}</h2></div></div>
          <dl className="facts">
            <div><dt>Path</dt><dd>{project.path}</dd></div>
            <div><dt>Nodes</dt><dd>{project.node_count}</dd></div>
            <div><dt>{t("settings.packages")}</dt><dd>{project.packages.join(", ") || "—"}</dd></div>
          </dl>
          {project.legacy && <div className="status-banner warning">{t("settings.legacy")}</div>}
        </section>

        <section className="panel package-panel">
          <div className="section-heading">
            <div><p className="eyebrow">PROJECT.TOML</p><h2><PackageOpen />{t("settings.packageCatalog")}</h2></div>
          </div>
          <p className="muted-copy">{t("settings.packageHelp")}</p>
          {dirty && <div className="status-banner warning"><AlertTriangle />{t("settings.packageDirty")}</div>}
          {catalog.error && <ErrorNotice error={catalog.error} onRetry={() => catalog.refetch()} />}
          <div className="package-list">
            {catalog.data?.packages.map((item) => {
              const transitive = item.selected && !item.declared;
              return (
                <label className="package-card" key={item.id}>
                  <input
                    type="checkbox"
                    checked={declared.has(item.id)}
                    disabled={!item.enabled || configure.isPending}
                    onChange={(event) => togglePackage(item.id, item.kind, event.target.checked)}
                  />
                  <span>
                    <strong>{item.id}</strong>
                    <small>{item.kind} · v{item.version} · {t("settings.packageSource")}</small>
                    {item.requires.length > 0 && <small>{t("settings.packageRequires", { packages: item.requires.join(", ") })}</small>}
                    {transitive && <em>{t("settings.packageDependency")}</em>}
                  </span>
                </label>
              );
            })}
          </div>
          <button
            className="button primary"
            type="button"
            disabled={dirty || !changed || configure.isPending}
            onClick={() => {
              if (window.confirm(t("settings.packageConfirm"))) configure.mutate();
            }}
          >
            <RotateCw />{t("settings.packageApply")}
          </button>
          {configure.error && <ErrorNotice error={configure.error} />}
        </section>

        <section className="panel danger-zone">
          <div><h2>{t("settings.close")}</h2><p>最近專案清單仍會保留此路徑。</p></div>
          <button className="button danger" type="button" onClick={() => close.mutate()}><FolderX />{t("settings.close")}</button>
          {close.error && <ErrorNotice error={close.error} />}
        </section>
      </div>
    </div>
  );
}
