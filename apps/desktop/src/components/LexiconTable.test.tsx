import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { LexiconEntry } from "../contracts";
import { LexiconTable } from "./LexiconTable";

describe("LexiconTable", () => {
  it("only mounts a bounded window for a large lexicon", () => {
    const entries: LexiconEntry[] = Array.from({ length: 1_000 }, (_, index) => ({
      name: `sign_${index}`,
      categories: ["Noun"],
      underlying_form: `/ka${index}/`,
      gloss: `ITEM ${index}`,
      senses: [],
      dimensions: [],
    }));

    render(<LexiconTable entries={entries} />);

    expect(screen.getByText("sign_0")).toBeInTheDocument();
    expect(screen.queryByText("sign_999")).not.toBeInTheDocument();
    expect(screen.getAllByRole("row").length).toBeLessThan(60);
  });
});
