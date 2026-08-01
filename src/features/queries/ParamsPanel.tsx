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
    <fieldset aria-label="Parâmetros">
      <legend>Parâmetros</legend>
      {paramNames.map((name) => (
        <label key={name}>
          {name}
          <input
            value={values[name] ?? ""}
            onChange={(e) => { onChange(name, e.currentTarget.value); }}
          />
        </label>
      ))}
    </fieldset>
  );
}
