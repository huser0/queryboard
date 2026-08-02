import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { QuerySelector } from "../QuerySelector";
import type { QuerySummary } from "../../../ipc/types";

function query(slug: string): QuerySummary {
  return {
    id: slug,
    slug,
    name: slug,
    connection_slug: "conn",
    sql: "SELECT 1",
    params: [],
    created_at: "",
    updated_at: "",
  };
}

describe("QuerySelector", () => {
  it("calls onChange with the updated slug array when toggled", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <QuerySelector
        queries={[query("a"), query("b")]}
        selectedSlugs={["a"]}
        runningSlugs={[]}
        onChange={onChange}
        onDelete={vi.fn()}
        onEdit={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("checkbox", { name: /b/i }));
    expect(onChange).toHaveBeenCalledWith(["a", "b"]);

    await user.click(screen.getByRole("checkbox", { name: /^a/i }));
    expect(onChange).toHaveBeenCalledWith([]);
  });

  it("disables the checkbox for a running query", () => {
    render(
      <QuerySelector
        queries={[query("a")]}
        selectedSlugs={["a"]}
        runningSlugs={["a"]}
        onChange={vi.fn()}
        onDelete={vi.fn()}
        onEdit={vi.fn()}
      />,
    );
    expect(screen.getByRole("checkbox", { name: /a/i })).toBeDisabled();
  });

  it("calls onDelete when Remover is clicked", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn().mockResolvedValue(undefined);
    render(
      <QuerySelector
        queries={[query("a")]}
        selectedSlugs={[]}
        runningSlugs={[]}
        onChange={vi.fn()}
        onDelete={onDelete}
        onEdit={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /remover/i }));
    expect(onDelete).toHaveBeenCalledWith("a");
  });

  it("shows an inline error when onDelete rejects", async () => {
    const user = userEvent.setup();
    const onDelete = vi.fn().mockRejectedValue(new Error("não é possível remover"));
    render(
      <QuerySelector
        queries={[query("a")]}
        selectedSlugs={[]}
        runningSlugs={[]}
        onChange={vi.fn()}
        onDelete={onDelete}
        onEdit={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("button", { name: /remover/i }));
    expect(await screen.findByRole("alert")).toHaveTextContent("não é possível remover");
  });

  it("calls onEdit with the full query when Editar is clicked", async () => {
    const user = userEvent.setup();
    const onEdit = vi.fn();
    const a = query("a");
    render(
      <QuerySelector
        queries={[a]}
        selectedSlugs={[]}
        runningSlugs={[]}
        onChange={vi.fn()}
        onDelete={vi.fn()}
        onEdit={onEdit}
      />,
    );

    await user.click(screen.getByRole("button", { name: /editar/i }));
    expect(onEdit).toHaveBeenCalledWith(a);
  });
});
