import { describe, expect, it } from "vitest";
import {
  evolutionTreeSchema,
  packageCatalogSchema,
  pendingChangeSchema,
  projectSummarySchema,
  sourceReconcileSchema,
  UI_SCHEMA,
  weightConfigSchema,
} from "./contracts";

describe("conlang.ui/v1 contract", () => {
  it("accepts a pinned project summary", () => {
    expect(projectSummarySchema.parse({
      schema: UI_SCHEMA,
      path: "C:/langcraft/demo",
      name: "Demo",
      legacy: false,
      graph_dirty: false,
      has_pending: false,
      node_count: 1,
      active: "abc",
      packages: ["std:core"],
    }).name).toBe("Demo");
  });

  it("rejects unknown fields instead of silently drifting", () => {
    const result = evolutionTreeSchema.safeParse({ schema: UI_SCHEMA, nodes: [], childrn: [] });
    expect(result.success).toBe(false);
  });

  it("fails hard on an incompatible schema", () => {
    const result = evolutionTreeSchema.safeParse({ schema: "conlang.ui/v2", nodes: [] });
    expect(result.success).toBe(false);
  });

  it("pins the pending preview shape", () => {
    const result = pendingChangeSchema.parse({
      schema: UI_SCHEMA,
      source: "schema conlang.chg/v1\n",
      statements: 0,
      diff: { aligned: 1, born: 0, died: 0, phon: 0, syn: 0, sem: 0, prag: 0, structural: 0 },
    });
    expect(result.diff.aligned).toBe(1);
  });

  it("pins the expert source reconcile report and nested pending contract", () => {
    const result = sourceReconcileSchema.parse({
      schema: UI_SCHEMA,
      matched: 4,
      inserted: 1,
      deleted: 0,
      primitive_edits: 2,
      pending: {
        schema: UI_SCHEMA,
        source: "schema conlang.chg/v1\n",
        statements: 2,
        diff: { aligned: 4, born: 1, died: 0, phon: 0, syn: 0, sem: 1, prag: 0, structural: 1 },
      },
    });
    expect(result.pending.statements).toBe(2);
  });

  it("pins package source and declaration state", () => {
    const result = packageCatalogSchema.parse({
      schema: UI_SCHEMA,
      packages: [{
        id: "std:core",
        kind: "std",
        version: "1",
        source: "embedded",
        enabled: true,
        declared: true,
        selected: true,
        requires: [],
      }],
    });
    expect(result.packages[0]?.source).toBe("embedded");
  });

  it("pins effective weight provenance", () => {
    const result = weightConfigSchema.parse({
      schema: UI_SCHEMA,
      declaration_source: "project.toml:weights",
      manual: [{ segment: "k", weight: 0.8, source: "manual" }],
      effective: [
        { segment: "k", weight: 0.8, source: "manual" },
        { segment: "a", weight: 0.5, source: "prior" },
      ],
    });
    expect(result.effective.map((item) => item.source)).toEqual(["manual", "prior"]);
  });
});
