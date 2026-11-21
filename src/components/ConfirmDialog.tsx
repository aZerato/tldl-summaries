import { useEffect } from "react";

interface Props {
  /** Titre de la boîte (ex. « Supprimer de l'historique »). */
  title: string;
  /** Ce qui va être supprimé. */
  message: string;
  /** Libellé du bouton de confirmation (défaut « Supprimer »). */
  confirmLabel?: string;
  /** Exécute la suppression (la boîte est fermée par l'appelant). */
  onConfirm: () => void;
  /** Ferme sans supprimer (Annuler / Échap / clic sur le fond). */
  onCancel: () => void;
}

/** Boîte de confirmation in-app (suppressions) — remplace le `confirm`
 *  natif (pas fiable sous le webview Tauri 2) par une boîte au thème de
 *  l'app. Échap et clic sur le fond ferment sans confirmer ; le focus
 *  initial est sur « Annuler » (l'option sûre par défaut). */
export default function ConfirmDialog({
  title,
  message,
  confirmLabel = "Supprimer",
  onConfirm,
  onCancel,
}: Props) {
  // Échap = Annuler.
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onCancel();
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div
      className="confirm-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onCancel();
      }}
    >
      <div className="confirm-card">
        <h3>{title}</h3>
        <p>{message}</p>
        <div className="row">
          <button className="btn" autoFocus onClick={onCancel}>
            Annuler
          </button>
          <button className="btn danger" onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
