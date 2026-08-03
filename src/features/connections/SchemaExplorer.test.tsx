import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SchemaExplorer } from "./SchemaExplorer";
import { client } from "../../ipc/client";

vi.mock("../../ipc/client", () => ({
  client: {
    connectionSchema: vi.fn(),
  },
}));

describe("SchemaExplorer", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders nothing when no connection is active", () => {
    const { container } = render(<SchemaExplorer connectionSlug={null} />);
    expect(container).toBeEmptyDOMElement();
    expect(client.connectionSchema).not.toHaveBeenCalled();
  });

  it("fetches and lists tables for the active connection, columns collapsed by default", async () => {
    vi.mocked(client.connectionSchema).mockResolvedValue([
      { name: "offers", columns: [{ name: "offer_id", data_type: "integer" }] },
      { name: "products", columns: [{ name: "product_id", data_type: "integer" }] },
    ]);

    render(<SchemaExplorer connectionSlug="queryboard_local" />);

    expect(await screen.findByText("offers")).toBeInTheDocument();
    expect(screen.getByText("products")).toBeInTheDocument();
    expect(client.connectionSchema).toHaveBeenCalledWith("queryboard_local");
    expect(screen.queryByText("offer_id")).not.toBeInTheDocument();
  });

  it("expands a table to show its columns when clicked", async () => {
    const user = userEvent.setup();
    vi.mocked(client.connectionSchema).mockResolvedValue([
      { name: "offers", columns: [{ name: "offer_id", data_type: "integer" }] },
    ]);

    render(<SchemaExplorer connectionSlug="queryboard_local" />);

    const tableToggle = await screen.findByRole("button", { name: /offers/i });
    await user.click(tableToggle);

    expect(screen.getByText("offer_id")).toBeInTheDocument();
    expect(screen.getByText("integer")).toBeInTheDocument();
  });

  it("shows an inline error when the schema fetch rejects", async () => {
    vi.mocked(client.connectionSchema).mockRejectedValue(new Error("conexão indisponível"));

    render(<SchemaExplorer connectionSlug="queryboard_local" />);

    expect(await screen.findByRole("alert")).toHaveTextContent("conexão indisponível");
  });
});
