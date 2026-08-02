/** Forma mínima que tanto `QuerySummary` (query salva) quanto um objeto
 * sintético derivado de um `AdhocSlot` (ver Board.tsx) satisfazem — é o
 * que permite mesclar parâmetros de fontes salvas e ad-hoc sem duplicar
 * lógica. */
export interface HasParams {
  params: { name: string }[];
}

/** União dos nomes de parâmetro das queries selecionadas, sem repetição —
 * CLAUDE.md §6.1: "Parâmetros de mesmo nome em queries diferentes
 * compartilham o valor". Ordem alfabética para o formulário não pular de
 * lugar conforme a seleção muda. */
export function mergedParamNames(selected: HasParams[]): string[] {
  const names = new Set<string>();
  for (const query of selected) {
    for (const param of query.params) {
      names.add(param.name);
    }
  }
  return [...names].sort((a, b) => a.localeCompare(b));
}

/** Recorta do mapa compartilhado só os nomes que esta query declara —
 * é o que vai para `client.queryRun`/`client.queryRunAdhoc`. */
export function paramsFor(
  query: HasParams,
  values: Record<string, string>,
): Record<string, string> {
  const result: Record<string, string> = {};
  for (const param of query.params) {
    result[param.name] = values[param.name] ?? "";
  }
  return result;
}

/** Nomes de parâmetro desta query que estão sem valor preenchido — todo
 * parâmetro declarado é obrigatório na prática (tanto queries salvas
 * quanto blocos ad-hoc sempre marcam `required: true` na hora de
 * declarar). Rodar com um valor em branco não é erro do banco, é um
 * formulário incompleto — vale a pena travar antes de gastar uma
 * chamada IPC com uma mensagem clara em vez de deixar o driver rejeitar
 * com algo como "valor '' não é um inteiro válido". */
export function missingParamNames(query: HasParams, values: Record<string, string>): string[] {
  return query.params
    .map((p) => p.name)
    .filter((name) => (values[name] ?? "").trim().length === 0);
}
