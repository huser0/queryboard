import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { RunBar } from "./RunBar";

describe("RunBar", () => {
  it("disables cancel until a run is in progress", () => {
    render(
      <RunBar
        status="idle"
        onRun={() => undefined}
        onCancel={() => undefined}
        rowCount={null}
        elapsedMs={null}
        truncated={false}
      />,
    );
    expect(screen.getByRole("button", { name: "Cancelar" })).toBeDisabled();
  });

  it("enables cancel while running and calls onCancel when clicked", async () => {
    const onCancel = vi.fn();
    render(
      <RunBar
        status="running"
        onRun={() => undefined}
        onCancel={onCancel}
        rowCount={null}
        elapsedMs={null}
        truncated={false}
      />,
    );
    const cancelButton = screen.getByRole("button", { name: "Cancelar" });
    expect(cancelButton).toBeEnabled();
    await userEvent.click(cancelButton);
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it("shows the truncated banner only when the result was truncated", () => {
    const { rerender } = render(
      <RunBar
        status="ok"
        onRun={() => undefined}
        onCancel={() => undefined}
        rowCount={1000}
        elapsedMs={42}
        truncated={false}
      />,
    );
    expect(screen.queryByText(/truncado/)).not.toBeInTheDocument();

    rerender(
      <RunBar
        status="ok"
        onRun={() => undefined}
        onCancel={() => undefined}
        rowCount={1000}
        elapsedMs={42}
        truncated={true}
      />,
    );
    expect(screen.getByText(/truncado/)).toBeInTheDocument();
  });

  it("reflects the cancelled status", () => {
    render(
      <RunBar
        status="cancelled"
        onRun={() => undefined}
        onCancel={() => undefined}
        rowCount={null}
        elapsedMs={null}
        truncated={false}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent("cancelado");
  });
});
