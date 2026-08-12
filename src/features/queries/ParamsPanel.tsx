interface ParamsPanelProps {
  paramNames: string[];
  values: Record<string, string>;
  onChange: (name: string, value: string) => void;
}

export function ParamsPanel({ paramNames, values, onChange }: ParamsPanelProps) {
  if (paramNames.length === 0) {
    return null;
  }

  return (
    <div className="params-panel" role="group" aria-label="Parâmetros">
      <h3 className="section-label">Parâmetros</h3>
      <div className="params-panel__fields">
        {paramNames.map((name) => (
          <label key={name} data-param={name}>
            {name}
            <input
              value={values[name] ?? ""}
              onChange={(e) => { onChange(name, e.currentTarget.value); }}
            />
          </label>
        ))}
      </div>
    </div>
  );
}
