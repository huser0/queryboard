import { forwardRef, useImperativeHandle, useRef } from "react";
import CodeMirror, { type ReactCodeMirrorRef } from "@uiw/react-codemirror";
import { sql as sqlLang } from "@codemirror/lang-sql";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

// Sem isto o editor tokeniza SQL (via `sql()`) mas não pinta nada — sem
// tema aplicado, tudo sai em preto/cinza uniforme. Cores tiradas da
// própria paleta do app (App.css) em vez de um tema pronto de terceiro,
// pra não puxar dependência nova só por causa de cor.
const sqlHighlightStyle = HighlightStyle.define([
  { tag: t.keyword, color: "#a626a4", fontWeight: 600 },
  { tag: [t.name, t.propertyName], color: "#0f0f0f" },
  { tag: [t.string, t.special(t.string)], color: "#1a7f37" },
  { tag: [t.number, t.bool, t.null], color: "#c2410c" },
  { tag: t.comment, color: "#8b96a3", fontStyle: "italic" },
  { tag: [t.operator, t.punctuation], color: "#55606b" },
  { tag: t.paren, color: "#55606b" },
]);

interface QueryEditorProps {
  value: string;
  onChange: (value: string) => void;
  error: string | null;
}

export interface QueryEditorHandle {
  /** Insere `text` na posição do cursor (substituindo a seleção, se
   * houver) e devolve o foco ao editor — usado pelo SchemaExplorer pra
   * inserir nome de tabela/coluna clicado sem o usuário precisar
   * clicar de volta no editor primeiro. */
  insertAtCursor: (text: string) => void;
}

export const QueryEditor = forwardRef<QueryEditorHandle, QueryEditorProps>(
  function QueryEditor({ value, onChange, error }, ref) {
    const cmRef = useRef<ReactCodeMirrorRef>(null);

    useImperativeHandle(
      ref,
      () => ({
        insertAtCursor(text: string) {
          const view = cmRef.current?.view;
          if (view === undefined) {
            return;
          }
          const { from, to } = view.state.selection.main;
          view.dispatch({
            changes: { from, to, insert: text },
            selection: { anchor: from + text.length },
          });
          view.focus();
        },
      }),
      [],
    );

    return (
      <div>
        <CodeMirror
          ref={cmRef}
          value={value}
          height="200px"
          extensions={[sqlLang(), syntaxHighlighting(sqlHighlightStyle)]}
          onChange={onChange}
          aria-label="Editor de SQL"
        />
        {error !== null && <div role="alert">{error}</div>}
      </div>
    );
  },
);
