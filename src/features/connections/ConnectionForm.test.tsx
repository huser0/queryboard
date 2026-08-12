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

    await userEvent.click(screen.getByRole("button", { name: /salvar conexão/i }));

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
    await userEvent.click(screen.getByRole("button", { name: /salvar conexão/i }));

    expect(onSubmit).toHaveBeenCalledOnce();
    const submitted = onSubmit.mock.calls[0]?.[0] as { slug: string; password: string };
    expect(submitted.slug).toBe("erp_prod");
    expect(submitted.password).toBe("s3cr3t");
  });

  it("disables the submit button while submitting", () => {
    render(<ConnectionForm onSubmit={() => Promise.resolve()} submitting={true} />);
    expect(screen.getByRole("button", { name: /salvando/i })).toBeDisabled();
  });

  describe("editing an existing connection", () => {
    const existing = {
      id: "c1",
      slug: "erp_prod",
      name: "ERP produção",
      kind: "postgres" as const,
      host: "db.internal",
      port: 5432,
      database: "erp",
      service_name: null,
      username: "app_readonly",
      max_rows: 1000,
      timeout_ms: 30_000,
      created_at: "2026-01-01T00:00:00Z",
      updated_at: "2026-01-01T00:00:00Z",
    };

    it("pre-fills fields and keeps the slug read-only", () => {
      render(
        <ConnectionForm
          editing={existing}
          onSubmit={() => Promise.resolve()}
          onUpdate={() => Promise.resolve()}
          submitting={false}
        />,
      );

      expect(screen.getByLabelText("Slug")).toHaveValue("erp_prod");
      expect(screen.getByLabelText("Slug")).toBeDisabled();
      expect(screen.getByLabelText("Host")).toHaveValue("db.internal");
      expect(screen.getByLabelText("Senha")).not.toBeRequired();
    });

    it("submits without a password when the field is left blank, keeping the current secret", async () => {
      const onUpdate = vi.fn().mockResolvedValue(undefined);
      render(
        <ConnectionForm
          editing={existing}
          onSubmit={() => Promise.resolve()}
          onUpdate={onUpdate}
          submitting={false}
        />,
      );

      await userEvent.click(screen.getByRole("button", { name: /salvar alterações/i }));

      expect(onUpdate).toHaveBeenCalledOnce();
      const [slug, input] = onUpdate.mock.calls[0] as [string, { password?: string }];
      expect(slug).toBe("erp_prod");
      expect(input.password).toBeUndefined();
    });

    it("submits a new password when the field is filled in", async () => {
      const onUpdate = vi.fn().mockResolvedValue(undefined);
      render(
        <ConnectionForm
          editing={existing}
          onSubmit={() => Promise.resolve()}
          onUpdate={onUpdate}
          submitting={false}
        />,
      );

      await userEvent.type(screen.getByLabelText("Senha"), "nova-senha");
      await userEvent.click(screen.getByRole("button", { name: /salvar alterações/i }));

      const [, input] = onUpdate.mock.calls[0] as [string, { password?: string }];
      expect(input.password).toBe("nova-senha");
    });

    it("calls onCancel when the cancel button is clicked", async () => {
      const onCancel = vi.fn();
      render(
        <ConnectionForm
          editing={existing}
          onSubmit={() => Promise.resolve()}
          onUpdate={() => Promise.resolve()}
          onCancel={onCancel}
          submitting={false}
        />,
      );

      await userEvent.click(screen.getByRole("button", { name: /cancelar/i }));
      expect(onCancel).toHaveBeenCalledOnce();
    });
  });
});
