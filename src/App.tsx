import { useEffect, useState } from "react";
import ImportPage from "./pages/ImportPage";
import ResultPage from "./pages/ResultPage";
import SettingsPage from "./pages/SettingsPage";
import HistoryPage from "./pages/HistoryPage";
import ContextsPage from "./pages/ContextsPage";
import { getConfig } from "./api/tauri";
import type { AppConfig, PipelineOutput } from "./types";

type Page = "import" | "result" | "history" | "contexts" | "settings";

export default function App() {
  const [page, setPage] = useState<Page>("import");
  const [result, setResult] = useState<PipelineOutput | null>(null);
  const [config, setConfigState] = useState<AppConfig | null>(null);

  useEffect(() => {
    getConfig()
      .then(setConfigState)
      .catch(() => setConfigState(null));
  }, []);

  return (
    <div className="app">
      <header className="app-header">
        <span className="brand">TL;DW</span>
        <nav className="tabs">
          <button
            className={page === "import" ? "tab active" : "tab"}
            onClick={() => setPage("import")}
          >
            Nouveau
          </button>
          <button
            className={page === "result" ? "tab active" : "tab"}
            disabled={result === null}
            onClick={() => setPage("result")}
          >
            Résultat
          </button>
          <button
            className={page === "history" ? "tab active" : "tab"}
            onClick={() => setPage("history")}
          >
            Historique
          </button>
          <button
            className={page === "contexts" ? "tab active" : "tab"}
            onClick={() => setPage("contexts")}
          >
            Contextes
          </button>
          <button
            className={page === "settings" ? "tab active" : "tab"}
            onClick={() => setPage("settings")}
          >
            Modèles
          </button>
        </nav>
        <span className="app-sub">transcription + résumé · 100 % local</span>
      </header>
      <main className="app-main">
        {page === "import" && (
          <ImportPage
            config={config}
            onDone={(r) => {
              setResult(r);
              setPage("result");
            }}
            onGoContexts={() => setPage("contexts")}
          />
        )}
        {page === "result" && result && <ResultPage result={result} />}
        {page === "settings" && config && (
          <SettingsPage config={config} onChange={setConfigState} />
        )}
        {page === "history" && (
          <HistoryPage
            onOpen={(r) => {
              setResult(r);
              setPage("result");
            }}
          />
        )}
        {page === "contexts" && <ContextsPage />}
      </main>
    </div>
  );
}
