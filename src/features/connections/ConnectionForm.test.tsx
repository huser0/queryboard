import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConnectionForm } from "./ConnectionForm";

describe("ConnectionForm", () => {
  it("shows the read-only credential warning", () => {
    render(<ConnectionForm onSubmit={() => Promise.resolve()} submitting={false} />);
    expect(screen.getByRole("note")).toHaveTextContent(/somente leitura/i);
  });

  it("does not submit when required fields are empty", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<ConnectionForm onSubmit={onSubmit} submitting={false} />);

    await userEvent.click(screen.getByRole("button", { name: /salvar connection/i }));

    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("submits the filled-in connection with the password included", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<ConnectionForm onSubmit={onSubmit} submitting={false} />);

    await userEvent.type(screen.getByLabelText("Slug"), "erp_prod");
    await userEvent.type(screen.getByLabelText("Nome"), "ERP produção");
    await userEvent.type(screen.getByLabelText("Host"), "db.internal");
    await userEvent.type(screen.getByLabelText("Usuário"), "app_readonly");
    await userEvent.type(screen.getByLabelText("Senha"), "s3cr3t");
    await userEvent.click(screen.getByRole("button", { name: /salvar connection/i }));

    expect(onSubmit).toHaveBeenCalledOnce();
    const submitted = onSubmit.mock.calls[0]?.[0] as { slug: string; password: string };
    expect(submitted.slug).toBe("erp_prod");
    expect(submitted.password).toBe("s3cr3t");
  });

  it("disables the submit button while submitting", () => {
    render(<ConnectionForm onSubmit={() => Promise.resolve()} submitting={true} />);
    expect(screen.getByRole("button", { name: /salvando/i })).toBeDisabled();
  });
});
