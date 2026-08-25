import { useMutation, useQueryClient } from "@tanstack/react-query";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
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
import { useCallback, useEffect, useRef, useState } from "react";
import { NavLink, Outlet, useBlocker, useLocation, useNavigate } from "react-router-dom";
import { useTranslation } from "react-i18next";
import type { ProjectSummary } from "../contracts";
import { DirtyDraftProvider, type SaveDirtyDraft } from "../dirtyGuard";
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
  const [dirtyDrafts, setDirtyDrafts] = useState<Map<string, SaveDirtyDraft>>(
    () => new Map(),
  );
  const [closeRequested, setCloseRequested] = useState(false);
  const [guardBusy, setGuardBusy] = useState(false);
  const [guardError, setGuardError] = useState<unknown>();
  const projectRef = useRef(project);
  const dirtyDraftsRef = useRef(dirtyDrafts);
  const allowNativeClose = useRef(false);

  useEffect(() => {
    projectRef.current = project;
  }, [project]);
  useEffect(() => {
    dirtyDraftsRef.current = dirtyDrafts;
  }, [dirtyDrafts]);

  const registerDirtyDraft = useCallback((key: string, save: SaveDirtyDraft | null) => {
    setDirtyDrafts((current) => {
      const next = new Map(current);
      if (save) next.set(key, save);
      else next.delete(key);
      return next;
    });
  }, []);
  const blocker = useBlocker(
    ({ currentLocation, nextLocation }) =>
      dirtyDrafts.size > 0 &&
      (currentLocation.pathname !== nextLocation.pathname ||
        currentLocation.search !== nextLocation.search ||
        currentLocation.hash !== nextLocation.hash),
  );
  useEffect(() => {
    if (blocker.state === "blocked") setGuardError(undefined);
  }, [blocker.state]);
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
    if (!("__TAURI_INTERNALS__" in window)) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onCloseRequested((event) => {
        if (allowNativeClose.current) return;
        const current = projectRef.current;
        if (!current.graph_dirty && !current.has_pending && dirtyDraftsRef.current.size === 0) {
          return;
        }
        event.preventDefault();
        setGuardError(undefined);
        setCloseRequested(true);
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch((error) => setGuardError(error));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

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

  const saveLocalDrafts = async () => {
    for (const saveDraft of dirtyDraftsRef.current.values()) {
      await saveDraft();
    }
  };

  const saveAndLeave = async () => {
    if (blocker.state !== "blocked") return;
    setGuardBusy(true);
    setGuardError(undefined);
    try {
      await saveLocalDrafts();
      blocker.proceed();
    } catch (error) {
      setGuardError(error);
    } finally {
      setGuardBusy(false);
    }
  };

  const closeWindow = async () => {
    allowNativeClose.current = true;
    try {
      await getCurrentWindow().close();
    } catch (error) {
      allowNativeClose.current = false;
      throw error;
    }
  };

  const saveAndClose = async () => {
    setGuardBusy(true);
    setGuardError(undefined);
    try {
      const before = projectRef.current;
      const needsWorkingCopy = before.has_pending || dirtyDraftsRef.current.size > 0;
      const workingCopyPath = needsWorkingCopy
        ? await saveDialog({
            filters: [{ name: "LangCraft ChangeSet", extensions: ["chg"] }],
          })
        : null;
      // Cancelling the path chooser must not mutate the Rust session.
      if (needsWorkingCopy && !workingCopyPath) return;

      await saveLocalDrafts();
      const current = await api.summary();
      if (current?.has_pending) {
        if (!workingCopyPath) throw new Error(t("dirtyGuard.workingCopyPathRequired"));
        await api.saveWorkingCopy(workingCopyPath);
      }
      if (current?.graph_dirty) await api.saveProject();
      await closeWindow();
    } catch (error) {
      setGuardError(error);
    } finally {
      setGuardBusy(false);
    }
  };

  const discardAndClose = async () => {
    setGuardBusy(true);
    setGuardError(undefined);
    try {
      await closeWindow();
    } catch (error) {
      setGuardError(error);
    } finally {
      setGuardBusy(false);
    }
  };

  return (
    <DirtyDraftProvider value={registerDirtyDraft}>
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
      {blocker.state === "blocked" && !closeRequested && (
        <div className="modal-backdrop" role="presentation">
          <section className="modal" role="dialog" aria-modal="true" aria-labelledby="route-dirty-title">
            <div>
              <p className="eyebrow">UNSAVED DRAFT</p>
              <h2 id="route-dirty-title">{t("dirtyGuard.routeTitle")}</h2>
            </div>
            <p>{t("dirtyGuard.routeMessage")}</p>
            {guardError !== undefined && <div className="status-banner error" role="alert">{guardError instanceof Error ? guardError.message : String(guardError)}</div>}
            <div className="modal-actions">
              <button className="button ghost" type="button" disabled={guardBusy} onClick={() => blocker.reset()}>{t("dirtyGuard.stay")}</button>
              <button className="button danger" type="button" disabled={guardBusy} onClick={() => blocker.proceed()}>{t("dirtyGuard.discardLeave")}</button>
              <button className="button primary" type="button" disabled={guardBusy} onClick={saveAndLeave}>{t("dirtyGuard.saveLeave")}</button>
            </div>
          </section>
        </div>
      )}
      {closeRequested && (
        <div className="modal-backdrop" role="presentation">
          <section className="modal" role="dialog" aria-modal="true" aria-labelledby="close-dirty-title">
            <div>
              <p className="eyebrow">UNSAVED PROJECT</p>
              <h2 id="close-dirty-title">{t("dirtyGuard.closeTitle")}</h2>
            </div>
            <p>{t("dirtyGuard.closeMessage", { name: project.name ?? t("shell.untitled") })}</p>
            {(project.has_pending || dirtyDrafts.size > 0) && <p className="muted-copy">{t("dirtyGuard.workingCopyHint")}</p>}
            {guardError !== undefined && <div className="status-banner error" role="alert">{guardError instanceof Error ? guardError.message : String(guardError)}</div>}
            <div className="modal-actions">
              <button className="button ghost" type="button" disabled={guardBusy} onClick={() => setCloseRequested(false)}>{t("dirtyGuard.stay")}</button>
              <button className="button danger" type="button" disabled={guardBusy} onClick={discardAndClose}>{t("dirtyGuard.discardClose")}</button>
              <button className="button primary" type="button" disabled={guardBusy} onClick={saveAndClose}>{t("dirtyGuard.saveClose")}</button>
            </div>
          </section>
        </div>
      )}
    </div>
    </DirtyDraftProvider>
  );
}
