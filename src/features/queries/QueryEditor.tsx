import CodeMirror from "@uiw/react-codemirror";
import { sql as sqlLang } from "@codemirror/lang-sql";

interface QueryEditorProps {
  value: string;
  onChange: (value: string) => void;
  error: string | null;
}

export function QueryEditor({ value, onChange, error }: QueryEditorProps) {
  return (
    <div>
      <CodeMirror
        value={value}
        height="200px"
        extensions={[sqlLang()]}
        onChange={onChange}
        aria-label="Editor de SQL"
      />
      {error !== null && <div role="alert">{error}</div>}
    </div>
  );
}
