import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { AdhocTabs } from "../AdhocTabs";
import { IDLE_STATE, type AdhocSlot, type QueryExecutionState } from "../types";

const slotA: AdhocSlot = {
  id: "a",
  connectionSlug: "conn",
  sql: "select * from offers",
  paramNames: [],
  savedAsSlug: null,
};

const slotB: AdhocSlot = {
  id: "b",
  connectionSlug: "conn",
  sql: "",
  paramNames: [],
  savedAsSlug: "consulta_salva",
};

describe("AdhocTabs", () => {
  it("shows one tab per slot, marks the active one, and calls onSelect when another is clicked", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();

    render(
      <AdhocTabs
        slots={[slotA, slotB]}
        activeId="a"
        executions={{}}
        onSelect={onSelect}
        onClose={() => { /* noop */ }}
        onReorder={() => { /* noop */ }}
        onAddNew={() => { /* noop */ }}
      />,
    );

    const tabA = screen.getByRole("tab", { name: /select \* from offers/i });
    const tabB = screen.getByRole("tab", { name: /sql ad-hoc/i });
    expect(tabA).toHaveAttribute("aria-selected", "true");
    expect(tabB).toHaveAttribute("aria-selected", "false");

    await user.click(tabB);
    expect(onSelect).toHaveBeenCalledWith("b");
  });

  it("shows a 'salvo' badge only for a tab backed by a saved query", () => {
    render(
      <AdhocTabs
        slots={[slotA, slotB]}
        activeId="a"
        executions={{}}
        onSelect={() => { /* noop */ }}
        onClose={() => { /* noop */ }}
        onReorder={() => { /* noop */ }}
        onAddNew={() => { /* noop */ }}
      />,
    );

    expect(screen.getByText("salvo")).toBeInTheDocument();
  });

  it("calls onClose with the tab's id when its × is clicked, not onSelect", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    const onClose = vi.fn();

    render(
      <AdhocTabs
        slots={[slotA]}
        activeId="a"
        executions={{}}
        onSelect={onSelect}
        onClose={onClose}
        onReorder={() => { /* noop */ }}
        onAddNew={() => { /* noop */ }}
      />,
    );

    await user.click(screen.getByRole("button", { name: /fechar select \* from offers/i }));
    expect(onClose).toHaveBeenCalledWith("a");
    expect(onSelect).not.toHaveBeenCalled();
  });

  it("calls onAddNew when the + tab is clicked", async () => {
    const user = userEvent.setup();
    const onAddNew = vi.fn();

    render(
      <AdhocTabs
        slots={[]}
        activeId={null}
        executions={{}}
        onSelect={() => { /* noop */ }}
        onClose={() => { /* noop */ }}
        onReorder={() => { /* noop */ }}
        onAddNew={onAddNew}
      />,
    );

    await user.click(screen.getByRole("button", { name: /adicionar sql ad-hoc/i }));
    expect(onAddNew).toHaveBeenCalledOnce();
  });

  it("reflects the running status as a status dot on the tab", () => {
    const executions: Record<string, QueryExecutionState> = {
      a: { ...IDLE_STATE, status: "running" },
    };

    render(
      <AdhocTabs
        slots={[slotA]}
        activeId="a"
        executions={executions}
        onSelect={() => { /* noop */ }}
        onClose={() => { /* noop */ }}
        onReorder={() => { /* noop */ }}
        onAddNew={() => { /* noop */ }}
      />,
    );

    // O dot é puramente visual (aria-hidden), sem role pra consultar via
    // testing-library — busca direta no DOM é o único jeito de checar.
    const dot = document.querySelector(".status-dot");
    expect(dot).toHaveAttribute("data-status", "running");
  });
});
