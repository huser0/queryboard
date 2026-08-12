import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog } from "./ConfirmDialog";

describe("ConfirmDialog", () => {
  it("renders nothing when closed", () => {
    render(
      <ConfirmDialog
        open={false}
        title="Remover erp_prod?"
        description="Essa ação não pode ser desfeita."
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
  });

  it("shows the title/description and focuses the confirm button when open", () => {
    render(
      <ConfirmDialog
        open={true}
        title="Remover erp_prod?"
        description="Essa ação não pode ser desfeita."
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );
    const dialog = screen.getByRole("alertdialog", { name: /remover erp_prod\?/i });
    expect(dialog).toHaveTextContent("Essa ação não pode ser desfeita.");
    expect(screen.getByRole("button", { name: /remover/i })).toHaveFocus();
  });

  it("calls onConfirm when the confirm button is clicked", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    render(
      <ConfirmDialog
        open={true}
        title="t"
        description="d"
        onConfirm={onConfirm}
        onCancel={vi.fn()}
      />,
    );
    await user.click(screen.getByRole("button", { name: /remover/i }));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it("calls onCancel when Cancelar is clicked, when Escape is pressed, or when clicking the backdrop", async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    render(
      <ConfirmDialog open={true} title="t" description="d" onConfirm={vi.fn()} onCancel={onCancel} />,
    );

    await user.click(screen.getByRole("button", { name: /cancelar/i }));
    expect(onCancel).toHaveBeenCalledTimes(1);

    await user.keyboard("{Escape}");
    expect(onCancel).toHaveBeenCalledTimes(2);
  });

  it("does not call onCancel when clicking inside the dialog content", async () => {
    const user = userEvent.setup();
    const onCancel = vi.fn();
    render(
      <ConfirmDialog open={true} title="t" description="d" onConfirm={vi.fn()} onCancel={onCancel} />,
    );
    await user.click(screen.getByText("d"));
    expect(onCancel).not.toHaveBeenCalled();
  });
});
