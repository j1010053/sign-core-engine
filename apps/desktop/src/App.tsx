import { useQuery } from "@tanstack/react-query";
import { lazy, Suspense } from "react";
import { Navigate, Route, Routes } from "react-router-dom";
import { ErrorNotice } from "./components/ErrorNotice";
import { Shell } from "./components/Shell";
import { api } from "./ipc";
import { Launcher } from "./pages/Launcher";

const OverviewPage = lazy(() => import("./pages/OverviewPage").then((module) => ({ default: module.OverviewPage })));
const EvolutionPage = lazy(() => import("./pages/EvolutionPage").then((module) => ({ default: module.EvolutionPage })));
const GeneratePage = lazy(() => import("./pages/GeneratePage").then((module) => ({ default: module.GeneratePage })));
const AnalysisPage = lazy(() => import("./pages/AnalysisPage").then((module) => ({ default: module.AnalysisPage })));
const SourcePage = lazy(() => import("./pages/SourcePage").then((module) => ({ default: module.SourcePage })));
const SettingsPage = lazy(() => import("./pages/SettingsPage").then((module) => ({ default: module.SettingsPage })));

export default function App() {
  const project = useQuery({ queryKey: ["project"], queryFn: api.summary });

  if (project.isPending) return <div className="boot-screen"><div className="spinner" />LangCraft</div>;
  if (project.error) return <div className="boot-screen"><ErrorNotice error={project.error} onRetry={() => project.refetch()} /></div>;
  if (!project.data) return <Launcher />;

  return (
    <Suspense fallback={<div className="boot-screen"><div className="spinner" />LangCraft</div>}>
      <Routes>
        <Route element={<Shell project={project.data} />}>
          <Route index element={<OverviewPage />} />
          <Route path="evolution" element={<EvolutionPage />} />
          <Route path="generate" element={<GeneratePage />} />
          <Route path="analysis" element={<AnalysisPage />} />
          <Route path="source" element={<SourcePage />} />
          <Route path="settings" element={<SettingsPage project={project.data} />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
    </Suspense>
  );
}
