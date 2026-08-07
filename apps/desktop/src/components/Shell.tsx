import {
  BarChart3,
  Braces,
  GitBranch,
  Languages,
  LayoutDashboard,
  Save,
  Settings,
  Sparkles,
} from "lucide-react";
import { NavLink, Outlet } from "react-router-dom";
import { useTranslation } from "react-i18next";
import type { ProjectSummary } from "../contracts";

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
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark"><Languages aria-hidden="true" /></div>
          <div><strong>{t("app.name")}</strong><span>{t("app.tagline")}</span></div>
        </div>
        <nav aria-label="Primary">
          {nav.map(([to, key, Icon]) => (
            <NavLink key={to} to={to} end={to === "/"}>
              <Icon aria-hidden="true" />
              <span>{t(key)}</span>
            </NavLink>
          ))}
        </nav>
        <div className="project-chip">
          <span className="eyebrow">PROJECT</span>
          <strong>{project.name ?? "Untitled"}</strong>
          <span title={project.path}>{project.path}</span>
          <div className="status-row">
            {project.graph_dirty && <span className="badge warning"><Save />未儲存</span>}
            {project.has_pending && <span className="badge">.chg</span>}
          </div>
        </div>
      </aside>
      <main className="main-area"><Outlet /></main>
    </div>
  );
}

