import { describe, expect, it } from "vitest";
import type { CatalogPackage } from "../contracts";
import { exactPackageSelection } from "./SettingsPage";

describe("package-loader v2 settings selection", () => {
  it("pins arbitrary declared namespaces without promoting transitive packages", () => {
    const packages: CatalogPackage[] = [
      {
        id: "catalog:traditional-case",
        kind: "catalog",
        version: "1.2.3",
        source: "vendored",
        enabled: true,
        declared: true,
        selected: true,
        requires: ["dataset:grambank@2026.1"],
      },
      {
        id: "dataset:grambank",
        kind: "dataset",
        version: "2026.1",
        source: "installed",
        enabled: true,
        declared: false,
        selected: true,
        requires: [],
      },
      {
        id: "theory:cxg",
        kind: "theory",
        version: "4",
        source: "embedded",
        enabled: true,
        declared: true,
        selected: true,
        requires: [],
      },
    ];

    const selection = exactPackageSelection(
      packages,
      new Set(["catalog:traditional-case", "theory:cxg"]),
    );

    expect(selection).toEqual({
      roots: ["catalog:traditional-case@1.2.3", "theory:cxg@4"],
    });
  });
});
