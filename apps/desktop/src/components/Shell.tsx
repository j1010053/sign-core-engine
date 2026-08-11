import { useMutation, useQueryClient } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  BarChart3,
  Braces,
  CircleAlert,
  GitBranch,
  Languages,
  LayoutDashboard,
  Save,
  Settings,
  Sparkles,
} from "lucide-react";
import { useEffect } from "react";
import { NavLink, Outlet, useLocation, useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import type { ProjectSummary } from "../contracts";
import { api } from "../ipc";

const nav = [
  ["/", "nav.overview", LayoutDashboard],
  ["/evolution", "nav.evolution", GitBranch],
  ["/generate", "nav.generate", Sparkles],
  ["/analysis", "nav.analysis", BarChart3],
  ["/source", "nav.source", Braces],
  ["/settings", "nav.settings", Settings],
] as const;

export function Shell({ project }: { project: ProjectSummary }) {
  const { t } = useTranslation();
  const location = useLocation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const current = nav.find(([to]) => to === location.pathname) ?? nav[0];
  const saveProject = useMutation({
    mutationFn: api.saveProject,
    onSuccess: async (summary) => {
      queryClient.setQueryData(["project"], summary);
      await queryClient.invalidateQueries({ queryKey: ["tree"] });
    },
  });

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    const title = `${project.name ?? t("shell.untitled")} — ${t("app.name")}`;
    void getCurrentWindow().setTitle(title).catch(() => {
      // The browser-only Vite preview has no Tauri window; the packaged app does.
    });
  }, [project.name, t]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.ctrlKey && event.key.toLowerCase() === "s") {
        event.preventDefault();
        if (project.graph_dirty && !saveProject.isPending) saveProject.mutate();
        return;
      }
      if (event.altKey && /^[1-6]$/.test(event.key)) {
        event.preventDefault();
        navigate(nav[Number(event.key) - 1][0]);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate, project.graph_dirty, saveProject]);

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark"><Languages aria-hidden="true" /></div>
          <div><strong>{t("app.name")}</strong><span>{t("app.tagline")}</span></div>
        </div>
        <nav aria-label="Primary">
          {nav.map(([to, key, Icon], index) => (
            <NavLink key={to} to={to} end={to === "/"} title={`${t(key)}  Alt+${index + 1}`}>
              <Icon aria-hidden="true" />
              <span>{t(key)}</span>
              <kbd>{index + 1}</kbd>
            </NavLink>
          ))}
        </nav>
        <div className="project-chip">
          <span className="eyebrow">PROJECT</span>
          <strong>{project.name ?? t("shell.untitled")}</strong>
          <span title={project.path}>{project.path}</span>
          <div className="status-row">
            {project.graph_dirty && <span className="badge warning"><Save />{t("shell.unsaved")}</span>}
            {project.has_pending && <span className="badge">{t("shell.workingCopy")}</span>}
          </div>
        </div>
      </aside>
      <main className="main-area">
        <header className="desktop-commandbar">
          <div className="command-context">
            <span>{t("shell.workspace")}</span>
            <strong>{t(current[1])}</strong>
          </div>
          <div className="command-status" role="status">
            {project.has_pending && <span className="working-indicator"><i />{t("shell.pending")}</span>}
            {project.graph_dirty && <span className="dirty-indicator"><i />{t("shell.unsavedChanges")}</span>}
            {!project.has_pending && !project.graph_dirty && <span>{t("shell.saved")}</span>}
          </div>
          <button
            className="button command-save"
            type="button"
            disabled={!project.graph_dirty || saveProject.isPending}
            title={`${t("editor.saveProject")} (Ctrl+S)`}
            onClick={() => saveProject.mutate()}
          >
            <Save />
            <span>{saveProject.isPending ? t("shell.saving") : t("common.save")}</span>
            <kbd>Ctrl S</kbd>
          </button>
        </header>
        {saveProject.error && (
          <div className="command-error" role="alert">
            <CircleAlert />{saveProject.error instanceof Error ? saveProject.error.message : t("errors.fallback")}
          </div>
        )}
        <div className="route-view"><Outlet /></div>
      </main>
    </div>
  );
}
