import { useEffect, useId, useRef } from "react";

interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
}

/** Diálogo de confirmação genérico — toda remoção (connection, query
 * salva) passa por aqui em vez de agir direto no clique. O vermelho fica
 * só no botão de confirmar: é o único lugar da UI onde essa cor deveria
 * aparecer fora de um indicador de erro (ver "hierarquia de ações"). Não
 * usa <dialog>/showModal nativo porque o ambiente de teste (jsdom) não
 * implementa esses métodos — um overlay controlado dá o mesmo
 * comportamento (foco no botão de confirmar, Escape fecha) sem depender
 * disso. */
export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = "Remover",
  onConfirm,
  onCancel,
}: ConfirmDialogProps) {
  const confirmRef = useRef<HTMLButtonElement>(null);
  const titleId = useId();
  const descriptionId = useId();

  useEffect(() => {
    if (open) {
      confirmRef.current?.focus();
    }
  }, [open]);

  useEffect(() => {
    if (!open) {
      return;
    }
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        onCancel();
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => { document.removeEventListener("keydown", onKeyDown); };
  }, [open, onCancel]);

  if (!open) {
    return null;
  }

  return (
    <div className="confirm-dialog-backdrop" onClick={onCancel}>
      <div
        className="confirm-dialog"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={descriptionId}
        onClick={(e) => { e.stopPropagation(); }}
      >
        <h3 id={titleId}>{title}</h3>
        <p id={descriptionId}>{description}</p>
        <div className="confirm-dialog__actions">
          <button type="button" onClick={onCancel}>
            Cancelar
          </button>
          <button type="button" data-variant="danger" ref={confirmRef} onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
