export type RunStatus = "idle" | "running" | "ok" | "cancelled" | "error";

interface RunBarProps {
  status: RunStatus;
  onRun: () => void;
  onCancel: () => void;
  rowCount: number | null;
  elapsedMs: number | null;
  truncated: boolean;
}

export function RunBar({
  status,
  onRun,
  onCancel,
  rowCount,
  elapsedMs,
  truncated,
}: RunBarProps) {
  return (
    <div aria-label="Barra de execução">
      <button type="button" onClick={onRun} disabled={status === "running"}>
        Executar
      </button>
      <button type="button" onClick={onCancel} disabled={status !== "running"}>
        Cancelar
      </button>
      <span role="status">
        {status === "idle" && "pronto"}
        {status === "running" && "executando…"}
        {status === "ok" && "concluído"}
        {status === "cancelled" && "cancelado"}
        {status === "error" && "erro"}
      </span>
      {rowCount !== null && elapsedMs !== null && (
        <span>
          {rowCount} linha(s) em {elapsedMs}ms
        </span>
      )}
      {truncated && (
        <span role="alert">
          resultado truncado — mais linhas existem além do limite configurado
        </span>
      )}
    </div>
  );
}
