import "@testing-library/jest-dom/vitest";

// jsdom nunca faz layout de verdade — offsetHeight/offsetWidth ficam
// sempre em 0, o que faz @tanstack/react-virtual calcular uma janela
// visível vazia (nenhuma linha "cabe" numa viewport de 0px). Sem isso,
// nenhum teste de grade virtualizada consegue ver uma linha sequer.
Object.defineProperty(HTMLElement.prototype, "offsetHeight", {
  configurable: true,
  value: 480,
});
Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
  configurable: true,
  value: 800,
});
