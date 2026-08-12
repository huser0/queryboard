import { Fragment, type ReactNode } from "react";
import { pluralize } from "../../components/pluralize";

interface RunAllBarProps {
  disabled: boolean;
  anyRunning: boolean;
  onRunAll: () => void;
  onCancelAll: () => void;
  summary: { total: number; running: number; ok: number; error: number; cancelled: number };
}

export function RunAllBar({ disabled, anyRunning, onRunAll, onCancelAll, summary }: RunAllBarProps) {
  const segments: ReactNode[] = [
    `${summary.total.toString()} ${pluralize(summary.total, "query selecionada", "queries selecionadas")}`,
  ];
  if (summary.running > 0) {
    segments.push(`${summary.running.toString()} rodando`);
  }
  if (summary.ok > 0) {
    segments.push(`${summary.ok.toString()} ok`);
  }
  if (summary.error > 0) {
    segments.push(
      <span key="error" className="run-summary__error">
        {summary.error} {pluralize(summary.error, "erro", "erros")}
      </span>,
    );
  }
  if (summary.cancelled > 0) {
    segments.push(
      `${summary.cancelled.toString()} ${pluralize(summary.cancelled, "cancelada", "canceladas")}`,
    );
  }

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
          {segments.map((segment, index) => (
            <Fragment key={index}>
              {index > 0 && " · "}
              {segment}
            </Fragment>
          ))}
        </span>
      )}
    </div>
  );
}
