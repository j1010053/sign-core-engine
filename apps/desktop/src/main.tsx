import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { HashRouter } from "react-router-dom";
import { TooltipProvider } from "@radix-ui/react-tooltip";
import "@xyflow/react/dist/style.css";
import "./i18n";
import "./styles.css";
import App from "./App";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 15_000, retry: 1 },
    mutations: { retry: 0 },
  },
});

async function bootstrap() {
  if (import.meta.env.VITE_WDIO === "true") {
    await import("@wdio/tauri-plugin");
  }
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <TooltipProvider delayDuration={350}>
          <HashRouter>
            <App />
          </HashRouter>
        </TooltipProvider>
      </QueryClientProvider>
    </React.StrictMode>,
  );
}

void bootstrap();
