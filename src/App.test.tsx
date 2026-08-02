import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import { client } from "./ipc/client";
import type { ConnectionSummary } from "./ipc/types";

vi.mock("./ipc/client", () => ({
  client: {
    connectionList: vi.fn().mockResolvedValue([]),
    connectionCreate: vi.fn(),
    connectionDelete: vi.fn(),
    connectionTest: vi.fn(),
    queryList: vi.fn().mockResolvedValue([]),
    queryCreate: vi.fn(),
    queryDelete: vi.fn(),
    queryExtractParams: vi.fn().mockResolvedValue([]),
    queryRun: vi.fn(),
    queryRunAdhoc: vi.fn(),
    queryCancel: vi.fn(),
    connectionSchema: vi.fn().mockResolvedValue([]),
  },
}));

const connA: ConnectionSummary = {
  id: "c1",
  slug: "queryboard_local",
  name: "Postgres Local",
  kind: "postgres",
  host: "localhost",
  port: 55432,
  database: "queryboard",
  service_name: null,
  username: "queryboard",
  max_rows: 1000,
  timeout_ms: 30000,
  created_at: "",
  updated_at: "",
};

describe("App", () => {
  it("renders the app shell and loads connections/queries from the backend", async () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "queryboard" })).toBeInTheDocument();
    await waitFor(() => {
      expect(client.connectionList).toHaveBeenCalledOnce();
      expect(client.queryList).toHaveBeenCalledOnce();
    });
  });

  it("shows an error panel when loading connections/queries fails", async () => {
    vi.mocked(client.connectionList).mockRejectedValueOnce(new Error("banco indisponível"));
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent("banco indisponível");
  });

  it("shows no active connection until one is picked in the sidebar, then marks it there and in the banner", async () => {
    const user = userEvent.setup();
    vi.mocked(client.connectionList).mockResolvedValueOnce([connA]);
    render(<App />);

    expect(await screen.findByText(/nenhuma connection selecionada/i)).toBeInTheDocument();

    const connectionButton = await screen.findByRole("button", {
      name: /postgres local.*queryboard_local/i,
    });
    await user.click(connectionButton);

    expect(connectionButton).toHaveAttribute("aria-current", "true");
    expect(
      screen.getByText(/conectado a: postgres local \(queryboard_local\)/i),
    ).toBeInTheDocument();
  });
});
