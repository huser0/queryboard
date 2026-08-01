import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import App from "./App";
import { client } from "./ipc/client";

vi.mock("./ipc/client", () => ({
  client: {
    connectionList: vi.fn().mockResolvedValue([]),
    connectionCreate: vi.fn(),
    connectionDelete: vi.fn(),
    connectionTest: vi.fn(),
    queryList: vi.fn().mockResolvedValue([]),
    queryCreate: vi.fn(),
    queryExtractParams: vi.fn().mockResolvedValue([]),
    queryRun: vi.fn(),
    queryCancel: vi.fn(),
  },
}));

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
});
