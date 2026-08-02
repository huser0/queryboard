interface RunAllBarProps {
  disabled: boolean;
  anyRunning: boolean;
  onRunAll: () => void;
  onCancelAll: () => void;
  summary: { total: number; running: number; ok: number; error: number; cancelled: number };
}

export function RunAllBar({ disabled, anyRunning, onRunAll, onCancelAll, summary }: RunAllBarProps) {
  return (
    <div aria-label="Barra de execução do painel">
      <button
        type="button"
        data-variant="primary"
        className="run-button"
        onClick={onRunAll}
        disabled={disabled || anyRunning}
      >
        <span aria-hidden="true">▶</span> Executar tudo
      </button>
      <button type="button" onClick={onCancelAll} disabled={!anyRunning}>
        Cancelar tudo
      </button>
      {summary.total > 0 && (
        <span role="status" className="run-summary">
          {summary.total} selecionada(s) · {summary.running} rodando · {summary.ok} ok ·{" "}
          {summary.error} erro · {summary.cancelled} cancelada(s)
        </span>
      )}
    </div>
  );
}
