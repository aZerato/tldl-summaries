import { useEffect, useState } from "react";
import ImportPage from "./pages/ImportPage";
import ResultPage from "./pages/ResultPage";
import SettingsPage from "./pages/SettingsPage";
import HistoryPage from "./pages/HistoryPage";
import ContextsPage from "./pages/ContextsPage";
import { getConfig, toOutput } from "./api/tauri";
import type { AppConfig, PipelineOutput, ResultMeta } from "./types";

type Page = "import" | "result" | "history" | "contexts" | "settings";

export default function App() {
  const [page, setPage] = useState<Page>("import");
  const [result, setResult] = useState<PipelineOutput | null>(null);
  // F15/F16 : id + métadonnées de l'enregistrement de l'historique affiché —
  // re-résumé et résumés par contexte depuis le résultat (`null` = non
  // traçable).
  const [recordId, setRecordId] = useState<number | null>(null);
  const [resultMeta, setResultMeta] = useState<ResultMeta | null>(null);
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
            Configuration
          </button>
        </nav>
        <span className="app-sub">transcription + résumé · 100 % local</span>
      </header>
      <main className="app-main">
        {page === "import" && (
          <ImportPage
            config={config}
            onDone={(r, rec) => {
              setResult(r);
              setRecordId(rec ? rec.id : null);
              setResultMeta(
                rec
                  ? { contextName: rec.contextName, created: rec.created }
                  : null,
              );
              setPage("result");
            }}
            onGoContexts={() => setPage("contexts")}
            // F16 : redirection vers un résultat existant du fichier.
            onOpenExisting={(rec) => {
              setResult(toOutput(rec));
              setRecordId(rec.id);
              setResultMeta({
                contextName: rec.contextName,
                created: rec.created,
              });
              setPage("result");
            }}
          />
        )}
        {page === "result" && result && (
          <ResultPage
            result={result}
            recordId={recordId}
            resultMeta={resultMeta}
            onShown={(updated, rec) => {
              setResult(updated);
              setRecordId(rec ? rec.id : null);
              setResultMeta(
                rec
                  ? { contextName: rec.contextName, created: rec.created }
                  : null,
              );
            }}
          />
        )}
        {page === "settings" && config && (
          <SettingsPage config={config} onChange={setConfigState} />
        )}
        {page === "history" && (
          <HistoryPage
            onOpen={(r, rec) => {
              setResult(r);
              setRecordId(rec.id);
              setResultMeta({
                contextName: rec.contextName,
                created: rec.created,
              });
              setPage("result");
            }}
          />
        )}
        {page === "contexts" && <ContextsPage />}
      </main>
    </div>
  );
}
