import type { CellValue } from "../ipc/client";

/** Insere separador de milhar (".") e troca o ponto decimal por vírgula
 * (pt-BR) — sempre por manipulação de string, nunca `Number()`. Isso vale
 * pra Decimal (que já é string canônica — CLAUDE.md D6) e também pra
 * Int/Float: evitar o Number roundtrip aqui garante que um valor com mais
 * dígitos do que um f64/i32 representa fielmente nunca é arredondado só
 * porque virou pt-BR. */
export function formatNumberString(raw: string): string {
  const negative = raw.startsWith("-");
  const unsigned = negative ? raw.slice(1) : raw;
  const [intPart, fracPart] = unsigned.split(".");
  const grouped = (intPart ?? "").replace(/\B(?=(\d{3})+(?!\d))/g, ".");
  const sign = negative ? "-" : "";
  return fracPart !== undefined ? `${sign}${grouped},${fracPart}` : `${sign}${grouped}`;
}

const DATE_RE = /^(\d{4})-(\d{2})-(\d{2})/;
const TIME_RE = /T(\d{2}):(\d{2}):(\d{2})/;

/** "2022-03-01" -> "01/03/2022". Faz o parse por regex sobre a string
 * ISO, nunca via `new Date(raw)` — um `Date` reinterpretaria a string sob
 * o fuso horário do navegador, o que é errado aqui: essas datas/horas são
 * valores "de parede" do banco, sem fuso (ou, no caso de TimestampTz, com
 * o offset já embutido no campo do driver), não timestamps UTC. */
export function formatDateOnly(raw: string): string {
  const m = DATE_RE.exec(raw);
  if (m === null) {
    return raw;
  }
  const [, y, mo, d] = m;
  return `${d ?? ""}/${mo ?? ""}/${y ?? ""}`;
}

/** "2022-03-01T14:30:00..." -> "01/03/2022 14:30"; omite a hora quando é
 * meia-noite exata (00:00:00) — caso comum pra colunas DATE que na
 * prática só carregam o dia. */
export function formatTimestamp(raw: string): string {
  const dateMatch = DATE_RE.exec(raw);
  if (dateMatch === null) {
    return raw;
  }
  const [, y, mo, d] = dateMatch;
  const datePart = `${d ?? ""}/${mo ?? ""}/${y ?? ""}`;
  const timeMatch = TIME_RE.exec(raw);
  if (timeMatch === null) {
    return datePart;
  }
  const [, hh, mm, ss] = timeMatch;
  if (hh === "00" && mm === "00" && ss === "00") {
    return datePart;
  }
  return `${datePart} ${hh ?? ""}:${mm ?? ""}`;
}

export interface FormattedCell {
  /** Texto pronto pra exibir na célula. */
  text: string;
  /** true só para CellValue::Null — controla o marcador visual "—". */
  isNull: boolean;
  /** Valor bruto (não formatado) pra `title`, então o dado exato
   * continua acessível (ex. copiar/inspecionar) mesmo com a célula
   * mostrando a versão formatada. */
  raw: string;
}

export function formatCell(cell: CellValue): FormattedCell {
  if (cell.type === "Null") {
    return { text: "—", isNull: true, raw: "" };
  }
  if (cell.type === "Bool") {
    const text = cell.value ? "true" : "false";
    return { text, isNull: false, raw: text };
  }
  if (cell.type === "Bytes") {
    const text = `<${cell.value.length.toString()} bytes>`;
    return { text, isNull: false, raw: text };
  }
  if (cell.type === "Int") {
    const raw = String(cell.value);
    return { text: formatNumberString(raw), isNull: false, raw };
  }
  if (cell.type === "Decimal") {
    return { text: formatNumberString(cell.value), isNull: false, raw: cell.value };
  }
  if (cell.type === "Float") {
    const raw = String(cell.value);
    return { text: formatNumberString(raw), isNull: false, raw };
  }
  if (cell.type === "Date") {
    return { text: formatDateOnly(cell.value), isNull: false, raw: cell.value };
  }
  if (cell.type === "Timestamp" || cell.type === "TimestampTz") {
    return { text: formatTimestamp(cell.value), isNull: false, raw: cell.value };
  }
  const text = cell.value;
  return { text, isNull: false, raw: text };
}

// Decimal nunca deve virar Number() — CLAUDE.md D6, string canônica.
export function isNumericLike(cell: CellValue): boolean {
  return cell.type === "Int" || cell.type === "Decimal" || cell.type === "Float";
}
