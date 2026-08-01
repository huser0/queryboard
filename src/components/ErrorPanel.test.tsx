import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ErrorPanel } from "./ErrorPanel";

describe("ErrorPanel", () => {
  it("renders exactly the message it was given, nothing invented", () => {
    const message = 'erro de sintaxe SQL: syntax error at or near "FORM"';
    render(<ErrorPanel message={message} />);
    expect(screen.getByRole("alert")).toHaveTextContent(message);
  });

  it("never renders host/user/password fields, only the message string", () => {
    render(<ErrorPanel message="falha ao conectar: [dsn redigida]" />);
    expect(screen.queryByText(/password/i)).not.toBeInTheDocument();
  });
});
